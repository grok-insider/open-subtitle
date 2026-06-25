//! The filename/release-string identifier — turns a path or name into a `Media`
//! using the pure `os_core::guess` parser, then optionally hashes the file.

use crate::hash::OsdbHasher;
use async_trait::async_trait;
use os_core::ports::{Identifier, MediaInput};
use os_core::{guess, CoreResult, Hasher, Media, MediaKind};
use std::path::Path;

/// Identifies media from its filename/release string (and hashes the file when a
/// path is given). Anime-vs-series is inferred from the release shape, or forced
/// via `MediaInput::kind_hint`.
#[derive(Debug, Clone, Default)]
pub struct FilenameIdentifier {
    hasher: OsdbHasher,
}

impl FilenameIdentifier {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Identifier for FilenameIdentifier {
    async fn identify(&self, input: &MediaInput) -> CoreResult<Media> {
        // Determine the name to parse.
        let name = input
            .name
            .clone()
            .or_else(|| {
                input
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .or_else(|| input.query.clone())
            .unwrap_or_default();

        let g = guess::guess(&name);

        // Was there a bracket group prefix (anime-like)? Re-derive cheaply.
        let anime_like = name.trim_start().starts_with('[');
        let mut kind = input.kind_hint.unwrap_or_else(|| g.kind(anime_like));

        // A free-text query with no episode markers is most likely a movie unless
        // the caller hinted otherwise.
        if input.query.is_some() && !g.is_episode && input.kind_hint.is_none() {
            kind = MediaKind::Movie;
        }

        let title = input
            .query
            .clone()
            .or_else(|| g.title.clone())
            .unwrap_or_else(|| name.clone());

        let season = input
            .season
            .or(g.season)
            .or(if kind.is_episodic() { Some(1) } else { None });
        let episodes = match input.episode {
            Some(e) => vec![e],
            None => g.episodes.clone(),
        };

        let mut media = Media {
            kind,
            title,
            year: g.year,
            season,
            episodes,
            release: g.release_info(),
            name: Some(name),
            ..Default::default()
        };

        // Hash the file if we have a path.
        if let Some(path) = &input.path {
            if let Ok(Some(h)) = self.hasher.hash_file(Path::new(path)) {
                media.hashes.insert(self.hasher.name().to_string(), h);
            }
            if let Ok(meta) = std::fs::metadata(path) {
                media.size = Some(meta.len());
            }
        }

        Ok(media)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn identifies_anime_from_name() {
        let id = FilenameIdentifier::new();
        let input = MediaInput {
            name: Some(
                "[SubsPlease] Kage no Jitsuryokusha ni Naritakute! - 01 (1080p) [ABCD].mkv".into(),
            ),
            ..Default::default()
        };
        let m = id.identify(&input).await.unwrap();
        assert_eq!(m.kind, MediaKind::Anime);
        assert_eq!(m.episodes, vec![1]);
        assert_eq!(m.season, Some(1));
        assert!(m.title.contains("Kage no Jitsuryokusha"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn identifies_movie_from_query() {
        let id = FilenameIdentifier::new();
        let input = MediaInput {
            query: Some("Interstellar".into()),
            ..Default::default()
        };
        let m = id.identify(&input).await.unwrap();
        assert_eq!(m.kind, MediaKind::Movie);
        assert_eq!(m.title, "Interstellar");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn respects_explicit_season_episode() {
        let id = FilenameIdentifier::new();
        let input = MediaInput {
            query: Some("The Show".into()),
            kind_hint: Some(MediaKind::Series),
            season: Some(2),
            episode: Some(5),
            ..Default::default()
        };
        let m = id.identify(&input).await.unwrap();
        assert_eq!(m.season, Some(2));
        assert_eq!(m.episodes, vec![5]);
    }
}
