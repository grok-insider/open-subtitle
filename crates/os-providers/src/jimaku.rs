//! Jimaku provider — the best source for **anime** subtitles. Matches by AniList
//! id (which the AniList refiner fills in), then lists files for the episode.
//! Needs a free API key, so it's default-disabled. Japanese-first, but English
//! files are surfaced when present.

use crate::http;
use async_trait::async_trait;
use os_core::ports::{Capabilities, Provider};
use os_core::{Container, CoreError, CoreResult, Language, Query, RawSubtitle, SubtitleCandidate};

const BASE: &str = "https://jimaku.cc/api";

/// Jimaku anime subtitle provider (requires `api_key`).
pub struct Jimaku {
    client: reqwest::Client,
    api_key: String,
}

impl Jimaku {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Jimaku { client, api_key }
    }
}

/// Guess a file's language from its name (Jimaku is JP-first).
fn lang_from_name(name: &str) -> Language {
    let n = name.to_lowercase();
    let code =
        if n.contains("eng") || n.contains(".en.") || n.contains("[en]") || n.contains("english") {
            "en"
        } else {
            "ja"
        };
    Language::parse(code).unwrap()
}

fn container_from_name(name: &str) -> Option<Container> {
    let n = name.to_lowercase();
    if n.ends_with(".zip") {
        Some(Container::Zip)
    } else if n.ends_with(".7z") || n.ends_with(".rar") {
        None // unsupported archive — skip
    } else {
        // .srt/.ass/.ssa/.vtt and anything else are treated as plain bytes.
        Some(Container::Plain)
    }
}

/// Parse a Jimaku `/files` response into candidates for a given episode.
pub fn parse_files(json: &serde_json::Value, episode: Option<u32>) -> Vec<SubtitleCandidate> {
    let arr = match json.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for f in arr {
        let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let url = f.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || url.is_empty() {
            continue;
        }
        let container = match container_from_name(name) {
            Some(c) => c,
            None => continue,
        };
        let language = lang_from_name(name);
        let mut c = SubtitleCandidate::new("jimaku", url, language);
        c.download_url = Some(url.to_string());
        c.release = Some(name.to_string());
        c.format = name.rsplit_once('.').map(|(_, e)| e.to_lowercase());
        if let Some(ep) = episode {
            c.hints.insert("episode".into(), ep.to_string());
        }
        // Stash the container choice for fetch.
        c.hints
            .insert("container".into(), format!("{container:?}").to_lowercase());
        out.push(c);
    }
    out
}

#[async_trait]
impl Provider for Jimaku {
    fn name(&self) -> &str {
        "jimaku"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            movies: false,
            series: false,
            anime: true,
            keyless: false,
            ..Default::default()
        }
    }

    async fn list(&self, query: &Query) -> CoreResult<Vec<SubtitleCandidate>> {
        if self.api_key.is_empty() {
            return Err(CoreError::AuthRequired("jimaku: api_key required".into()));
        }
        let media = &query.media;

        // 1. Find the entry by AniList id, else by name.
        let search_url = match media.ids.anilist {
            Some(id) => format!("{BASE}/entries/search?anilist_id={id}"),
            None => format!(
                "{BASE}/entries/search?query={}",
                urlencoding::encode(&media.title)
            ),
        };
        let entries: serde_json::Value = self
            .client
            .get(&search_url)
            .header("Authorization", &self.api_key)
            .send()
            .await
            .map_err(|e| http::net_err("jimaku search", e))?
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("jimaku search: {e}")))?;
        let entry_id = entries
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e.get("id"))
            .and_then(|v| v.as_i64());
        let entry_id = match entry_id {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        // 2. List files for the episode.
        let mut files_url = format!("{BASE}/entries/{entry_id}/files");
        if let Some(ep) = media.episode_num() {
            files_url.push_str(&format!("?episode={ep}"));
        }
        let files: serde_json::Value = self
            .client
            .get(&files_url)
            .header("Authorization", &self.api_key)
            .send()
            .await
            .map_err(|e| http::net_err("jimaku files", e))?
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("jimaku files: {e}")))?;

        Ok(parse_files(&files, media.episode_num()))
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> CoreResult<RawSubtitle> {
        let url = candidate
            .download_url
            .as_ref()
            .ok_or_else(|| CoreError::Provider("jimaku: missing url".into()))?;
        let resp = self
            .client
            .get(url)
            .header("Authorization", &self.api_key)
            .send()
            .await
            .map_err(|e| http::net_err("jimaku fetch", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(resp.status(), "jimaku fetch"));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| http::net_err("jimaku body", e))?
            .to_vec();
        let container = if candidate.hints.get("container").map(|s| s.as_str()) == Some("zip") {
            Container::Zip
        } else {
            Container::Plain
        };
        Ok(RawSubtitle {
            filename: candidate
                .release
                .clone()
                .unwrap_or_else(|| "jimaku.srt".into()),
            bytes,
            container,
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
    fn parses_files_and_langs() {
        let json = serde_json::json!([
            { "name": "[SubsPlease] Show - 01.srt", "url": "https://jimaku.cc/x/1.srt", "size": 1000 },
            { "name": "Show - 01 [eng].ass", "url": "https://jimaku.cc/x/2.ass", "size": 2000 },
            { "name": "archive.7z", "url": "https://jimaku.cc/x/3.7z", "size": 3000 }
        ]);
        let v = parse_files(&json, Some(1));
        // The .7z is skipped.
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].language.alpha3, "jpn");
        assert_eq!(v[1].language.alpha3, "eng");
        assert_eq!(v[0].hints.get("episode").map(|s| s.as_str()), Some("1"));
    }
}
