//! OpenSubtitles.com modern REST provider. Needs an API key (and optionally a
//! login for higher limits). Default-disabled. Search params are sent in
//! alphabetical order (the API redirects otherwise). Two-step download:
//! POST `/download {file_id}` → CDN link → GET.

use crate::http;
use async_trait::async_trait;
use os_core::ports::{Capabilities, Provider};
use os_core::{Container, CoreError, CoreResult, Language, Query, RawSubtitle, SubtitleCandidate};

const BASE: &str = "https://api.opensubtitles.com/api/v1";

/// OpenSubtitles.com provider (requires `api_key`).
pub struct OpenSubtitlesCom {
    client: reqwest::Client,
    api_key: String,
    user_agent: String,
}

impl OpenSubtitlesCom {
    pub fn new(client: reqwest::Client, api_key: String, user_agent: String) -> Self {
        OpenSubtitlesCom {
            client,
            api_key,
            user_agent,
        }
    }
}

/// Parse the `{ data: [...] }` search response into candidates.
pub fn parse_results(json: &serde_json::Value) -> Vec<SubtitleCandidate> {
    let arr = match json.get("data").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in arr {
        let attrs = match item.get("attributes") {
            Some(a) => a,
            None => continue,
        };
        let file_id = attrs
            .get("files")
            .and_then(|f| f.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.get("file_id"))
            .and_then(|v| v.as_i64());
        let file_id = match file_id {
            Some(id) => id,
            None => continue,
        };
        let lang_code = attrs
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("en");
        let language = match Language::parse(lang_code) {
            Some(l) => l,
            None => continue,
        };

        let mut c = SubtitleCandidate::new("opensubtitles_com", file_id.to_string(), language);
        c.release = attrs
            .get("release")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        c.hi = attrs
            .get("hearing_impaired")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        c.forced = attrs
            .get("foreign_parts_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        c.matched_by_hash = attrs
            .get("moviehash_match")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        c.format = Some("srt".into());

        if let Some(fd) = attrs.get("feature_details") {
            if let Some(imdb) = fd.get("imdb_id").and_then(|v| v.as_i64()) {
                c.hints.insert("imdb".into(), imdb.to_string());
            }
            if let Some(s) = fd.get("season_number").and_then(|v| v.as_i64()) {
                c.hints.insert("season".into(), s.to_string());
            }
            if let Some(e) = fd.get("episode_number").and_then(|v| v.as_i64()) {
                c.hints.insert("episode".into(), e.to_string());
            }
        }
        out.push(c);
    }
    out
}

#[async_trait]
impl Provider for OpenSubtitlesCom {
    fn name(&self) -> &str {
        "opensubtitles_com"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            movies: true,
            series: true,
            anime: true,
            keyless: false,
            ..Default::default()
        }
    }

    async fn list(&self, query: &Query) -> CoreResult<Vec<SubtitleCandidate>> {
        if self.api_key.is_empty() {
            return Err(CoreError::AuthRequired(
                "opensubtitles_com: api_key required".into(),
            ));
        }
        let media = &query.media;
        // Build params, then sort alphabetically (API requirement).
        let mut params: Vec<(String, String)> = Vec::new();
        let langs = query
            .languages
            .iter()
            .map(|l| l.alpha2())
            .collect::<Vec<_>>()
            .join(",");
        params.push(("languages".into(), langs));
        if let Some(imdb) = &media.ids.imdb {
            let digits: String = imdb.chars().filter(|c| c.is_ascii_digit()).collect();
            params.push(("imdb_id".into(), digits));
        } else {
            params.push(("query".into(), media.title.to_lowercase()));
        }
        if media.kind.is_episodic() {
            if let Some(s) = media.season {
                params.push(("season_number".into(), s.to_string()));
            }
            if let Some(e) = media.episode_num() {
                params.push(("episode_number".into(), e.to_string()));
            }
        }
        if let Some(h) = media.hashes.get("osdb") {
            params.push(("moviehash".into(), h.clone()));
        }
        params.sort_by(|a, b| a.0.cmp(&b.0));

        let resp = self
            .client
            .get(format!("{BASE}/subtitles"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", &self.user_agent)
            .query(&params)
            .send()
            .await
            .map_err(|e| http::net_err("opensubtitles_com", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(resp.status(), "opensubtitles_com"));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("opensubtitles_com: {e}")))?;
        Ok(parse_results(&json))
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> CoreResult<RawSubtitle> {
        // Step 1: request a download link for the file id.
        let body = serde_json::json!({ "file_id": candidate.id.parse::<i64>().unwrap_or(0) });
        let resp = self
            .client
            .post(format!("{BASE}/download"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| http::net_err("opensubtitles_com download", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(
                resp.status(),
                "opensubtitles_com download",
            ));
        }
        let dl: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("opensubtitles_com download: {e}")))?;
        let link = dl
            .get("link")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::Provider("opensubtitles_com: no link".into()))?;
        let file_name = dl
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("subtitle.srt")
            .to_string();

        // Step 2: fetch the actual file.
        let resp = self
            .client
            .get(link)
            .send()
            .await
            .map_err(|e| http::net_err("opensubtitles_com fetch", e))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| http::net_err("opensubtitles_com body", e))?
            .to_vec();
        Ok(RawSubtitle {
            filename: file_name,
            bytes,
            container: Container::Plain,
            language: candidate.language.clone(),
            provider: self.name().to_string(),
            release: candidate.release.clone(),
            hi: candidate.hi,
            forced: candidate.forced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_com_json() {
        let json = serde_json::json!({
            "data": [{
                "id": "7",
                "type": "subtitle",
                "attributes": {
                    "language": "en",
                    "release": "The.Show.S01E02.1080p.WEB",
                    "hearing_impaired": false,
                    "foreign_parts_only": false,
                    "moviehash_match": true,
                    "files": [{ "file_id": 555, "file_name": "x.srt" }],
                    "feature_details": { "imdb_id": 1234567, "season_number": 1, "episode_number": 2 }
                }
            }]
        });
        let v = parse_results(&json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "555");
        assert!(v[0].matched_by_hash);
        assert_eq!(v[0].hints.get("imdb").map(|s| s.as_str()), Some("1234567"));
    }
}
