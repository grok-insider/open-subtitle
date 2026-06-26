//! `POST /scan` — walk a media library, fetch subtitles for files that are
//! missing one (per requested language), and (optionally) record anything still
//! unfound in the wanted list for scheduled re-search.
//!
//! Body (JSON):
//! ```json
//! { "dir": "/media/tv", "languages": ["en","es"], "recursive": true, "add_wanted": true }
//! ```
//! `languages` may also be a comma string; it defaults to the config languages.
//! `recursive` defaults to `true`; `add_wanted` defaults to `automation.track_wanted`.
//!
//! Files are processed sequentially: the engine already fans out across providers
//! per search, and sequential walking keeps provider load (and throttling) sane
//! on large libraries.

use crate::{fetch_for_target, wanted_from, FetchTarget, Out, Store};
use os_config::Config;
use os_core::Language;
use os_engine::{library, Engine};
use serde_json::{json, Value};
use std::path::Path;

fn parse_langs(v: Option<&Value>, cfg: &Config) -> Vec<Language> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str())
            .filter_map(|s| Language::parse(s.trim()))
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .filter_map(|c| Language::parse(c.trim()))
            .collect(),
        _ => cfg.languages(),
    }
}

pub async fn handle(engine: &Engine, cfg: &Config, store: &Store, body: &str) -> Out {
    let payload: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return Out::error(400, "parse", format!("invalid JSON: {e}")),
    };

    let dir = match payload.get("dir").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => return Out::error(400, "config", "missing \"dir\""),
    };
    let dir_path = Path::new(&dir);
    if !dir_path.is_dir() {
        return Out::error(400, "config", format!("not a directory: {dir}"));
    }

    let langs = parse_langs(payload.get("languages"), cfg);
    if langs.is_empty() {
        return Out::error(400, "config", "no languages configured");
    }
    let recursive = payload
        .get("recursive")
        .and_then(|r| r.as_bool())
        .unwrap_or(true);
    let add_wanted = payload
        .get("add_wanted")
        .and_then(|r| r.as_bool())
        .unwrap_or(cfg.automation.track_wanted);

    let videos = library::walk_videos(dir_path, recursive);
    let scanned = videos.len();

    let mut results = Vec::new();
    let mut with_gaps = 0usize;
    let mut fetched_files = 0usize;
    let mut wanted_added = 0usize;

    for video in &videos {
        let missing = library::missing_languages(video, &langs);
        if missing.is_empty() {
            continue;
        }
        with_gaps += 1;

        let target = FetchTarget {
            path: Some(video.to_string_lossy().into_owned()),
            ..Default::default()
        };
        match fetch_for_target(engine, cfg, &target, &missing).await {
            Ok(outcome) => {
                fetched_files += outcome.delivered.len();
                if add_wanted && !outcome.missing.is_empty() {
                    store.upsert(wanted_from(
                        &outcome.media,
                        &target,
                        &outcome.missing,
                        "scan",
                        true,
                    ));
                    wanted_added += 1;
                }
                results.push(json!({
                    "file": video.to_string_lossy(),
                    "media": outcome.media.label(),
                    "fetched": Value::Array(outcome.written),
                    "still_missing": outcome.missing,
                }));
            }
            Err(e) => results.push(json!({
                "file": video.to_string_lossy(),
                "error": e.to_string(),
            })),
        }
    }

    Out::json(json!({
        "ok": true,
        "dir": dir,
        "recursive": recursive,
        "languages": langs.iter().map(|l| l.alpha2()).collect::<Vec<_>>(),
        "scanned": scanned,
        "with_gaps": with_gaps,
        "fetched_files": fetched_files,
        "wanted_added": wanted_added,
        "results": results,
    }))
}
