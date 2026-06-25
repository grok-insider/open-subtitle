//! OpenSubtitles.com-compatible mapping (pure functions).
//!
//! The "wedge" (see `docs/STRATEGY.md`): present our engine's results in the
//! shape OpenSubtitles.com's REST API uses, so existing clients can be repointed
//! at a local `ostd` and transparently use our keyless multi-provider engine.
//! These functions are pure and unit-tested; the wiring lives in `main.rs`.

use os_core::{Language, SubtitleCandidate};
use std::hash::{Hash, Hasher};

/// A deterministic, JS-safe (≤ 2^53-1) non-zero integer id for a candidate, so a
/// repeated search yields the same `file_id` a client can later download.
pub fn file_id_for(c: &SubtitleCandidate) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.provider.hash(&mut h);
    c.id.hash(&mut h);
    let v = h.finish() & 0x1F_FFFF_FFFF_FFFF; // 53 bits
    v.max(1)
}

/// OpenSubtitles language code: ISO 639-1 plus a lowercased region (e.g. `pt-br`).
pub fn osc_lang(l: &Language) -> String {
    let mut s = l.alpha2();
    if let Some(r) = &l.region {
        s.push('-');
        s.push_str(&r.to_lowercase());
    }
    s
}

/// One `data[]` entry for a candidate, plus its `file_id` (for the registry).
pub fn candidate_to_osc(c: &SubtitleCandidate) -> (u64, serde_json::Value) {
    let fid = file_id_for(c);
    let file_name = c.release.clone().unwrap_or_else(|| format!("{fid}.srt"));
    let attributes = serde_json::json!({
        "language": osc_lang(&c.language),
        "release": c.release,
        "hearing_impaired": c.hi,
        "foreign_parts_only": c.forced,
        "from_trusted": false,
        "ai_translated": false,
        "machine_translated": false,
        "download_count": c.hints.get("downloads")
            .and_then(|d| d.parse::<i64>().ok()).unwrap_or(0),
        "ratings": (c.score as f64) / 100.0,
        "files": [ { "file_id": fid, "file_name": file_name } ],
    });
    (
        fid,
        serde_json::json!({
            "id": fid.to_string(),
            "type": "subtitle",
            "attributes": attributes,
        }),
    )
}

/// The full `/subtitles` response body for a set of candidates. Returns the body
/// and the `(file_id, candidate)` pairs to register for later download.
pub fn search_response(
    candidates: &[SubtitleCandidate],
    page: u32,
) -> (serde_json::Value, Vec<(u64, SubtitleCandidate)>) {
    let mut data = Vec::with_capacity(candidates.len());
    let mut registry = Vec::with_capacity(candidates.len());
    for c in candidates {
        let (fid, entry) = candidate_to_osc(c);
        data.push(entry);
        registry.push((fid, c.clone()));
    }
    let total = data.len();
    let body = serde_json::json!({
        "total_pages": 1,
        "total_count": total,
        "per_page": total,
        "page": page,
        "data": data,
    });
    (body, registry)
}

/// The `/download` response pointing at our local file endpoint.
pub fn download_response(base_url: &str, file_id: u64, file_name: &str) -> serde_json::Value {
    serde_json::json!({
        "link": format!("{base_url}/osc/file/{file_id}"),
        "file_name": file_name,
        "requests": 0,
        "remaining": 1000,        // keyless providers ≈ unlimited
        "message": "",
        "reset_time": "",
        "reset_time_utc": "",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand() -> SubtitleCandidate {
        let mut c = SubtitleCandidate::new(
            "opensubtitles_org",
            "999",
            Language::parse("pt-BR").unwrap(),
        );
        c.release = Some("Movie.2009.1080p".into());
        c.hi = true;
        c.score = 216;
        c.hints.insert("downloads".into(), "42".into());
        c
    }

    #[test]
    fn file_id_is_stable_nonzero_and_js_safe() {
        let c = cand();
        let a = file_id_for(&c);
        let b = file_id_for(&c);
        assert_eq!(a, b);
        assert!(a >= 1);
        assert!(a <= 0x1F_FFFF_FFFF_FFFF);
    }

    #[test]
    fn lang_uses_region() {
        assert_eq!(osc_lang(&Language::parse("pt-BR").unwrap()), "pt-br");
        assert_eq!(osc_lang(&Language::parse("en").unwrap()), "en");
    }

    #[test]
    fn candidate_maps_to_osc_shape() {
        let c = cand();
        let (fid, v) = candidate_to_osc(&c);
        assert_eq!(v["type"], "subtitle");
        assert_eq!(v["id"], fid.to_string());
        let attrs = &v["attributes"];
        assert_eq!(attrs["language"], "pt-br");
        assert_eq!(attrs["hearing_impaired"], true);
        assert_eq!(attrs["files"][0]["file_id"], fid);
        assert_eq!(attrs["files"][0]["file_name"], "Movie.2009.1080p");
        assert_eq!(attrs["download_count"], 42);
    }

    #[test]
    fn search_response_registers_all() {
        let cands = vec![cand(), {
            let mut c = cand();
            c.id = "1000".into();
            c
        }];
        let (body, reg) = search_response(&cands, 1);
        assert_eq!(body["total_count"], 2);
        assert_eq!(body["data"].as_array().unwrap().len(), 2);
        assert_eq!(reg.len(), 2);
        // file_ids differ because the candidate ids differ.
        assert_ne!(reg[0].0, reg[1].0);
    }

    #[test]
    fn download_response_points_at_local_file() {
        let v = download_response("http://127.0.0.1:4110", 12345, "x.srt");
        assert_eq!(v["link"], "http://127.0.0.1:4110/osc/file/12345");
        assert_eq!(v["file_name"], "x.srt");
    }
}
