//! `ost` — the open-subtitle CLI and the sidecar the mpv plugin drives.
//!
//! Keyless by default. Every subcommand supports `--json` (the sidecar contract).

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use os_compose::build_engine;
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::{Language, MediaKind};
use os_engine::library;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "ost",
    version,
    about = "open-subtitle: find & download subtitles, keyless by default"
)]
struct Cli {
    /// Path to a config file (defaults to the XDG location).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Verbose logging to stderr.
    #[arg(long, short, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create the default config file.
    Init,
    /// Show the resolved config.
    Config,
    /// List wired providers and any throttle state.
    Providers,
    /// Identify media from a file/name/query (prints Media).
    Identify(IdentifyArgs),
    /// Search providers and print scored candidates.
    Search(SearchArgs),
    /// Download the best subtitle(s) and write them next to the video / to --out.
    Get(GetArgs),
    /// One-shot: identify → download → sync → (transcribe fallback).
    Auto(GetArgs),
    /// Scan a library directory and fetch subtitles for videos missing them.
    Scan(ScanArgs),
    /// Sync an existing subtitle file to a video (needs a sync backend).
    Sync(SyncArgs),
    /// Translate an existing subtitle file to a target language.
    Translate(TranslateArgs),
}

#[derive(Parser)]
struct IdentifyArgs {
    /// A video file path, a release name, or a free-text title.
    input: String,
    #[arg(long)]
    season: Option<u32>,
    #[arg(long)]
    episode: Option<u32>,
    /// Force media kind: movie | series | anime.
    #[arg(long)]
    kind: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
struct SearchArgs {
    input: String,
    /// Comma-separated language preference, e.g. `en,es` (defaults to config).
    #[arg(long, short = 'l')]
    langs: Option<String>,
    #[arg(long)]
    season: Option<u32>,
    #[arg(long)]
    episode: Option<u32>,
    #[arg(long)]
    kind: Option<String>,
    /// Limit the number of results shown.
    #[arg(long, default_value_t = 15)]
    limit: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
struct GetArgs {
    input: String,
    #[arg(long, short = 'l')]
    langs: Option<String>,
    #[arg(long)]
    season: Option<u32>,
    #[arg(long)]
    episode: Option<u32>,
    #[arg(long)]
    kind: Option<String>,
    /// Output directory (defaults to the video's directory or the CWD).
    #[arg(long, short = 'o')]
    out: Option<PathBuf>,
    /// Strip hearing-impaired cues.
    #[arg(long)]
    hi: bool,
    /// Sync the downloaded subtitle to the video (requires a sync backend + a file path).
    #[arg(long)]
    sync: bool,
    /// Also translate the result into this language (requires a translate backend).
    #[arg(long)]
    translate: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
struct ScanArgs {
    /// Library directory to walk.
    dir: PathBuf,
    /// Comma-separated language preference, e.g. `en,es` (defaults to config).
    #[arg(long, short = 'l')]
    langs: Option<String>,
    /// Don't descend into subdirectories.
    #[arg(long)]
    no_recursive: bool,
    /// Strip hearing-impaired cues from downloaded subtitles.
    #[arg(long)]
    hi: bool,
    /// Only report which videos are missing subtitles; download nothing.
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
struct SyncArgs {
    /// Subtitle file to sync.
    subtitle: PathBuf,
    /// Reference video file.
    video: PathBuf,
    /// Output path (defaults to overwriting the input).
    #[arg(long, short = 'o')]
    out: Option<PathBuf>,
}

#[derive(Parser)]
struct TranslateArgs {
    /// Subtitle file to translate.
    subtitle: PathBuf,
    /// Target language, e.g. `es`.
    #[arg(long, short = 't')]
    to: String,
    /// Output path (defaults to `<stem>.<lang>.srt` next to the input).
    #[arg(long, short = 'o')]
    out: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "os_engine=debug,os_providers=debug".into()),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    let config_path = match &cli.config {
        Some(p) => p.clone(),
        None => Config::default_path().context("resolving config path")?,
    };

    match cli.command {
        Command::Init => cmd_init(&config_path),
        Command::Config => cmd_config(&config_path),
        Command::Providers => cmd_providers(&config_path),
        Command::Identify(args) => cmd_identify(&config_path, args).await,
        Command::Search(args) => cmd_search(&config_path, args).await,
        Command::Get(args) => cmd_get(&config_path, args, false).await,
        Command::Auto(args) => cmd_get(&config_path, args, true).await,
        Command::Scan(args) => cmd_scan(&config_path, args).await,
        Command::Sync(args) => cmd_sync(&config_path, args).await,
        Command::Translate(args) => cmd_translate(&config_path, args).await,
    }
}

fn parse_kind(s: &Option<String>) -> Option<MediaKind> {
    match s.as_deref() {
        Some("movie") => Some(MediaKind::Movie),
        Some("series") | Some("tv") => Some(MediaKind::Series),
        Some("anime") => Some(MediaKind::Anime),
        _ => None,
    }
}

fn parse_langs(spec: &Option<String>, cfg: &Config) -> Vec<Language> {
    match spec {
        Some(s) => s
            .split(',')
            .filter_map(|c| Language::parse(c.trim()))
            .collect(),
        None => cfg.languages(),
    }
}

fn make_input(
    input: &str,
    season: Option<u32>,
    episode: Option<u32>,
    kind: Option<MediaKind>,
) -> MediaInput {
    let p = Path::new(input);
    if p.is_file() {
        let name = p.file_name().map(|s| s.to_string_lossy().into_owned());
        MediaInput {
            path: Some(p.to_path_buf()),
            name,
            query: None,
            kind_hint: kind,
            season,
            episode,
        }
    } else {
        MediaInput {
            path: None,
            name: Some(input.to_string()),
            query: None,
            kind_hint: kind,
            season,
            episode,
        }
    }
}

fn cmd_init(path: &Path) -> Result<()> {
    if path.exists() {
        println!("config already exists: {}", path.display());
        return Ok(());
    }
    let cfg = Config::default();
    cfg.save(path).context("writing config")?;
    println!("wrote default config: {}", path.display());
    println!("keyless providers are enabled by default — no API key needed.");
    Ok(())
}

fn cmd_config(path: &Path) -> Result<()> {
    let cfg = Config::load(path).context("loading config")?;
    println!("config: {}", path.display());
    println!("{}", toml::to_string_pretty(&cfg).unwrap_or_default());
    Ok(())
}

fn cmd_providers(path: &Path) -> Result<()> {
    let cfg = Config::load(path)?;
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    println!("wired providers:");
    for name in engine.provider_names() {
        let throttle = engine
            .throttler()
            .throttled_for(&name)
            .map(|d| format!("  (throttled {}s)", d.as_secs()))
            .unwrap_or_default();
        println!("  - {name}{throttle}");
    }
    Ok(())
}

async fn cmd_identify(path: &Path, args: IdentifyArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    let input = make_input(
        &args.input,
        args.season,
        args.episode,
        parse_kind(&args.kind),
    );
    let media = engine.identify(&input).await.map_err(anyhow::Error::msg)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&media)?);
    } else {
        println!("{}", media.label());
        println!("  kind:   {:?}", media.kind);
        if let Some(y) = media.year {
            println!("  year:   {y}");
        }
        if !media.hashes.is_empty() {
            println!("  hashes: {:?}", media.hashes);
        }
        if media.ids.anilist.is_some() || media.ids.imdb.is_some() {
            println!("  ids:    {:?}", media.ids);
        }
    }
    Ok(())
}

async fn cmd_search(path: &Path, args: SearchArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let langs = parse_langs(&args.langs, &cfg);
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    let input = make_input(
        &args.input,
        args.season,
        args.episode,
        parse_kind(&args.kind),
    );
    let media = engine.identify(&input).await.map_err(anyhow::Error::msg)?;
    let mut results = engine
        .search(&media, &langs)
        .await
        .map_err(anyhow::Error::msg)?;
    results.truncate(args.limit);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        println!("{} — {} results", media.label(), results.len());
        for c in &results {
            println!(
                "  [{:>4}] {:<18} {:<7} {}",
                c.score,
                c.provider,
                c.language.display_tag(),
                c.release.as_deref().unwrap_or("(no release name)")
            );
        }
        if results.is_empty() {
            println!("  (no results — try --verbose, or a different title/season/episode)");
        }
    }
    Ok(())
}

async fn cmd_get(path: &Path, args: GetArgs, auto_mode: bool) -> Result<()> {
    let cfg = Config::load(path)?;
    let langs = parse_langs(&args.langs, &cfg);
    if langs.is_empty() {
        anyhow::bail!("no languages specified (use -l en,es or set languages in config)");
    }
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    let input = make_input(
        &args.input,
        args.season,
        args.episode,
        parse_kind(&args.kind),
    );

    let opts = ProcessOpts {
        to_utf8: cfg.process.to_utf8,
        target_format: Some(cfg.process.format.clone()),
        remove_hi: args.hi || cfg.process.remove_hi,
        language: None,
    };

    let do_sync = args.sync || auto_mode;
    let (media, mut files) = if auto_mode {
        engine
            .auto(&input, &langs, &opts, do_sync)
            .await
            .map_err(anyhow::Error::msg)?
    } else {
        let media = engine.identify(&input).await.map_err(anyhow::Error::msg)?;
        let mut files = match engine.download_best(&media, &langs, &opts).await {
            Ok(f) => f,
            Err(os_core::CoreError::NotFound) => Vec::new(),
            Err(e) => return Err(anyhow::Error::msg(e)),
        };
        if do_sync {
            if let Some(p) = &input.path {
                let video = Path::new(p);
                let mut synced = Vec::with_capacity(files.len());
                for f in files {
                    synced.push(engine.sync_to(f, video).await);
                }
                files = synced;
            }
        }
        (media, files)
    };

    if files.is_empty() {
        anyhow::bail!("no subtitle found for {}", media.label());
    }

    // Optional translation of the results.
    if let Some(tl) = &args.translate {
        let to = Language::parse(tl)
            .ok_or_else(|| anyhow::anyhow!("invalid --translate language: {tl}"))?;
        let mut extra = Vec::new();
        for f in &files {
            match engine.translate_to(f, &to).await {
                Ok(t) => extra.push(t),
                Err(e) => eprintln!("translate failed: {e}"),
            }
        }
        files.extend(extra);
    }

    // Decide output directory + stem.
    let (out_dir, stem) = output_target(&args, &media);
    std::fs::create_dir_all(&out_dir).context("creating output dir")?;

    let mut written = Vec::new();
    for f in &files {
        let name = f.sidecar_name(&stem);
        let dest = out_dir.join(&name);
        std::fs::write(&dest, &f.text).with_context(|| format!("writing {}", dest.display()))?;
        written.push(serde_json::json!({
            "language": f.language.display_tag(),
            "provider": f.provider,
            "format": f.format,
            "path": dest.to_string_lossy(),
            "release": f.release,
        }));
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&written)?);
    } else {
        for w in &written {
            println!(
                "downloaded {} [{}] -> {}",
                w["language"].as_str().unwrap_or(""),
                w["provider"].as_str().unwrap_or(""),
                w["path"].as_str().unwrap_or("")
            );
        }
    }
    Ok(())
}

async fn cmd_scan(path: &Path, args: ScanArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let langs = parse_langs(&args.langs, &cfg);
    if langs.is_empty() {
        anyhow::bail!("no languages specified (use -l en,es or set languages in config)");
    }
    if !args.dir.is_dir() {
        anyhow::bail!("not a directory: {}", args.dir.display());
    }

    let opts = ProcessOpts {
        to_utf8: cfg.process.to_utf8,
        target_format: Some(cfg.process.format.clone()),
        remove_hi: args.hi || cfg.process.remove_hi,
        language: None,
    };

    let recursive = !args.no_recursive;
    let videos = library::walk_videos(&args.dir, recursive);
    let scanned = videos.len();

    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;

    let mut results = Vec::new();
    let mut with_gaps = 0usize;
    let mut fetched_files = 0usize;

    for video in &videos {
        let missing = library::missing_languages(video, &langs);
        if missing.is_empty() {
            continue;
        }
        with_gaps += 1;
        let missing_tags: Vec<String> = missing.iter().map(|l| l.display_tag()).collect();

        if args.dry_run {
            if !args.json {
                println!("missing [{}] {}", missing_tags.join(","), video.display());
            }
            results.push(json!({
                "file": video.to_string_lossy(),
                "missing": missing_tags,
            }));
            continue;
        }

        let input = make_input(&video.to_string_lossy(), None, None, None);
        let media = match engine.identify(&input).await {
            Ok(m) => m,
            Err(e) => {
                if !args.json {
                    eprintln!("identify failed for {}: {e}", video.display());
                }
                results.push(json!({ "file": video.to_string_lossy(), "error": e.to_string() }));
                continue;
            }
        };
        let files = match engine.download_best(&media, &missing, &opts).await {
            Ok(f) => f,
            Err(os_core::CoreError::NotFound) => Vec::new(),
            Err(e) => {
                results.push(json!({ "file": video.to_string_lossy(), "error": e.to_string() }));
                continue;
            }
        };

        let dir = video
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = video
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "subtitle".into());

        let mut written = Vec::new();
        for f in &files {
            let dest = dir.join(f.sidecar_name(&stem));
            match std::fs::write(&dest, &f.text) {
                Ok(()) => {
                    fetched_files += 1;
                    if !args.json {
                        println!(
                            "downloaded {} [{}] -> {}",
                            f.language.display_tag(),
                            f.provider,
                            dest.display()
                        );
                    }
                    written.push(json!({
                        "language": f.language.display_tag(),
                        "provider": f.provider,
                        "path": dest.to_string_lossy(),
                    }));
                }
                Err(e) => {
                    written.push(json!({ "error": format!("write {}: {e}", dest.display()) }))
                }
            }
        }
        let still_missing: Vec<String> = missing
            .iter()
            .filter(|l| !files.iter().any(|f| f.language.same_language(l)))
            .map(|l| l.display_tag())
            .collect();
        results.push(json!({
            "file": video.to_string_lossy(),
            "media": media.label(),
            "fetched": written,
            "still_missing": still_missing,
        }));
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dir": args.dir.to_string_lossy(),
                "recursive": recursive,
                "languages": langs.iter().map(|l| l.alpha2()).collect::<Vec<_>>(),
                "scanned": scanned,
                "with_gaps": with_gaps,
                "fetched_files": fetched_files,
                "results": results,
            }))?
        );
    } else if args.dry_run {
        println!(
            "scanned {scanned} video(s); {with_gaps} missing one or more of [{}]",
            langs
                .iter()
                .map(|l| l.alpha2())
                .collect::<Vec<_>>()
                .join(",")
        );
    } else {
        println!("scanned {scanned} video(s); fetched {fetched_files} subtitle(s) for {with_gaps} file(s) with gaps");
    }
    Ok(())
}

/// Load a local subtitle file into a `SubtitleFile`.
fn load_subtitle(path: &Path) -> Result<os_core::SubtitleFile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "srt".into());
    // Try to read a language tag from the filename (e.g. movie.en.srt).
    let lang = fname
        .rsplit('.')
        .nth(1)
        .and_then(Language::parse)
        .unwrap_or_else(|| Language::parse("en").unwrap());
    Ok(os_core::SubtitleFile {
        language: lang,
        format,
        text,
        provider: "local".into(),
        release: Some(fname),
        hi: false,
        forced: false,
    })
}

async fn cmd_sync(path: &Path, args: SyncArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    if !engine.has_sync() {
        anyhow::bail!(
            "no sync backend available (set sync.backend = \"ffsubsync\" and install it)"
        );
    }
    let sub = load_subtitle(&args.subtitle)?;
    let synced = engine.sync_to(sub, &args.video).await;
    let dest = args.out.unwrap_or(args.subtitle);
    std::fs::write(&dest, &synced.text).with_context(|| format!("writing {}", dest.display()))?;
    println!("synced -> {}", dest.display());
    Ok(())
}

async fn cmd_translate(path: &Path, args: TranslateArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let engine = build_engine(&cfg).map_err(anyhow::Error::msg)?;
    if !engine.has_translate() {
        anyhow::bail!(
            "no translate backend available (set translate.backend and endpoint in config)"
        );
    }
    let to = Language::parse(&args.to)
        .ok_or_else(|| anyhow::anyhow!("invalid target language: {}", args.to))?;
    let sub = load_subtitle(&args.subtitle)?;
    let translated = engine
        .translate_to(&sub, &to)
        .await
        .map_err(anyhow::Error::msg)?;
    let dest = args.out.unwrap_or_else(|| {
        let stem = args
            .subtitle
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "subtitle".into());
        let dir = args
            .subtitle
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        dir.join(format!("{stem}.{}.srt", to.alpha2()))
    });
    std::fs::write(&dest, &translated.text)
        .with_context(|| format!("writing {}", dest.display()))?;
    println!("translated -> {}", dest.display());
    Ok(())
}

fn output_target(args: &GetArgs, media: &os_core::Media) -> (PathBuf, String) {
    let p = Path::new(&args.input);
    if p.is_file() {
        let dir = args
            .out
            .clone()
            .or_else(|| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "subtitle".into());
        (dir, stem)
    } else {
        let dir = args.out.clone().unwrap_or_else(|| PathBuf::from("."));
        let stem = sanitize_stem(&media.label());
        (dir, stem)
    }
}

fn sanitize_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(".")
}
