//! # os-compose
//!
//! The shared composition root: wire concrete adapters into an [`Engine`] from a
//! [`Config`]. The CLI, daemon, and FFI all call [`build_engine`] so they behave
//! identically. This is the only place (besides the binaries) allowed to name
//! concrete adapter types.

use os_config::Config;
use os_core::{CoreResult, WeightedScorer};
use os_engine::Engine;
use os_identify::{AniListRefiner, FilenameIdentifier};
use os_process::DefaultPostProcessor;
use os_providers::{Jimaku, OpenSubtitlesCom, OpenSubtitlesOrg, SubDl};
use std::sync::Arc;

/// Build the engine from config, wiring every enabled adapter.
pub fn build_engine(cfg: &Config) -> CoreResult<Engine> {
    let client = os_providers::client(&cfg.net.user_agent, cfg.net.timeout_secs);

    let mut builder = Engine::builder()
        .identifier(Arc::new(FilenameIdentifier::new()))
        .refiner(Arc::new(AniListRefiner::new(client.clone())))
        .scorer(Arc::new(WeightedScorer))
        .post_processor(Arc::new(DefaultPostProcessor))
        .max_concurrency(cfg.net.max_concurrency);

    // --- Providers -------------------------------------------------------
    // Keyless primary.
    if cfg.providers.opensubtitles_org.enabled {
        builder = builder.provider(Arc::new(OpenSubtitlesOrg::new(client.clone())));
    }
    // Key-optional providers (only wired when a key is present).
    if cfg.providers.subdl.enabled {
        if let Some(key) = nonempty(&cfg.providers.subdl.api_key) {
            builder = builder.provider(Arc::new(SubDl::new(client.clone(), key)));
        }
    }
    if cfg.providers.opensubtitles_com.enabled {
        if let Some(key) = nonempty(&cfg.providers.opensubtitles_com.api_key) {
            builder = builder.provider(Arc::new(OpenSubtitlesCom::new(
                client.clone(),
                key,
                cfg.net.user_agent.clone(),
            )));
        }
    }
    if cfg.providers.jimaku.enabled {
        if let Some(key) = nonempty(&cfg.providers.jimaku.api_key) {
            builder = builder.provider(Arc::new(Jimaku::new(client.clone(), key)));
        }
    }

    // --- Toolchain (optional; only wired if the tool/endpoint is available) ---
    if let Some(sync) = os_sync::from_backend(&cfg.sync.backend) {
        builder = builder.synchronizer(Arc::from(sync));
    }
    if let Some(tr) = os_translate::from_backend(
        client.clone(),
        &cfg.translate.backend,
        cfg.translate.endpoint.clone(),
        cfg.translate.api_key.clone(),
    ) {
        builder = builder.translator(Arc::from(tr));
    }
    if let Some(tx) =
        os_transcribe::from_backend(&cfg.transcribe.backend, cfg.transcribe.model.clone())
    {
        builder = builder.transcriber(Arc::from(tx));
    }

    builder.build()
}

fn nonempty(o: &Option<String>) -> Option<String> {
    o.clone().filter(|s| !s.is_empty())
}
