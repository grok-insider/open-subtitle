//! # os-providers
//!
//! Subtitle source adapters. Each implements `os_core::Provider`. v1 set:
//! - [`OpenSubtitlesOrg`] — keyless, the default primary.
//! - [`SubDl`] — key-optional (free key), generous anon downloads.
//! - [`OpenSubtitlesCom`] — key/login optional, default-disabled.

pub mod http;
pub mod jimaku;
pub mod opensubtitles_com;
pub mod opensubtitles_org;
pub mod subdl;

pub use jimaku::Jimaku;
pub use opensubtitles_com::OpenSubtitlesCom;
pub use opensubtitles_org::OpenSubtitlesOrg;
pub use subdl::SubDl;

pub use http::client;
