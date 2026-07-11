//! OpenSubtitles.org legacy REST provider — **keyless**, the default primary.
//!
//! Endpoint: `https://rest.opensubtitles.org/search/<segments>` where segments
//! are `key-value` pairs joined by `/` in **alphabetical key order**. Returns a
//! JSON array. Download links point at gzip files. A user agent is required;
//! `TemporaryUserAgent` is accepted by the host for light use.

use crate::http;
use async_trait::async_trait;
use os_core::ports::{Capabilities, Provider};
use os_core::{Container, CoreError, CoreResult, Language, Query, RawSubtitle, SubtitleCandidate};

const BASE: &str = "https://rest.opensubtitles.org/search";
const UA: &str = "TemporaryUserAgent";

/// Keyless OpenSubtitles.org provider.
pub struct OpenSubtitlesOrg {
    client: reqwest::Client,
}

impl OpenSubtitlesOrg {
    pub fn new(client: reqwest::Client) -> Self {
        OpenSubtitlesOrg { client }
    }

    /// One `sublanguageid-…` segment for a single language code.
    ///
    /// The rest.opensubtitles.org endpoint rejects multi-value parameters
    /// (`sublanguageid-eng,spa` → HTTP 400 "multiple values per parameter are
    /// not supported"), so each language is searched as its own request.
    fn lang_seg_one(lang: &Language) -> (String, String) {
        ("sublanguageid".into(), lang.alpha3b().to_string())
    }

    /// Build the alphabetical `key-value/...` path from segments.
    fn build_url(&self, segs: &[(String, String)]) -> String {
        let mut segs = segs.to_vec();
        segs.sort_by(|a, b| a.0.cmp(&b.0));
        let path = segs
            .iter()
            .map(|(k, v)| format!("{k}-{}", urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("/");
        format!("{BASE}/{path}")
    }

    /// Search URLs for hash / imdb / text queries, fanned out **per language**.
    ///
    /// The endpoint ANDs all segments, so a non-matching hash must not be mixed
    /// with the text query — we run them separately and merge the results. Same
    /// for imdbid vs query. Languages are also separate (API rejects multi-lang
    /// `sublanguageid`).
    fn build_urls(&self, q: &Query) -> Vec<String> {
        let media = &q.media;

        // `None` = no language filter (empty request list).
        let langs: Vec<Option<&Language>> = if q.languages.is_empty() {
            vec![None]
        } else {
            q.languages.iter().map(Some).collect()
        };

        let mut urls = Vec::new();

        // Episodic season/episode segments appended to a metadata search.
        let ep_segs = |segs: &mut Vec<(String, String)>| {
            if media.kind.is_episodic() {
                if let Some(s) = media.season {
                    segs.push(("season".into(), s.to_string()));
                }
                if let Some(e) = media.episode_num() {
                    segs.push(("episode".into(), e.to_string()));
                }
            }
        };

        for lang in langs {
            let push_lang = |segs: &mut Vec<(String, String)>| {
                if let Some(l) = lang {
                    segs.push(Self::lang_seg_one(l));
                }
            };

            // Hash search.
            if let Some(h) = media.hashes.get("osdb") {
                let mut segs: Vec<(String, String)> = Vec::new();
                push_lang(&mut segs);
                segs.push(("moviehash".into(), h.clone()));
                if let Some(size) = media.size {
                    segs.push(("moviebytesize".into(), size.to_string()));
                }
                urls.push(self.build_url(&segs));
            }

            // IMDb-id search (ALONE — opensubtitles.org returns nothing when
            // imdbid and query are combined, even though each works on its own).
            if let Some(imdb) = &media.ids.imdb {
                let digits: String = imdb.chars().filter(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    let mut segs: Vec<(String, String)> = Vec::new();
                    push_lang(&mut segs);
                    segs.push(("imdbid".into(), digits));
                    ep_segs(&mut segs);
                    urls.push(self.build_url(&segs));
                }
            }

            // Text-query search (separate from the imdbid search).
            if !media.title.is_empty() {
                let mut segs: Vec<(String, String)> = Vec::new();
                push_lang(&mut segs);
                segs.push(("query".into(), media.title.to_lowercase()));
                ep_segs(&mut segs);
                urls.push(self.build_url(&segs));
            }
        }

        urls
    }
}

/// Parse the JSON array returned by the search endpoint into candidates.
pub fn parse_results(json: &serde_json::Value) -> Vec<SubtitleCandidate> {
    let arr = match json.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in arr {
        let id = item
            .get("IDSubtitleFile")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let dl = item
            .get("SubDownloadLink")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if id.is_empty() || dl.is_empty() {
            continue;
        }

        // Language: prefer ISO639 (alpha-2), fall back to SubLanguageID (alpha-3B).
        let lang_code = item
            .get("ISO639")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("SubLanguageID").and_then(|v| v.as_str()))
            .unwrap_or("en");
        let language = match Language::parse(lang_code) {
            Some(l) => l,
            None => continue,
        };

        let mut c = SubtitleCandidate::new("opensubtitles_org", id, language);
        c.download_url = Some(dl.to_string());
        c.release = item
            .get("MovieReleaseName")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("SubFileName").and_then(|v| v.as_str()))
            .map(|s| s.to_string());
        c.format = item
            .get("SubFormat")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());
        c.hi = item.get("SubHearingImpaired").and_then(|v| v.as_str()) == Some("1");
        c.matched_by_hash = item.get("MatchedBy").and_then(|v| v.as_str()) == Some("moviehash");

        if let Some(imdb) = item.get("IDMovieImdb").and_then(|v| v.as_str()) {
            if !imdb.is_empty() && imdb != "0" {
                c.hints.insert("imdb".into(), imdb.to_string());
            }
        }
        if let Some(s) = item.get("SeriesSeason").and_then(|v| v.as_str()) {
            if s != "0" && !s.is_empty() {
                c.hints.insert("season".into(), s.to_string());
            }
        }
        if let Some(e) = item.get("SeriesEpisode").and_then(|v| v.as_str()) {
            if e != "0" && !e.is_empty() {
                c.hints.insert("episode".into(), e.to_string());
            }
        }
        if let Some(d) = item.get("SubDownloadsCnt").and_then(|v| v.as_str()) {
            c.hints.insert("downloads".into(), d.to_string());
        }

        out.push(c);
    }
    out
}

#[async_trait]
impl Provider for OpenSubtitlesOrg {
    fn name(&self) -> &str {
        "opensubtitles_org"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            movies: true,
            series: true,
            anime: true,
            keyless: true,
            ..Default::default()
        }
    }

    async fn list(&self, query: &Query) -> CoreResult<Vec<SubtitleCandidate>> {
        let urls = self.build_urls(query);
        let mut merged: Vec<SubtitleCandidate> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_err = None;

        for url in &urls {
            let resp = match self.client.get(url).header("User-Agent", UA).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(http::net_err("opensubtitles_org", e));
                    continue;
                }
            };
            if !resp.status().is_success() {
                last_err = Some(http::status_to_error(resp.status(), "opensubtitles_org"));
                continue;
            }
            let json: serde_json::Value = match resp.json().await {
                Ok(j) => j,
                Err(e) => {
                    last_err = Some(CoreError::Parse(format!("opensubtitles_org: {e}")));
                    continue;
                }
            };
            for c in parse_results(&json) {
                if seen.insert(c.id.clone()) {
                    merged.push(c);
                }
            }
        }

        // Only surface an error if every request failed and nothing came back.
        if merged.is_empty() {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(merged)
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> CoreResult<RawSubtitle> {
        let url = candidate
            .download_url
            .as_ref()
            .ok_or_else(|| CoreError::Provider("missing download url".into()))?;
        let resp = self
            .client
            .get(url)
            .header("User-Agent", UA)
            .send()
            .await
            .map_err(|e| http::net_err("opensubtitles_org fetch", e))?;
        if !resp.status().is_success() {
            return Err(http::status_to_error(
                resp.status(),
                "opensubtitles_org fetch",
            ));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| http::net_err("opensubtitles_org body", e))?
            .to_vec();
        let filename = candidate
            .release
            .clone()
            .unwrap_or_else(|| format!("{}.srt", candidate.id));
        Ok(RawSubtitle {
            filename,
            bytes,
            container: Container::Gzip,
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
    use os_core::{Media, Query};

    #[test]
    fn builds_separate_hash_and_text_searches() {
        let p = OpenSubtitlesOrg::new(reqwest::Client::new());
        let mut media = Media::episode("The Show", 1, 2);
        media.hashes.insert("osdb".into(), "abc123".into());
        media.size = Some(700_000_000);
        let q = Query {
            media,
            languages: vec![
                Language::parse("en").unwrap(),
                Language::parse("es").unwrap(),
            ],
        };
        let urls = p.build_urls(&q);
        // Per language × (hash + text) = 4 URLs. Languages are NOT comma-joined
        // (API returns 400 for multi-value sublanguageid).
        assert_eq!(urls.len(), 4);
        let hash_urls: Vec<_> = urls
            .iter()
            .filter(|u| u.contains("moviehash-abc123"))
            .collect();
        let text_urls: Vec<_> = urls.iter().filter(|u| u.contains("query-")).collect();
        assert_eq!(hash_urls.len(), 2);
        assert_eq!(text_urls.len(), 2);
        for u in &hash_urls {
            assert!(!u.contains("query-"));
        }
        for u in &text_urls {
            assert!(u.contains("episode-2"));
            assert!(u.contains("season-1"));
        }
        let joined = urls.join("\n");
        assert!(joined.contains("sublanguageid-eng"));
        assert!(joined.contains("sublanguageid-spa"));
        assert!(!joined.contains("eng%2Cspa") && !joined.contains("eng,spa"));
    }

    #[test]
    fn imdbid_and_query_are_separate_searches() {
        // opensubtitles.org returns nothing when imdbid+query are combined, so
        // they must be issued as distinct searches.
        let p = OpenSubtitlesOrg::new(reqwest::Client::new());
        let mut media = Media::movie("Interstellar");
        media.ids.imdb = Some("0816692".into());
        let q = Query {
            media,
            languages: vec![Language::parse("en").unwrap()],
        };
        let urls = p.build_urls(&q);
        let imdb_url = urls.iter().find(|u| u.contains("imdbid-0816692")).unwrap();
        let query_url = urls.iter().find(|u| u.contains("query-")).unwrap();
        // Neither search mixes imdbid with query.
        assert!(!imdb_url.contains("query-"));
        assert!(!query_url.contains("imdbid-"));
    }

    #[test]
    fn parses_results() {
        let json = serde_json::json!([
            {
                "IDSubtitleFile": "999",
                "SubFileName": "The.Show.S01E02.1080p.srt",
                "SubLanguageID": "eng",
                "ISO639": "en",
                "SubDownloadLink": "https://dl.opensubtitles.org/x/999.gz",
                "SubFormat": "srt",
                "SubHearingImpaired": "0",
                "MatchedBy": "moviehash",
                "MovieReleaseName": "The.Show.S01E02.1080p.WEB-DL",
                "SeriesSeason": "1",
                "SeriesEpisode": "2",
                "IDMovieImdb": "1234567"
            }
        ]);
        let v = parse_results(&json);
        assert_eq!(v.len(), 1);
        let c = &v[0];
        assert_eq!(c.language.alpha3, "eng");
        assert!(c.matched_by_hash);
        assert_eq!(c.hints.get("season").map(|s| s.as_str()), Some("1"));
        assert_eq!(c.hints.get("imdb").map(|s| s.as_str()), Some("1234567"));
        assert!(c.download_url.as_deref().unwrap().ends_with(".gz"));
    }
}
