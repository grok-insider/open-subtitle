//! Refiners enrich a `Media` before search. The AniList refiner is keyless
//! (public GraphQL) and adds anime ids (anilist/mal) + canonical titles, which
//! make anime providers (Jimaku/AnimeTosho) far more accurate.

use async_trait::async_trait;
use os_core::ports::Refiner;
use os_core::{CoreError, CoreResult, Media, MediaKind};
use serde_json::json;

const ANILIST_API: &str = "https://graphql.anilist.co";

/// Enriches anime media with AniList/MAL ids and the canonical romaji title.
pub struct AniListRefiner {
    client: reqwest::Client,
}

impl AniListRefiner {
    pub fn new(client: reqwest::Client) -> Self {
        AniListRefiner { client }
    }
}

#[async_trait]
impl Refiner for AniListRefiner {
    fn name(&self) -> &str {
        "anilist"
    }

    async fn refine(&self, media: &mut Media) -> CoreResult<()> {
        // Only meaningful for anime, and only when we don't already have an id.
        if media.kind != MediaKind::Anime || media.ids.anilist.is_some() {
            return Ok(());
        }
        if media.title.is_empty() {
            return Ok(());
        }

        let query = r#"
            query ($search: String) {
              Media(search: $search, type: ANIME) {
                id
                idMal
                title { romaji english }
              }
            }"#;
        let body = json!({ "query": query, "variables": { "search": media.title } });

        let resp = self
            .client
            .post(ANILIST_API)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            // Best-effort: a refiner failure must never abort identification.
            return Ok(());
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Parse(e.to_string()))?;
        let node = &v["data"]["Media"];
        if node.is_null() {
            return Ok(());
        }

        if let Some(id) = node["id"].as_u64() {
            media.ids.anilist = Some(id);
        }
        if let Some(mal) = node["idMal"].as_u64() {
            media.ids.mal = Some(mal);
        }
        if let Some(romaji) = node["title"]["romaji"].as_str() {
            if !romaji.is_empty() && !media.alternative_titles.iter().any(|t| t == romaji) {
                media.alternative_titles.push(romaji.to_string());
            }
        }
        if let Some(eng) = node["title"]["english"].as_str() {
            if !eng.is_empty() && !media.alternative_titles.iter().any(|t| t == eng) {
                media.alternative_titles.push(eng.to_string());
            }
        }

        Ok(())
    }
}
