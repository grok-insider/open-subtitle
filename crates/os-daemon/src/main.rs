//! `ostd` — a tiny localhost HTTP/JSON server over the engine, so any app can
//! drive open-subtitle without FFI. Endpoints mirror the CLI:
//!
//! - `GET /health`                              → `{ "ok": true, "name": "ostd" }`
//! - `GET /identify?input=…[&season&episode&kind]`
//! - `GET /search?input=…[&langs=en,es …]`
//! - `GET /get?input=…[&langs=en …]`            → subtitle text inline

use os_compose::build_engine;
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::{Language, MediaKind};
use os_engine::Engine;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tiny_http::{Header, Response, Server};

fn main() -> anyhow::Result<()> {
    let addr = std::env::var("OSTD_ADDR").unwrap_or_else(|_| "127.0.0.1:4110".to_string());
    let cfg = Config::load_default().map_err(anyhow::Error::msg)?;
    let engine = Arc::new(build_engine(&cfg).map_err(anyhow::Error::msg)?);
    let cfg = Arc::new(cfg);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    eprintln!(
        "ostd listening on http://{addr}  (providers: {:?})",
        engine.provider_names()
    );

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let (path, query) = split_url(&url);
        let params = parse_query(query);

        let body = runtime.block_on(handle(&engine, &cfg, path, &params));
        let json = match body {
            Ok(v) => v,
            Err(e) => serde_json::json!({ "error": e }),
        };
        let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
        let resp = Response::from_string(json.to_string()).with_header(header);
        let _ = request.respond(resp);
    }
    Ok(())
}

async fn handle(
    engine: &Engine,
    cfg: &Config,
    path: &str,
    params: &HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    match path {
        "/health" => Ok(serde_json::json!({ "ok": true, "name": "ostd" })),
        "/identify" => {
            let input = build_input(params);
            let media = engine.identify(&input).await.map_err(|e| e.to_string())?;
            serde_json::to_value(media).map_err(|e| e.to_string())
        }
        "/search" => {
            let input = build_input(params);
            let langs = langs(params, cfg);
            let media = engine.identify(&input).await.map_err(|e| e.to_string())?;
            let results = engine
                .search(&media, &langs)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(results).map_err(|e| e.to_string())
        }
        "/get" => {
            let input = build_input(params);
            let langs = langs(params, cfg);
            let media = engine.identify(&input).await.map_err(|e| e.to_string())?;
            let opts = ProcessOpts {
                to_utf8: cfg.process.to_utf8,
                target_format: Some(cfg.process.format.clone()),
                remove_hi: cfg.process.remove_hi,
                language: None,
            };
            let files = engine
                .download_best(&media, &langs, &opts)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(files).map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown endpoint: {path}")),
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
