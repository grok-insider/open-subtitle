//! The composition root: build an `Engine` from a `Config` by wiring the
//! concrete adapters the user enabled. This is the only place that names
//! concrete adapter types.

use os_config::Config;
use os_core::{CoreResult, WeightedScorer};
use os_engine::Engine;
use os_identify::{AniListRefiner, FilenameIdentifier};
use os_process::DefaultPostProcessor;
use os_providers::{OpenSubtitlesCom, OpenSubtitlesOrg, SubDl};
use std::sync::Arc;

/// Build the engine from config.
pub fn build_engine(cfg: &Config) -> CoreResult<Engine> {
    let client = os_providers::client(&cfg.net.user_agent, cfg.net.timeout_secs);

    let mut builder = Engine::builder()
        .identifier(Arc::new(FilenameIdentifier::new()))
        .refiner(Arc::new(AniListRefiner::new(client.clone())))
        .scorer(Arc::new(WeightedScorer))
        .post_processor(Arc::new(DefaultPostProcessor))
        .max_concurrency(cfg.net.max_concurrency);

    // Keyless primary.
    if cfg.providers.opensubtitles_org.enabled {
        builder = builder.provider(Arc::new(OpenSubtitlesOrg::new(client.clone())));
    }
    // Key-optional providers (only wired when a key is present).
    if cfg.providers.subdl.enabled {
        if let Some(key) = cfg
            .providers
            .subdl
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
        {
            builder = builder.provider(Arc::new(SubDl::new(client.clone(), key)));
        }
    }
    if cfg.providers.opensubtitles_com.enabled {
        if let Some(key) = cfg
            .providers
            .opensubtitles_com
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
        {
            builder = builder.provider(Arc::new(OpenSubtitlesCom::new(
                client.clone(),
                key,
                cfg.net.user_agent.clone(),
            )));
        }
    }

    builder.build()
}
