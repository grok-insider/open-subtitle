//! Domain model. Deliberately a *superset* of what any single provider needs —
//! providers read the fields they support. No I/O lives here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::lang::Language;

/// What kind of media we're finding subtitles for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Movie,
    Series,
    Anime,
}

impl MediaKind {
    /// Series and anime are episodic; movies are not.
    pub fn is_episodic(&self) -> bool {
        matches!(self, MediaKind::Series | MediaKind::Anime)
    }
}

/// The identifier matrix. Providers cherry-pick what they support; the scorer
/// uses ID→field equivalences to convert a strong ID match into many implied
/// matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdSet {
    /// IMDb id without the `tt` prefix (e.g. `1234567`).
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    pub tvdb: Option<u64>,
    /// Series-level ids (for episodes).
    pub series_imdb: Option<String>,
    pub series_tmdb: Option<u64>,
    pub series_tvdb: Option<u64>,
    /// Anime ids.
    pub anilist: Option<u64>,
    pub anidb_episode: Option<u64>,
    pub mal: Option<u64>,
}

/// Per-scheme content hashes, keyed by hasher name (e.g. `osdb`, `napiprojekt`).
pub type Hashes = BTreeMap<String, String>;

/// Release/technical attributes parsed from a filename or returned by a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub release_group: Option<String>,
    /// e.g. `BluRay`, `WEB-DL`, `WEBRip`, `HDTV`.
    pub source: Option<String>,
    /// e.g. `720p`, `1080p`, `2160p`.
    pub resolution: Option<String>,
    /// e.g. `x264`, `x265`, `AV1`.
    pub video_codec: Option<String>,
    /// e.g. `AAC`, `AC3`, `DTS`.
    pub audio_codec: Option<String>,
    pub streaming_service: Option<String>,
    pub edition: Option<String>,
}

/// The media we want subtitles for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Media {
    pub kind: MediaKind,
    pub ids: IdSet,
    /// Movie title, or series name for episodes.
    pub title: String,
    pub original_title: Option<String>,
    pub alternative_titles: Vec<String>,
    pub year: Option<i32>,
    /// Season (1 for flat-numbered anime/movies).
    pub season: Option<u32>,
    /// Supports multi-episode files (`S01E01-E02`).
    pub episodes: Vec<u32>,
    /// Episode title, when known.
    pub episode_title: Option<String>,
    pub release: ReleaseInfo,
    pub hashes: Hashes,
    pub size: Option<u64>,
    /// The original filename/release string this was derived from.
    pub name: Option<String>,
}

impl Default for Media {
    fn default() -> Self {
        Media {
            kind: MediaKind::Movie,
            ids: IdSet::default(),
            title: String::new(),
            original_title: None,
            alternative_titles: Vec::new(),
            year: None,
            season: None,
            episodes: Vec::new(),
            episode_title: None,
            release: ReleaseInfo::default(),
            hashes: Hashes::new(),
            size: None,
            name: None,
        }
    }
}

impl Media {
    /// A new movie with a title.
    pub fn movie(title: impl Into<String>) -> Media {
        Media {
            kind: MediaKind::Movie,
            title: title.into(),
            ..Default::default()
        }
    }

    /// A new episode of a series.
    pub fn episode(series: impl Into<String>, season: u32, episode: u32) -> Media {
        Media {
            kind: MediaKind::Series,
            title: series.into(),
            season: Some(season),
            episodes: vec![episode],
            ..Default::default()
        }
    }

    /// A new anime episode (flat-numbered; season defaults to 1).
    pub fn anime(series: impl Into<String>, episode: u32) -> Media {
        Media {
            kind: MediaKind::Anime,
            title: series.into(),
            season: Some(1),
            episodes: vec![episode],
            ..Default::default()
        }
    }

    /// Best display title with fallbacks.
    pub fn display_title(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else if let Some(o) = &self.original_title {
            o
        } else {
            "Untitled"
        }
    }

    /// The first episode number (for multi-episode files).
    pub fn episode_num(&self) -> Option<u32> {
        self.episodes.iter().min().copied()
    }

    /// The `SxxEyy` coordinate (episodic media only).
    pub fn coordinate(&self) -> Option<String> {
        if self.kind.is_episodic() {
            Some(format!(
                "S{:02}E{:02}",
                self.season.unwrap_or(1),
                self.episode_num().unwrap_or(1)
            ))
        } else {
            None
        }
    }

    /// A human label like `Series - S01E01 - Title` or `Movie (2009)`.
    pub fn label(&self) -> String {
        if self.kind.is_episodic() {
            let coord = self.coordinate().unwrap_or_default();
            match &self.episode_title {
                Some(t) if !t.is_empty() => format!("{} - {} - {}", self.display_title(), coord, t),
                _ => format!("{} - {}", self.display_title(), coord),
            }
        } else {
            match self.year {
                Some(y) => format!("{} ({})", self.display_title(), y),
                None => self.display_title().to_string(),
            }
        }
    }
}

/// What container a freshly-fetched subtitle's bytes are wrapped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Plain,
    Gzip,
    Zip,
    Rar,
    Xz,
    Unknown,
}

/// A subtitle the engine could download, as listed by a provider (metadata only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleCandidate {
    pub provider: String,
    /// Provider-specific id used by `fetch` (file id, url, slug…).
    pub id: String,
    pub language: Language,
    /// Release/filename string for display + match fusion.
    pub release: Option<String>,
    pub hi: bool,
    pub forced: bool,
    pub format: Option<String>,
    /// Direct download URL, when the provider exposes one.
    pub download_url: Option<String>,
    /// The provider asserts this matched the file hash.
    pub matched_by_hash: bool,
    /// Provider-specific structured hints used by matching, e.g.
    /// `imdb`, `season`, `episode`, `year`, `downloads`, `fps`.
    pub hints: BTreeMap<String, String>,
    /// Filled by the engine's scorer (not serialized as input).
    #[serde(default)]
    pub score: i32,
    #[serde(default)]
    pub score_without_hash: i32,
}

impl SubtitleCandidate {
    pub fn new(provider: impl Into<String>, id: impl Into<String>, language: Language) -> Self {
        SubtitleCandidate {
            provider: provider.into(),
            id: id.into(),
            language,
            release: None,
            hi: false,
            forced: false,
            format: None,
            download_url: None,
            matched_by_hash: false,
            hints: BTreeMap::new(),
            score: 0,
            score_without_hash: 0,
        }
    }
}

/// Raw bytes as fetched from a provider, possibly compressed/archived.
#[derive(Debug, Clone)]
pub struct RawSubtitle {
    /// Suggested filename (used for format detection and archive member naming).
    pub filename: String,
    pub bytes: Vec<u8>,
    pub container: Container,
    pub language: Language,
    pub provider: String,
    pub release: Option<String>,
    pub hi: bool,
    pub forced: bool,
}

/// A materialized, decoded, post-processed subtitle ready to deliver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleFile {
    pub language: Language,
    /// Subtitle format extension, e.g. `srt`, `ass`, `vtt`.
    pub format: String,
    /// UTF-8 text content.
    pub text: String,
    pub provider: String,
    pub release: Option<String>,
    pub hi: bool,
    pub forced: bool,
}

impl SubtitleFile {
    /// The sidecar filename for a given video stem, e.g. `Movie.en.srt`.
    pub fn sidecar_name(&self, video_stem: &str) -> String {
        let mut tag = self.language.alpha2();
        if let Some(r) = &self.language.region {
            tag.push('-');
            tag.push_str(r);
        }
        if self.hi {
            tag.push_str(".hi");
        }
        if self.forced {
            tag.push_str(".forced");
        }
        format!("{video_stem}.{tag}.{}", self.format)
    }
}

/// A search request handed to providers.
#[derive(Debug, Clone)]
pub struct Query {
    pub media: Media,
    pub languages: Vec<Language>,
}
