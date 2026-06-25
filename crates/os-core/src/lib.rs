//! # os-core
//!
//! The pure heart of open-subtitle: the domain model, the port traits, the error
//! type, and the matching/scoring algorithm. **No I/O, no heavy deps** — this
//! crate compiles to `wasm32` and is the most-tested in the workspace.
//!
//! Adapters in sibling crates implement the [`ports`]; `os-engine` orchestrates
//! them depending only on this surface.

pub mod error;
pub mod guess;
pub mod lang;
pub mod model;
pub mod ports;
pub mod score;

pub use error::{network, CoreError, CoreResult};
pub use guess::{guess, Guess};
pub use lang::Language;
pub use model::{
    Container, Hashes, IdSet, Media, MediaKind, Query, RawSubtitle, ReleaseInfo, SubtitleCandidate,
    SubtitleFile,
};
pub use ports::{
    Capabilities, Hasher, Identifier, MediaInput, PostProcessor, ProcessOpts, Provider, Refiner,
    Scorer, Synchronizer, Transcriber, Translator,
};
pub use score::{compute_score, passes_series_safety, Match, Score, WeightedScorer};
