//! `ostd` — a localhost HTTP/JSON server over the engine.
//!
//! Native protocol (see `docs/PROTOCOL.md`):
//!   GET /health · /capabilities · /identify · /search · /get   (also under /v1)
//!
//! OpenSubtitles.com-compatible surface (the wedge — point existing clients here):
//!   GET  /osc/api/v1/subtitles?query&languages&season_number&episode_number&imdb_id
//!   POST /osc/api/v1/download  { "file_id": N }
//!   GET  /osc/file/<id>        → subtitle bytes

mod osc;
mod scan;
mod wanted;
mod webhook;

use os_compose::build_engine;
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::{CoreError, CoreResult, Language, Media, MediaKind, SubtitleCandidate};
use os_engine::Engine;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Response, Server};
use wanted::{now_secs, WantedItem, WantedStore};

/// file_id → candidate, so a client can download a previously-searched result.
type Registry = Arc<Mutex<HashMap<u64, SubtitleCandidate>>>;

/// Shared handle to the persistent wanted list.
type Store = Arc<WantedStore>;

const REGISTRY_CAP: usize = 20_000;

/// A ready-to-send HTTP response.
struct Out {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

impl Out {
    fn json(v: serde_json::Value) -> Out {
        Out {
            status: 200,
            content_type: "application/json".into(),
            body: v.to_string().into_bytes(),
        }
    }
    fn json_status(status: u16, v: serde_json::Value) -> Out {
        Out {
            status,
            content_type: "application/json".into(),
            body: v.to_string().into_bytes(),
        }
    }
    fn bytes(content_type: &str, body: Vec<u8>) -> Out {
        Out {
            status: 200,
            content_type: content_type.into(),
            body,
        }
    }
    fn error(status: u16, kind: &str, msg: impl std::fmt::Display) -> Out {
        Out::json_status(
            status,
            serde_json::json!({ "error": { "kind": kind, "message": msg.to_string() } }),
        )
    }
}

/// Map a `CoreError` to the typed error envelope + HTTP status.
fn err_from_core(e: &CoreError) -> Out {
    match e {
        CoreError::NotFound => Out::error(404, "not_found", e),
        CoreError::AuthRequired(_) => Out::error(401, "auth_required", e),
        CoreError::RateLimited { retry_after_secs } => Out::json_status(
            429,
            serde_json::json!({ "error": {
                "kind": "rate_limited", "message": e.to_string(),
                "retry_after_secs": retry_after_secs,
            }}),
        ),
        CoreError::DownloadLimit { .. } => Out::error(429, "download_limit", e),
        CoreError::Throttled(_) => Out::error(429, "throttled", e),
        CoreError::Unsupported(_) => Out::error(400, "unsupported", e),
        CoreError::Config(_) => Out::error(400, "config", e),
        CoreError::Parse(_) => Out::error(502, "parse", e),
        CoreError::Network(_) => Out::error(502, "network", e),
        CoreError::Provider(_) => Out::error(502, "provider", e),
        CoreError::Io(_) => Out::error(500, "io", e),
    }
}

fn main() -> anyhow::Result<()> {
    let addr = std::env::var("OSTD_ADDR").unwrap_or_else(|_| "127.0.0.1:4110".to_string());
    let base_url = format!("http://{addr}");
    let cfg = Arc::new(Config::load_default().map_err(anyhow::Error::msg)?);
    let engine = Arc::new(build_engine(&cfg).map_err(anyhow::Error::msg)?);
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));

    let wanted_path = Config::cache_dir()
        .map(|c| c.join("wanted.json"))
        .unwrap_or_else(|_| PathBuf::from("wanted.json"));
    let store: Store = Arc::new(WantedStore::load(wanted_path));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Background re-search of the wanted list (anime/slow releases that weren't
    // available at import time). Runs on a plain thread that drives the shared
    // runtime, so it's independent of the request loop.
    if cfg.automation.enabled
        && cfg.automation.track_wanted
        && cfg.automation.recheck_interval_secs > 0
    {
        let handle = runtime.handle().clone();
        let (s_engine, s_cfg, s_store) = (engine.clone(), cfg.clone(), store.clone());
        std::thread::spawn(move || scheduler_loop(handle, s_engine, s_cfg, s_store));
        eprintln!(
            "ostd: wanted-list scheduler on (every {}s, {} item(s) pending)",
            cfg.automation.recheck_interval_secs,
            store.len()
        );
    }

    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    eprintln!(
        "ostd listening on {base_url}  (providers: {:?})",
        engine.provider_names()
    );

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let (path, query) = split_url(&url);
        let path = path.strip_prefix("/v1").unwrap_or(path).to_string();
        let params = parse_query(query);

        let mut body = String::new();
        if matches!(method, Method::Post) {
            let _ = request.as_reader().read_to_string(&mut body);
        }

        let out = runtime.block_on(handle(
            &engine, &cfg, &registry, &store, &base_url, &method, &path, &params, &body,
        ));

        let header = Header::from_bytes(&b"Content-Type"[..], out.content_type.as_bytes())
            .unwrap_or_else(|_| {
                Header::from_bytes(&b"Content-Type"[..], &b"application/octet-stream"[..]).unwrap()
            });
        let resp = Response::from_data(out.body)
            .with_status_code(out.status)
            .with_header(header);
        let _ = request.respond(resp);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle(
    engine: &Engine,
    cfg: &Config,
    registry: &Registry,
    store: &Store,
    base_url: &str,
    method: &Method,
    path: &str,
    params: &HashMap<String, String>,
    body: &str,
) -> Out {
    match path {
        "/health" => Out::json(serde_json::json!({ "ok": true, "name": "ostd" })),
        "/capabilities" => capabilities(engine, cfg),

        "/identify" => match engine.identify(&build_input(params)).await {
            Ok(media) => Out::json(serde_json::to_value(media).unwrap_or_default()),
            Err(e) => err_from_core(&e),
        },
        "/search" => {
            let input = build_input(params);
            let langs = langs(params, cfg);
            match engine.identify(&input).await {
                Ok(media) => match engine.search(&media, &langs).await {
                    Ok(r) => Out::json(serde_json::to_value(r).unwrap_or_default()),
                    Err(e) => err_from_core(&e),
                },
                Err(e) => err_from_core(&e),
            }
        }
        "/get" => {
            let input = build_input(params);
            let langs = langs(params, cfg);
            let opts = process_opts(cfg);
            match engine.identify(&input).await {
                Ok(media) => match engine.download_best(&media, &langs, &opts).await {
                    Ok(files) => Out::json(serde_json::to_value(files).unwrap_or_default()),
                    Err(e) => err_from_core(&e),
                },
                Err(e) => err_from_core(&e),
            }
        }

        // ---- OpenSubtitles.com-compatible surface ----
        "/osc/api/v1/subtitles" => osc_subtitles(engine, cfg, registry, params).await,
        "/osc/api/v1/download" => {
            if !matches!(method, Method::Post) {
                return Out::error(405, "unsupported", "POST required");
            }
            osc_download(registry, base_url, body)
        }
        p if p.starts_with("/osc/file/") => {
            osc_file(engine, cfg, registry, &p["/osc/file/".len()..]).await
        }

        // ---- automation webhooks (Sonarr/Radarr "On Import") ----
        "/webhook" | "/webhook/sonarr" | "/webhook/radarr" => {
            if !matches!(method, Method::Post) {
                return Out::error(405, "unsupported", "POST required");
            }
            handle_webhook(engine, cfg, store, body).await
        }

        // ---- library scan + wanted list (the v0.4 automation flagship) ----
        "/scan" => {
            if !matches!(method, Method::Post) {
                return Out::error(405, "unsupported", "POST required");
            }
            scan::handle(engine, cfg, store, body).await
        }
        "/wanted" => match method {
            Method::Get => wanted_list(store),
            Method::Delete => {
                let n = store.len();
                store.clear();
                Out::json(json!({ "ok": true, "cleared": n }))
            }
            _ => Out::error(405, "unsupported", "GET or DELETE"),
        },
        "/wanted/clear" => {
            if !matches!(method, Method::Post) {
                return Out::error(405, "unsupported", "POST required");
            }
            let n = store.len();
            store.clear();
            Out::json(json!({ "ok": true, "cleared": n }))
        }
        "/wanted/run" => {
            if !matches!(method, Method::Post) {
                return Out::error(405, "unsupported", "POST required");
            }
            wanted_run_now(engine, cfg, store).await
        }

        _ => Out::error(404, "not_found", format!("unknown endpoint: {path}")),
    }
}

async fn handle_webhook(engine: &Engine, cfg: &Config, store: &Store, body: &str) -> Out {
    let payload: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return Out::error(400, "parse", format!("invalid JSON: {e}")),
    };
    match webhook::parse(&payload) {
        webhook::Action::Test => Out::json(json!({ "ok": true, "action": "test" })),
        webhook::Action::Ignore(reason) => {
            Out::json(json!({ "ok": true, "action": "ignored", "reason": reason }))
        }
        webhook::Action::Import(job) => {
            if !cfg.automation.enabled {
                return Out::json(json!({ "ok": true, "action": "disabled" }));
            }
            run_import(engine, cfg, store, *job).await
        }
    }
}

/// `GET /wanted` — the current wanted list (visibility + debugging).
fn wanted_list(store: &Store) -> Out {
    let items = store.list();
    Out::json(json!({
        "ok": true,
        "count": items.len(),
        "items": serde_json::to_value(&items).unwrap_or_default(),
    }))
}

/// `POST /wanted/run` — force an immediate re-search of every pending item
/// (ignoring the timer), honoring only the attempt cap.
async fn wanted_run_now(engine: &Engine, cfg: &Config, store: &Store) -> Out {
    if store.is_empty() {
        return Out::json(json!({
            "ok": true, "processed": 0, "delivered": 0, "satisfied": 0, "remaining": 0,
        }));
    }
    let cap = cfg.automation.max_attempts;
    let items: Vec<WantedItem> = store
        .list()
        .into_iter()
        .filter(|i| cap == 0 || i.attempts < cap)
        .collect();
    let mut satisfied = 0usize;
    let mut delivered_total = 0usize;
    for item in &items {
        let got = recheck_one(engine, cfg, store, item.clone()).await;
        delivered_total += got;
        if got > 0 && !store.list().iter().any(|i| i.key == item.key) {
            satisfied += 1;
        }
    }
    Out::json(json!({
        "ok": true, "processed": items.len(),
        "delivered": delivered_total, "satisfied": satisfied,
        "remaining": store.len(),
    }))
}

/// A normalized request to fetch subtitles, shared by webhook imports, library
/// scans, and the wanted-list re-search loop. `path` is the media file (used for
/// hashing + sidecar placement when reachable); `query` is the title fallback
/// when it isn't.
#[derive(Debug, Clone, Default)]
pub struct FetchTarget {
    pub path: Option<String>,
    pub query: Option<String>,
    pub kind: Option<MediaKind>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    pub series_imdb: Option<String>,
    pub series_tvdb: Option<u64>,
    pub episode_title: Option<String>,
}

/// The result of a fetch: what we identified, what we wrote, and which requested
/// languages are still missing (the wanted-list driver).
pub struct FetchOutcome {
    pub media: Media,
    pub written: Vec<serde_json::Value>,
    pub fetched: usize,
    /// alpha-2 tags actually written to disk this pass.
    pub delivered: Vec<String>,
    /// alpha-2 tags requested but not delivered.
    pub missing: Vec<String>,
}

/// Identify → overlay ids → download best per language → write sidecars. Shared
/// by every automation surface so they behave identically. `NotFound` is treated
/// as "nothing available yet" (empty), not an error; other errors propagate.
pub async fn fetch_for_target(
    engine: &Engine,
    cfg: &Config,
    target: &FetchTarget,
    langs: &[Language],
) -> CoreResult<FetchOutcome> {
    let file_exists = target
        .path
        .as_deref()
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false);

    // With an explicit title we identify by query; otherwise from the filename.
    let (name, query) = match &target.query {
        Some(q) => (None, Some(q.clone())),
        None if file_exists => (
            target.path.as_ref().and_then(|p| {
                Path::new(p)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
            }),
            None,
        ),
        None => (None, None),
    };
    let input = MediaInput {
        path: if file_exists {
            target.path.as_ref().map(PathBuf::from)
        } else {
            None
        },
        name,
        query,
        kind_hint: target.kind,
        season: target.season,
        episode: target.episode,
    };
    let mut media = engine.identify(&input).await?;
    if let Some(k) = target.kind {
        media.kind = k; // the payload's kind is authoritative.
    }

    // Overlay the caller-authoritative ids + episode title.
    match media.kind {
        MediaKind::Movie => {
            if target.imdb.is_some() {
                media.ids.imdb = target.imdb.clone();
            }
            if target.tmdb.is_some() {
                media.ids.tmdb = target.tmdb;
            }
        }
        _ => {
            if target.series_imdb.is_some() {
                media.ids.series_imdb = target.series_imdb.clone();
            }
            if target.series_tvdb.is_some() {
                media.ids.series_tvdb = target.series_tvdb;
            }
        }
    }
    if media.episode_title.is_none() && target.episode_title.is_some() {
        media.episode_title = target.episode_title.clone();
    }

    let opts = process_opts(cfg);
    let files = match engine.download_best(&media, langs, &opts).await {
        Ok(f) => f,
        Err(CoreError::NotFound) => Vec::new(),
        Err(e) => return Err(e),
    };

    let (dir, stem) = write_target(cfg, target.path.as_deref(), file_exists, &media);
    let mut written = Vec::new();
    let mut delivered = Vec::new();
    if !files.is_empty() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return Err(CoreError::Io(format!("create dir {}: {e}", dir.display())));
        }
        for f in &files {
            let dest = dir.join(f.sidecar_name(&stem));
            match std::fs::write(&dest, &f.text) {
                Ok(()) => {
                    delivered.push(f.language.alpha2());
                    written.push(json!({
                        "language": f.language.display_tag(), "provider": f.provider,
                        "path": dest.to_string_lossy(),
                    }));
                }
                Err(e) => {
                    written.push(json!({ "error": format!("write {}: {e}", dest.display()) }))
                }
            }
        }
    }

    let missing: Vec<String> = langs
        .iter()
        .map(|l| l.alpha2())
        .filter(|a| !delivered.iter().any(|d| d.eq_ignore_ascii_case(a)))
        .collect();

    Ok(FetchOutcome {
        media,
        written,
        fetched: files.len(),
        delivered,
        missing,
    })
}

/// Where to write sidecars: next to the media file when reachable, else the
/// configured fallback dir (or the cache).
fn write_target(
    cfg: &Config,
    path: Option<&str>,
    file_exists: bool,
    media: &Media,
) -> (PathBuf, String) {
    if file_exists {
        let pp = Path::new(path.unwrap_or("."));
        (
            pp.parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
            pp.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "subtitle".into()),
        )
    } else {
        let dir = cfg
            .automation
            .output_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Config::cache_dir()
                    .map(|c| c.join("automation"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            });
        (dir, sanitize_stem(&media.label()))
    }
}

/// Build a [`WantedItem`] from an identified media + the still-missing languages.
/// `attempted` marks whether a fetch was just tried (so the timer starts now).
pub fn wanted_from(
    media: &Media,
    target: &FetchTarget,
    missing: &[String],
    source: &str,
    attempted: bool,
) -> WantedItem {
    let now = now_secs();
    WantedItem {
        key: WantedItem::make_key(
            target.path.as_deref(),
            target.query.as_deref(),
            media.kind,
            media.season,
            media.episode_num(),
        ),
        path: target.path.clone(),
        query: target.query.clone(),
        kind: media.kind,
        season: media.season,
        episode: media.episode_num(),
        imdb: media.ids.imdb.clone(),
        tmdb: media.ids.tmdb,
        series_imdb: media.ids.series_imdb.clone(),
        series_tvdb: media.ids.series_tvdb,
        episode_title: media.episode_title.clone(),
        languages: missing.to_vec(),
        label: media.label(),
        source: source.into(),
        added_at: now,
        last_attempt: if attempted { now } else { 0 },
        attempts: u32::from(attempted),
    }
}

fn target_from_item(item: &WantedItem) -> FetchTarget {
    FetchTarget {
        path: item.path.clone(),
        query: item.query.clone(),
        kind: Some(item.kind),
        season: item.season,
        episode: item.episode,
        imdb: item.imdb.clone(),
        tmdb: item.tmdb,
        series_imdb: item.series_imdb.clone(),
        series_tvdb: item.series_tvdb,
        episode_title: item.episode_title.clone(),
    }
}

/// Re-search one wanted item; prune delivered languages (removing the item when
/// satisfied) and always bump its attempt bookkeeping. Returns the count
/// delivered this pass.
async fn recheck_one(engine: &Engine, cfg: &Config, store: &Store, item: WantedItem) -> usize {
    let langs: Vec<Language> = item
        .languages
        .iter()
        .filter_map(|c| Language::parse(c))
        .collect();
    if langs.is_empty() {
        store.remove(&item.key);
        return 0;
    }
    let target = target_from_item(&item);
    match fetch_for_target(engine, cfg, &target, &langs).await {
        Ok(outcome) => {
            let n = outcome.delivered.len();
            let satisfied = store.record_result(&item.key, &outcome.delivered, now_secs());
            if satisfied {
                eprintln!("ostd: wanted satisfied — {}", item.label);
            } else if n > 0 {
                eprintln!("ostd: wanted partial — {} (+{n})", item.label);
            }
            n
        }
        Err(e) => {
            // Transient (network/throttle): keep the item, bump the attempt.
            eprintln!("ostd: wanted recheck failed — {}: {e}", item.label);
            store.record_result(&item.key, &[], now_secs());
            0
        }
    }
}

/// Background loop: every `recheck_interval_secs`, re-search the due items.
fn scheduler_loop(
    handle: tokio::runtime::Handle,
    engine: Arc<Engine>,
    cfg: Arc<Config>,
    store: Store,
) {
    let interval = cfg.automation.recheck_interval_secs.max(1);
    let tick = std::time::Duration::from_secs(interval);
    loop {
        std::thread::sleep(tick);
        let due = store.due(now_secs(), interval, cfg.automation.max_attempts);
        if due.is_empty() {
            continue;
        }
        eprintln!("ostd: wanted re-search — {} item(s) due", due.len());
        for item in due {
            handle.block_on(recheck_one(&engine, &cfg, &store, item));
        }
    }
}

async fn run_import(engine: &Engine, cfg: &Config, store: &Store, job: webhook::Job) -> Out {
    let langs: Vec<Language> = cfg
        .automation
        .languages_or(&cfg.languages)
        .iter()
        .filter_map(|c| Language::parse(c))
        .collect();
    if langs.is_empty() {
        return Out::error(400, "config", "no languages configured");
    }

    let target = FetchTarget {
        path: job.file_path.as_ref().map(|p| cfg.automation.remap(p)),
        query: Some(job.title.clone()),
        kind: Some(job.kind),
        season: job.season,
        episode: job.episode,
        imdb: job.imdb.clone(),
        tmdb: job.tmdb,
        series_imdb: job.series_imdb.clone(),
        series_tvdb: job.series_tvdb,
        episode_title: job.episode_title.clone(),
    };

    let outcome = match fetch_for_target(engine, cfg, &target, &langs).await {
        Ok(o) => o,
        Err(e) => return err_from_core(&e),
    };

    // Record any still-missing languages for scheduled re-search.
    if !outcome.missing.is_empty() && cfg.automation.track_wanted {
        store.upsert(wanted_from(
            &outcome.media,
            &target,
            &outcome.missing,
            &format!("webhook:{}", job.source),
            true,
        ));
    }

    Out::json(json!({
        "ok": true, "action": "import", "source": job.source,
        "media": outcome.media.label(), "count": outcome.fetched,
        "downloaded": serde_json::Value::Array(outcome.written),
        "wanted": outcome.missing,
    }))
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

fn capabilities(engine: &Engine, cfg: &Config) -> Out {
    Out::json(serde_json::json!({
        "name": "open-subtitle",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "v1-draft",
        "providers": engine.provider_names(),
        "features": {
            "identify": true, "search": true, "get": true,
            "sync": engine.has_sync(),
            "translate": engine.has_translate(),
            "transcribe": engine.has_transcribe(),
            "opensubtitles_compat": true,
        },
        "languages_default": cfg.languages().iter().map(|l| l.alpha2()).collect::<Vec<_>>(),
    }))
}

/// Build a `Media` from OpenSubtitles-style query params (query/imdb_id/season/episode).
async fn osc_media(engine: &Engine, params: &HashMap<String, String>) -> Result<Media, CoreError> {
    let mut media = if let Some(q) = params.get("query").filter(|q| !q.is_empty()) {
        engine
            .identify(&MediaInput {
                name: Some(q.clone()),
                ..Default::default()
            })
            .await?
    } else {
        Media::default()
    };

    if let Some(imdb) = params.get("imdb_id").filter(|s| !s.is_empty()) {
        media.ids.imdb = Some(imdb.trim_start_matches("tt").to_string());
    }
    let season = params
        .get("season_number")
        .and_then(|s| s.parse::<u32>().ok());
    let episode = params
        .get("episode_number")
        .and_then(|s| s.parse::<u32>().ok());
    if season.is_some() || episode.is_some() {
        if media.kind == MediaKind::Movie {
            media.kind = MediaKind::Series;
        }
        media.season = Some(season.unwrap_or(1));
        if let Some(e) = episode {
            media.episodes = vec![e];
        }
    }
    Ok(media)
}

async fn osc_subtitles(
    engine: &Engine,
    cfg: &Config,
    registry: &Registry,
    params: &HashMap<String, String>,
) -> Out {
    let langs = match params.get("languages") {
        Some(s) if !s.is_empty() => s
            .split(',')
            .filter_map(|c| Language::parse(c.trim()))
            .collect(),
        _ => cfg.languages(),
    };
    let page = params
        .get("page")
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1);

    let media = match osc_media(engine, params).await {
        Ok(m) => m,
        Err(e) => return err_from_core(&e),
    };
    let candidates = match engine.search(&media, &langs).await {
        Ok(c) => c,
        Err(e) => return err_from_core(&e),
    };

    let (body, pairs) = osc::search_response(&candidates, page);
    {
        let mut reg = registry.lock().unwrap();
        if reg.len() > REGISTRY_CAP {
            reg.clear();
        }
        for (fid, cand) in pairs {
            reg.insert(fid, cand);
        }
    }
    Out::json(body)
}

fn osc_download(registry: &Registry, base_url: &str, body: &str) -> Out {
    let file_id = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("file_id").and_then(|f| f.as_u64()));
    let file_id = match file_id {
        Some(id) => id,
        None => return Out::error(400, "config", "missing or invalid file_id"),
    };
    let cand = registry.lock().unwrap().get(&file_id).cloned();
    match cand {
        Some(c) => {
            let file_name = c.release.unwrap_or_else(|| format!("{file_id}.srt"));
            Out::json(osc::download_response(base_url, file_id, &file_name))
        }
        None => Out::error(404, "not_found", "unknown file_id (search first)"),
    }
}

async fn osc_file(engine: &Engine, cfg: &Config, registry: &Registry, id_str: &str) -> Out {
    let file_id = match id_str.parse::<u64>() {
        Ok(id) => id,
        Err(_) => return Out::error(400, "config", "invalid file id"),
    };
    let cand = registry.lock().unwrap().get(&file_id).cloned();
    let cand = match cand {
        Some(c) => c,
        None => return Out::error(404, "not_found", "unknown file_id (search first)"),
    };
    let opts = process_opts(cfg);
    match engine.fetch_candidate(&cand, &opts).await {
        Ok(file) => {
            let ct = match file.format.as_str() {
                "srt" => "application/x-subrip; charset=utf-8",
                "ass" | "ssa" => "text/x-ssa; charset=utf-8",
                "vtt" => "text/vtt; charset=utf-8",
                _ => "text/plain; charset=utf-8",
            };
            Out::bytes(ct, file.text.into_bytes())
        }
        Err(e) => err_from_core(&e),
    }
}

fn process_opts(cfg: &Config) -> ProcessOpts {
    ProcessOpts {
        to_utf8: cfg.process.to_utf8,
        target_format: Some(cfg.process.format.clone()),
        remove_hi: cfg.process.remove_hi,
        language: None,
    }
}

fn build_input(params: &HashMap<String, String>) -> MediaInput {
    let input = params.get("input").cloned().unwrap_or_default();
    let kind = match params.get("kind").map(|s| s.as_str()) {
        Some("movie") => Some(MediaKind::Movie),
        Some("series") | Some("tv") => Some(MediaKind::Series),
        Some("anime") => Some(MediaKind::Anime),
        _ => None,
    };
    let season = params.get("season").and_then(|s| s.parse().ok());
    let episode = params.get("episode").and_then(|s| s.parse().ok());
    let p = Path::new(&input);
    if p.is_file() {
        MediaInput {
            path: Some(p.to_path_buf()),
            name: p.file_name().map(|s| s.to_string_lossy().into_owned()),
            query: None,
            kind_hint: kind,
            season,
            episode,
        }
    } else {
        MediaInput {
            path: None,
            name: Some(input),
            query: None,
            kind_hint: kind,
            season,
            episode,
        }
    }
}

fn langs(params: &HashMap<String, String>, cfg: &Config) -> Vec<Language> {
    match params.get("langs") {
        Some(s) => s
            .split(',')
            .filter_map(|c| Language::parse(c.trim()))
            .collect(),
        None => cfg.languages(),
    }
}

fn split_url(url: &str) -> (&str, &str) {
    match url.split_once('?') {
        Some((p, q)) => (p, q),
        None => (url, ""),
    }
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        // application/x-www-form-urlencoded uses '+' for space; percent-decode
        // alone leaves literal '+' (breaks curl --data-urlencode and many clients).
        let k = decode_query_component(k);
        let v = decode_query_component(v);
        map.insert(k, v);
    }
    map
}

fn decode_query_component(s: &str) -> String {
    let plus_as_space = s.replace('+', " ");
    urlencoding::decode(&plus_as_space)
        .map(|c| c.into_owned())
        .unwrap_or(plus_as_space)
}

#[cfg(test)]
mod query_tests {
    use super::*;

    #[test]
    fn parse_query_plus_is_space() {
        let m = parse_query("input=Interstellar+2014&langs=en");
        assert_eq!(
            m.get("input").map(String::as_str),
            Some("Interstellar 2014")
        );
        assert_eq!(m.get("langs").map(String::as_str), Some("en"));
    }

    #[test]
    fn parse_query_percent_encoding() {
        let m = parse_query("input=Interstellar%202014");
        assert_eq!(
            m.get("input").map(String::as_str),
            Some("Interstellar 2014")
        );
    }

    #[test]
    fn parse_query_literal_plus_via_percent() {
        // C++ must be encoded as C%2B%2B so '+' is not treated as space.
        let m = parse_query("input=C%2B%2B");
        assert_eq!(m.get("input").map(String::as_str), Some("C++"));
    }
}
