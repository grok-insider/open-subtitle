//! # os-identify
//!
//! Adapters that turn a file/name/query into a rich `Media`: the filename
//! identifier (using the pure `os_core::guess` parser), the OSDB file hasher,
//! and online refiners (AniList for anime ids/titles).

pub mod filename;
pub mod hash;
pub mod refine;

pub use filename::FilenameIdentifier;
pub use hash::OsdbHasher;
pub use refine::AniListRefiner;
