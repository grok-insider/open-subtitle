//! # os-engine
//!
//! The application layer: the `Engine` that composes ports and implements the
//! use-cases (`identify`, `search`, `download_best`, `fallback`), guarded by the
//! [`Throttler`]. Depends only on `os-core`.

pub mod throttle;

pub use throttle::Throttler;

use futures::stream::{self, StreamExt};
use os_core::ports::{
    Identifier, MediaInput, PostProcessor, ProcessOpts, Provider, Refiner, Scorer,
};
use os_core::{
    passes_series_safety, CoreError, CoreResult, Language, Media, Query, SubtitleCandidate,
    SubtitleFile,
};
use std::sync::Arc;

/// The composed engine. Cheap to clone-share via `Arc` fields.
#[derive(Clone)]
pub struct Engine {
    identifier: Arc<dyn Identifier>,
    refiners: Vec<Arc<dyn Refiner>>,
    providers: Vec<Arc<dyn Provider>>,
    scorer: Arc<dyn Scorer>,
    post: Arc<dyn PostProcessor>,
    throttler: Arc<Throttler>,
    max_concurrency: usize,
    min_score: i32,
}

impl Engine {
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Access the throttler (e.g. for a `providers` status command).
    pub fn throttler(&self) -> &Throttler {
        &self.throttler
    }

    /// Names of the wired providers.
    pub fn provider_names(&self) -> Vec<String> {
        self.providers
            .iter()
            .map(|p| p.name().to_string())
            .collect()
    }

    /// Identify media from an input, then enrich it with refiners (best-effort).
    pub async fn identify(&self, input: &MediaInput) -> CoreResult<Media> {
        let mut media = self.identifier.identify(input).await?;
        for r in &self.refiners {
            if let Err(e) = r.refine(&mut media).await {
                tracing::debug!(refiner = r.name(), error = %e, "refiner failed (ignored)");
            }
        }
        Ok(media)
    }

    /// Search all eligible providers in parallel, score, and sort (desc).
    pub async fn search(
        &self,
        media: &Media,
        languages: &[Language],
    ) -> CoreResult<Vec<SubtitleCandidate>> {
        let query = Query {
            media: media.clone(),
            languages: languages.to_vec(),
        };

        // Eligible providers: handle this kind, have any needed id, not throttled.
        let eligible: Vec<Arc<dyn Provider>> = self
            .providers
            .iter()
            .filter(|p| {
                let cap = p.capabilities();
                cap.handles(media.kind)
                    && (!cap.needs_hash || media.hashes.contains_key("osdb"))
                    && (!cap.needs_imdb || media.ids.imdb.is_some())
                    && !self.throttler.is_throttled(p.name())
            })
            .cloned()
            .collect();

        let throttler = self.throttler.clone();
        let results: Vec<Vec<SubtitleCandidate>> = stream::iter(eligible)
            .map(|p| {
                let q = query.clone();
                let throttler = throttler.clone();
                async move {
                    match p.list(&q).await {
                        Ok(cands) => {
                            throttler.record_success(p.name());
                            cands
                        }
                        Err(e) => {
                            if !e.is_soft_miss() {
                                tracing::debug!(provider = p.name(), error = %e, "list failed");
                            }
                            throttler.record_error(p.name(), &e);
                            Vec::new()
                        }
                    }
                }
            })
            .buffer_unordered(self.max_concurrency.max(1))
            .collect()
            .await;

        let mut all: Vec<SubtitleCandidate> = results.into_iter().flatten().collect();

        // Score every candidate.
        for c in &mut all {
            let s = self.scorer.score(c, media);
            c.score = s.score;
            c.score_without_hash = s.without_hash;
        }
        // Sort by score desc, tie-break on score_without_hash desc.
        all.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(b.score_without_hash.cmp(&a.score_without_hash))
        });
        Ok(all)
    }

    /// Download the best subtitle per requested language (in preference order),
    /// post-processed. Falls back to the next candidate on fetch/parse failure.
    pub async fn download_best(
        &self,
        media: &Media,
        languages: &[Language],
        opts: &ProcessOpts,
    ) -> CoreResult<Vec<SubtitleFile>> {
        let candidates = self.search(media, languages).await?;
        let mut out = Vec::new();

        for lang in languages {
            // Candidates for this language, already sorted best-first.
            let mut pool: Vec<&SubtitleCandidate> = candidates
                .iter()
                .filter(|c| c.language.same_language(lang))
                .collect();

            // Prefer candidates that clear the safety gate and the min score;
            // otherwise fall back to the highest-scored (provider already filtered
            // by season/episode/query, so this is a sane best-effort).
            let gated: Vec<&SubtitleCandidate> = pool
                .iter()
                .filter(|c| c.score >= self.min_score && passes_series_safety(c, media))
                .copied()
                .collect();
            if !gated.is_empty() {
                pool = gated;
            }

            let mut delivered = false;
            for c in pool {
                let provider = match self
                    .providers
                    .iter()
                    .find(|p| p.name() == c.provider.as_str())
                {
                    Some(p) => p,
                    None => continue,
                };
                match provider.fetch(c).await {
                    Ok(raw) => {
                        let mut o = opts.clone();
                        o.language = Some(lang.clone());
                        match self.post.process(raw, &o) {
                            Ok(file) => {
                                self.throttler.record_success(provider.name());
                                out.push(file);
                                delivered = true;
                                break;
                            }
                            Err(e) => {
                                tracing::debug!(provider = provider.name(), error = %e, "process failed");
                            }
                        }
                    }
                    Err(e) => {
                        self.throttler.record_error(provider.name(), &e);
                        tracing::debug!(provider = provider.name(), error = %e, "fetch failed");
                    }
                }
            }
            if !delivered {
                tracing::debug!(language = %lang.display_tag(), "no subtitle delivered");
            }
        }

        if out.is_empty() {
            Err(CoreError::NotFound)
        } else {
            Ok(out)
        }
    }

    /// Identify + download in one call.
    pub async fn auto(
        &self,
        input: &MediaInput,
        languages: &[Language],
        opts: &ProcessOpts,
    ) -> CoreResult<(Media, Vec<SubtitleFile>)> {
        let media = self.identify(input).await?;
        let files = self.download_best(&media, languages, opts).await?;
        Ok((media, files))
    }
}

/// Builder for [`Engine`]. Unset optional ports simply disable their features.
#[derive(Default)]
pub struct EngineBuilder {
    identifier: Option<Arc<dyn Identifier>>,
    refiners: Vec<Arc<dyn Refiner>>,
    providers: Vec<Arc<dyn Provider>>,
    scorer: Option<Arc<dyn Scorer>>,
    post: Option<Arc<dyn PostProcessor>>,
    max_concurrency: Option<usize>,
    min_score: Option<i32>,
}

impl EngineBuilder {
    pub fn identifier(mut self, id: Arc<dyn Identifier>) -> Self {
        self.identifier = Some(id);
        self
    }
    pub fn refiner(mut self, r: Arc<dyn Refiner>) -> Self {
        self.refiners.push(r);
        self
    }
    pub fn provider(mut self, p: Arc<dyn Provider>) -> Self {
        self.providers.push(p);
        self
    }
    pub fn scorer(mut self, s: Arc<dyn Scorer>) -> Self {
        self.scorer = Some(s);
        self
    }
    pub fn post_processor(mut self, p: Arc<dyn PostProcessor>) -> Self {
        self.post = Some(p);
        self
    }
    pub fn max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = Some(n);
        self
    }
    pub fn min_score(mut self, n: i32) -> Self {
        self.min_score = Some(n);
        self
    }

    /// Build the engine. Requires an identifier, a scorer, and a post-processor.
    pub fn build(self) -> CoreResult<Engine> {
        Ok(Engine {
            identifier: self
                .identifier
                .ok_or_else(|| CoreError::Config("engine: identifier required".into()))?,
            refiners: self.refiners,
            providers: self.providers,
            scorer: self
                .scorer
                .ok_or_else(|| CoreError::Config("engine: scorer required".into()))?,
            post: self
                .post
                .ok_or_else(|| CoreError::Config("engine: post-processor required".into()))?,
            throttler: Arc::new(Throttler::new()),
            max_concurrency: self.max_concurrency.unwrap_or(8),
            min_score: self.min_score.unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use os_core::ports::Capabilities;
    use os_core::{Container, RawSubtitle, WeightedScorer};

    struct FakeId;
    #[async_trait]
    impl Identifier for FakeId {
        async fn identify(&self, _input: &MediaInput) -> CoreResult<Media> {
            Ok(Media::episode("The Show", 1, 2))
        }
    }

    struct FakePost;
    impl PostProcessor for FakePost {
        fn process(&self, raw: RawSubtitle, _opts: &ProcessOpts) -> CoreResult<SubtitleFile> {
            Ok(SubtitleFile {
                language: raw.language,
                format: "srt".into(),
                text: String::from_utf8_lossy(&raw.bytes).into_owned(),
                provider: raw.provider,
                release: raw.release,
                hi: false,
                forced: false,
            })
        }
    }

    struct FakeProvider {
        name: String,
        good: bool,
    }
    #[async_trait]
    impl Provider for FakeProvider {
        fn name(&self) -> &str {
            &self.name
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        async fn list(&self, q: &Query) -> CoreResult<Vec<SubtitleCandidate>> {
            if !self.good {
                return Err(CoreError::Network("boom".into()));
            }
            let mut c =
                SubtitleCandidate::new(&self.name, "1", q.languages.first().cloned().unwrap());
            c.release = Some("The.Show.S01E02.1080p.WEB-DL.x264-GRP".into());
            Ok(vec![c])
        }
        async fn fetch(&self, c: &SubtitleCandidate) -> CoreResult<RawSubtitle> {
            Ok(RawSubtitle {
                filename: "x.srt".into(),
                bytes: b"1\n00:00:01,000 --> 00:00:02,000\nHi\n".to_vec(),
                container: Container::Plain,
                language: c.language.clone(),
                provider: self.name.clone(),
                release: c.release.clone(),
                hi: false,
                forced: false,
            })
        }
    }

    fn engine(providers: Vec<Arc<dyn Provider>>) -> Engine {
        let mut b = Engine::builder()
            .identifier(Arc::new(FakeId))
            .scorer(Arc::new(WeightedScorer))
            .post_processor(Arc::new(FakePost));
        for p in providers {
            b = b.provider(p);
        }
        b.build().unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_scores_and_sorts() {
        let e = engine(vec![Arc::new(FakeProvider {
            name: "good".into(),
            good: true,
        })]);
        let media = Media::episode("The Show", 1, 2);
        let en = Language::parse("en").unwrap();
        let res = e.search(&media, &[en]).await.unwrap();
        assert_eq!(res.len(), 1);
        assert!(res[0].score > 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failing_provider_is_throttled_not_fatal() {
        let e = engine(vec![
            Arc::new(FakeProvider {
                name: "bad".into(),
                good: false,
            }),
            Arc::new(FakeProvider {
                name: "good".into(),
                good: true,
            }),
        ]);
        let media = Media::episode("The Show", 1, 2);
        let en = Language::parse("en").unwrap();
        let res = e.search(&media, &[en]).await.unwrap();
        // The good provider still returns a result.
        assert_eq!(res.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn download_best_delivers_processed_file() {
        let e = engine(vec![Arc::new(FakeProvider {
            name: "good".into(),
            good: true,
        })]);
        let media = Media::episode("The Show", 1, 2);
        let en = Language::parse("en").unwrap();
        let files = e
            .download_best(&media, &[en], &ProcessOpts::default())
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].text.contains("Hi"));
    }
}
