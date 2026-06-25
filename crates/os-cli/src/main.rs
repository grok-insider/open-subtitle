//! `ost` — the open-subtitle CLI and the sidecar the mpv plugin drives.
//!
//! Keyless by default. Every subcommand supports `--json` (the sidecar contract).

mod compose;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::{Language, MediaKind};
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
    #[arg(long)]
    json: bool,
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
        Command::Get(args) => cmd_get(&config_path, args).await,
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
    let engine = compose::build_engine(&cfg).map_err(anyhow::Error::msg)?;
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
    let engine = compose::build_engine(&cfg).map_err(anyhow::Error::msg)?;
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
    let engine = compose::build_engine(&cfg).map_err(anyhow::Error::msg)?;
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

async fn cmd_get(path: &Path, args: GetArgs) -> Result<()> {
    let cfg = Config::load(path)?;
    let langs = parse_langs(&args.langs, &cfg);
    if langs.is_empty() {
        anyhow::bail!("no languages specified (use -l en,es or set languages in config)");
    }
    let engine = compose::build_engine(&cfg).map_err(anyhow::Error::msg)?;
    let input = make_input(
        &args.input,
        args.season,
        args.episode,
        parse_kind(&args.kind),
    );
    let media = engine.identify(&input).await.map_err(anyhow::Error::msg)?;

    let opts = ProcessOpts {
        to_utf8: cfg.process.to_utf8,
        target_format: Some(cfg.process.format.clone()),
        remove_hi: args.hi || cfg.process.remove_hi,
        language: None,
    };
    let files = engine
        .download_best(&media, &langs, &opts)
        .await
        .map_err(anyhow::Error::msg)?;

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
