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

use os_compose::build_engine;
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::{CoreError, Language, Media, MediaKind, SubtitleCandidate};
use os_engine::Engine;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Response, Server};

/// file_id → candidate, so a client can download a previously-searched result.
type Registry = Arc<Mutex<HashMap<u64, SubtitleCandidate>>>;

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

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

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
            &engine, &cfg, &registry, &base_url, &method, &path, &params, &body,
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

        _ => Out::error(404, "not_found", format!("unknown endpoint: {path}")),
    }
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
        let k = urlencoding::decode(k)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| k.to_string());
        let v = urlencoding::decode(v)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| v.to_string());
        map.insert(k, v);
    }
    map
}
