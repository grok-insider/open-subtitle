//! SubDL provider. Search needs a (free) API key; downloads come from
//! `dl.subdl.com` (generous anonymous per-IP quota). Default-disabled until a key
//! is configured, to preserve the keyless-by-default invariant.

use crate::http;
use async_trait::async_trait;
use os_core::ports::{Capabilities, Provider};
use os_core::{Container, CoreError, CoreResult, Language, Query, RawSubtitle, SubtitleCandidate};

const API: &str = "https://api.subdl.com/api/v1/subtitles";
const DL_BASE: &str = "https://dl.subdl.com";

/// SubDL provider (requires `api_key`).
pub struct SubDl {
    client: reqwest::Client,
    api_key: String,
}

impl SubDl {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        SubDl { client, api_key }
    }
}

/// Parse the SubDL JSON response (`{ subtitles: [...] }`) into candidates.
pub fn parse_results(json: &serde_json::Value) -> Vec<SubtitleCandidate> {
    let arr = match json.get("subtitles").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in arr {
        let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let lang_code = item
            .get("language")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("lang").and_then(|v| v.as_str()))
            .unwrap_or("EN");
        let language = match Language::parse(lang_code) {
            Some(l) => l,
            None => continue,
        };

        let mut c = SubtitleCandidate::new("subdl", url, language);
        c.download_url = Some(format!("{DL_BASE}{url}"));
        c.release = item
            .get("release_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("name").and_then(|v| v.as_str()))
            .map(|s| s.to_string());
        c.hi = item.get("hi").and_then(|v| v.as_bool()).unwrap_or(false);
        if let Some(s) = item.get("season").and_then(|v| v.as_i64()) {
            c.hints.insert("season".into(), s.to_string());
        }
        if let Some(e) = item.get("episode").and_then(|v| v.as_i64()) {
            c.hints.insert("episode".into(), e.to_string());
        }
        c.format = Some("srt".into());
        out.push(c);
    }
    out
}

#[async_trait]
impl Provider for SubDl {
    fn name(&self) -> &str {
        "subdl"
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
            return Err(CoreError::AuthRequired("subdl: api_key required".into()));
        }
        let media = &query.media;
        let langs = query
            .languages
            .iter()
            .map(|l| l.alpha2().to_uppercase())
            .collect::<Vec<_>>()
            .join(",");

        let mut req = self
            .client
            .get(API)
            .query(&[("api_key", self.api_key.as_str())])
            .query(&[("languages", langs.as_str())])
            .query(&[("subs_per_page", "30")]);

        if let Some(imdb) = &media.ids.imdb {
            req = req.query(&[("imdb_id", imdb.as_str())]);
        } else {
            req = req.query(&[("film_name", media.title.as_str())]);
        }
        if media.kind.is_episodic() {
            req = req.query(&[("type", "tv")]);
            if let Some(s) = media.season {
                req = req.query(&[("season_number", s.to_string().as_str())]);
            }
            if let Some(e) = media.episode_num() {
                req = req.query(&[("episode_number", e.to_string().as_str())]);
            }
        } else {
            req = req.query(&[("type", "movie")]);
        }

        let resp = req.send().await.map_err(|e| http::net_err("subdl", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(resp.status(), "subdl"));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("subdl: {e}")))?;
        Ok(parse_results(&json))
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> CoreResult<RawSubtitle> {
        let url = candidate
            .download_url
            .as_ref()
            .ok_or_else(|| CoreError::Provider("subdl: missing url".into()))?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| http::net_err("subdl fetch", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(resp.status(), "subdl fetch"));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| http::net_err("subdl body", e))?
            .to_vec();
        Ok(RawSubtitle {
            filename: candidate
                .release
                .clone()
                .unwrap_or_else(|| "subdl.zip".into()),
            bytes,
            container: Container::Zip,
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
    fn parses_subdl_json() {
        let json = serde_json::json!({
            "status": true,
            "subtitles": [
                {
                    "release_name": "The.Show.S01E02.1080p.WEB",
                    "name": "The Show",
                    "language": "EN",
                    "url": "/subtitle/123-456.zip",
                    "season": 1,
                    "episode": 2,
                    "hi": false
                }
            ]
        });
        let v = parse_results(&json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].language.alpha3, "eng");
        assert_eq!(
            v[0].download_url.as_deref(),
            Some("https://dl.subdl.com/subtitle/123-456.zip")
        );
        assert_eq!(v[0].hints.get("episode").map(|s| s.as_str()), Some("2"));
    }
}
