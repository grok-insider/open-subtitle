//! The port traits. Small, focused, object-safe `async` traits. Adapters in the
//! other crates implement these; `os-engine` depends only on this surface.

use crate::error::CoreResult;
use crate::model::{
    Language, Media, MediaKind, Query, RawSubtitle, SubtitleCandidate, SubtitleFile,
};
use crate::score::Score;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// What a provider can do — lets the engine pre-filter cheaply before any I/O.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub movies: bool,
    pub series: bool,
    pub anime: bool,
    /// Requires a content hash to search.
    pub needs_hash: bool,
    /// Requires an IMDb id.
    pub needs_imdb: bool,
    /// Requires a TVDB id.
    pub needs_tvdb: bool,
    /// Requires an AniDB episode id.
    pub needs_anidb: bool,
    /// Works with no API key / login.
    pub keyless: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            movies: true,
            series: true,
            anime: false,
            needs_hash: false,
            needs_imdb: false,
            needs_tvdb: false,
            needs_anidb: false,
            keyless: true,
        }
    }
}

impl Capabilities {
    /// Whether this provider can handle the given media kind.
    pub fn handles(&self, kind: MediaKind) -> bool {
        match kind {
            MediaKind::Movie => self.movies,
            MediaKind::Series => self.series,
            MediaKind::Anime => self.anime,
        }
    }
}

/// A subtitle source.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// List candidate subtitles (metadata only — no content fetched yet).
    async fn list(&self, query: &Query) -> CoreResult<Vec<SubtitleCandidate>>;
    /// Fetch the raw bytes for a candidate (possibly compressed/archived).
    async fn fetch(&self, candidate: &SubtitleCandidate) -> CoreResult<RawSubtitle>;
}

/// Input for identifying media.
#[derive(Debug, Clone, Default)]
pub struct MediaInput {
    pub path: Option<PathBuf>,
    /// Filename or release string.
    pub name: Option<String>,
    /// Free-text query (overrides parsed title).
    pub query: Option<String>,
    pub kind_hint: Option<MediaKind>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// Turns a file/name/query into a rich [`Media`].
#[async_trait]
pub trait Identifier: Send + Sync {
    async fn identify(&self, input: &MediaInput) -> CoreResult<Media>;
}

/// Computes a content hash of a media file.
pub trait Hasher: Send + Sync {
    /// Hash scheme name (the key under which it's stored in `Media::hashes`).
    fn name(&self) -> &str;
    /// Returns `None` when the file is too small/unsupported for this scheme.
    fn hash_file(&self, path: &Path) -> CoreResult<Option<String>>;
}

/// Enriches a [`Media`] in place (online ids, local metadata, …). Best-effort.
#[async_trait]
pub trait Refiner: Send + Sync {
    fn name(&self) -> &str;
    async fn refine(&self, media: &mut Media) -> CoreResult<()>;
}

/// Options for post-processing a fetched subtitle.
#[derive(Debug, Clone)]
pub struct ProcessOpts {
    pub to_utf8: bool,
    pub target_format: Option<String>,
    pub remove_hi: bool,
    pub language: Option<Language>,
}

impl Default for ProcessOpts {
    fn default() -> Self {
        ProcessOpts {
            to_utf8: true,
            target_format: None,
            remove_hi: false,
            language: None,
        }
    }
}

/// Decodes/normalizes/converts a fetched subtitle into a [`SubtitleFile`].
pub trait PostProcessor: Send + Sync {
    fn process(&self, raw: RawSubtitle, opts: &ProcessOpts) -> CoreResult<SubtitleFile>;
}

/// Scores a candidate against the target media (pure, swappable).
pub trait Scorer: Send + Sync {
    fn score(&self, candidate: &SubtitleCandidate, media: &Media) -> Score;
}

/// Aligns a subtitle to a reference (video audio or another subtitle).
#[async_trait]
pub trait Synchronizer: Send + Sync {
    fn name(&self) -> &str;
    async fn sync(&self, sub: &SubtitleFile, reference: &Path) -> CoreResult<SubtitleFile>;
}

/// Translates a subtitle into a target language.
#[async_trait]
pub trait Translator: Send + Sync {
    fn name(&self) -> &str;
    async fn translate(&self, sub: &SubtitleFile, to: &Language) -> CoreResult<SubtitleFile>;
}

/// Generates a subtitle from audio (last-resort fallback).
#[async_trait]
pub trait Transcriber: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe(
        &self,
        media_path: &Path,
        lang: Option<&Language>,
    ) -> CoreResult<SubtitleFile>;
}
