//! Sonarr/Radarr webhook parsing (pure functions).
//!
//! Turns an "On Import" (`eventType: Download`/`Import`) payload into a [`Job`]
//! describing what to fetch subtitles for. Detects Sonarr (`series`/
//! `episodeFile`) vs Radarr (`movie`/`movieFile`). Pure + unit-tested; the wiring
//! lives in `main.rs`. Field shapes follow Sonarr/Radarr's `WebhookImportPayload`.

use os_core::MediaKind;
use serde_json::Value;

/// What to do with a received webhook.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Fetch subtitles for this imported media.
    Import(Box<Job>),
    /// A "Test" ping from the *arr UI — acknowledge, do nothing.
    Test,
    /// Anything else (Grab/Rename/Health/…) — ignore with a reason.
    Ignore(String),
}

/// The metadata needed to find subtitles for an imported file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Job {
    pub kind: MediaKind,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    /// Movie-level IMDb (digits only) — authoritative for movies.
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    /// Series-level ids (for episodes).
    pub series_imdb: Option<String>,
    pub series_tvdb: Option<u64>,
    /// Absolute path to the imported media file, if present.
    pub file_path: Option<String>,
    pub source: &'static str, // "sonarr" | "radarr"
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}
fn i32_of(v: &Value, key: &str) -> Option<i32> {
    v.get(key).and_then(|x| x.as_i64()).map(|n| n as i32)
}
fn u32_of(v: &Value, key: &str) -> Option<u32> {
    v.get(key).and_then(|x| x.as_i64()).map(|n| n as u32)
}
fn u64_of(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|x| x.as_u64())
}
fn digits(s: &str) -> Option<String> {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() {
        None
    } else {
        Some(d)
    }
}

/// Parse a Sonarr/Radarr webhook payload into an [`Action`].
pub fn parse(payload: &Value) -> Action {
    let event = payload
        .get("eventType")
        .and_then(|e| e.as_str())
        .unwrap_or("");

    if event.eq_ignore_ascii_case("Test") {
        return Action::Test;
    }
    if !(event.eq_ignore_ascii_case("Download") || event.eq_ignore_ascii_case("Import")) {
        return Action::Ignore(format!("eventType={event}"));
    }

    // Radarr if it carries a movie/movieFile; otherwise treat as Sonarr.
    let is_radarr = payload.get("movie").is_some() || payload.get("movieFile").is_some();
    let job = if is_radarr {
        parse_radarr(payload)
    } else {
        parse_sonarr(payload)
    };

    match job {
        Some(j) if !j.title.is_empty() => Action::Import(Box::new(j)),
        _ => Action::Ignore("missing title/metadata".into()),
    }
}

fn parse_radarr(payload: &Value) -> Option<Job> {
    let movie = payload.get("movie")?;
    let title = s(movie, "title")?;
    let file = payload.get("movieFile");
    let file_path = file.and_then(|f| s(f, "path")).or_else(|| {
        join_path(
            s(movie, "folderPath"),
            file.and_then(|f| s(f, "relativePath")),
        )
    });

    Some(Job {
        kind: MediaKind::Movie,
        title,
        year: i32_of(movie, "year"),
        imdb: s(movie, "imdbId").and_then(|i| digits(&i)),
        tmdb: u64_of(movie, "tmdbId"),
        file_path,
        source: "radarr",
        ..Default::default()
    })
}

fn parse_sonarr(payload: &Value) -> Option<Job> {
    let series = payload.get("series")?;
    let title = s(series, "title")?;
    let ep = payload
        .get("episodes")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first());

    let kind = match s(series, "type").as_deref() {
        Some("anime") => MediaKind::Anime,
        _ => MediaKind::Series,
    };

    let file = payload.get("episodeFile");
    let file_path = file
        .and_then(|f| s(f, "path"))
        .or_else(|| join_path(s(series, "path"), file.and_then(|f| s(f, "relativePath"))));

    Some(Job {
        kind,
        title,
        year: i32_of(series, "year"),
        season: ep.and_then(|e| u32_of(e, "seasonNumber")),
        episode: ep.and_then(|e| u32_of(e, "episodeNumber")),
        episode_title: ep.and_then(|e| s(e, "title")),
        series_imdb: s(series, "imdbId").and_then(|i| digits(&i)),
        series_tvdb: u64_of(series, "tvdbId"),
        file_path,
        source: "sonarr",
        ..Default::default()
    })
}

fn join_path(dir: Option<String>, rel: Option<String>) -> Option<String> {
    match (dir, rel) {
        (Some(d), Some(r)) => Some(format!(
            "{}/{}",
            d.trim_end_matches('/'),
            r.trim_start_matches('/')
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sonarr_import_episode() {
        let p = json!({
            "eventType": "Download",
            "series": { "title": "The Eminence in Shadow", "path": "/tv/Eminence",
                        "tvdbId": 415978, "imdbId": "tt15679884", "type": "anime", "year": 2022 },
            "episodes": [ { "seasonNumber": 1, "episodeNumber": 1, "title": "The Hated Classmate" } ],
            "episodeFile": { "relativePath": "Season 01/ep01.mkv",
                             "path": "/tv/Eminence/Season 01/ep01.mkv" },
            "isUpgrade": false
        });
        match parse(&p) {
            Action::Import(j) => {
                assert_eq!(j.kind, MediaKind::Anime);
                assert_eq!(j.title, "The Eminence in Shadow");
                assert_eq!(j.season, Some(1));
                assert_eq!(j.episode, Some(1));
                assert_eq!(
                    j.file_path.as_deref(),
                    Some("/tv/Eminence/Season 01/ep01.mkv")
                );
                assert_eq!(j.series_tvdb, Some(415978));
                assert_eq!(j.series_imdb.as_deref(), Some("15679884"));
                assert_eq!(j.source, "sonarr");
            }
            other => panic!("expected Import, got {other:?}"),
        }
    }

    #[test]
    fn sonarr_path_fallback_from_relative() {
        let p = json!({
            "eventType": "Download",
            "series": { "title": "Show", "path": "/tv/Show", "type": "standard" },
            "episodes": [ { "seasonNumber": 2, "episodeNumber": 5 } ],
            "episodeFile": { "relativePath": "Season 02/e05.mkv" }  // no absolute path
        });
        match parse(&p) {
            Action::Import(j) => {
                assert_eq!(j.file_path.as_deref(), Some("/tv/Show/Season 02/e05.mkv"));
                assert_eq!(j.kind, MediaKind::Series);
            }
            other => panic!("expected Import, got {other:?}"),
        }
    }

    #[test]
    fn radarr_import_movie() {
        let p = json!({
            "eventType": "Download",
            "movie": { "title": "Interstellar", "year": 2014, "folderPath": "/movies/Interstellar (2014)",
                       "tmdbId": 157336, "imdbId": "tt0816692" },
            "movieFile": { "relativePath": "Interstellar.2014.mkv",
                           "path": "/movies/Interstellar (2014)/Interstellar.2014.mkv" },
            "isUpgrade": false
        });
        match parse(&p) {
            Action::Import(j) => {
                assert_eq!(j.kind, MediaKind::Movie);
                assert_eq!(j.title, "Interstellar");
                assert_eq!(j.year, Some(2014));
                assert_eq!(j.imdb.as_deref(), Some("0816692"));
                assert_eq!(j.tmdb, Some(157336));
                assert_eq!(
                    j.file_path.as_deref(),
                    Some("/movies/Interstellar (2014)/Interstellar.2014.mkv")
                );
                assert_eq!(j.source, "radarr");
            }
            other => panic!("expected Import, got {other:?}"),
        }
    }

    #[test]
    fn test_event_is_acknowledged() {
        let p = json!({ "eventType": "Test", "series": { "title": "Test Title", "path": "C:\\x" },
                        "episodes": [ { "seasonNumber": 1, "episodeNumber": 1 } ] });
        assert_eq!(parse(&p), Action::Test);
    }

    #[test]
    fn grab_event_is_ignored() {
        let p = json!({ "eventType": "Grab", "series": { "title": "X" } });
        assert!(matches!(parse(&p), Action::Ignore(_)));
    }
}
