//! The single error type returned across all ports.
//!
//! Adapters convert their concrete errors into a [`CoreError`] variant at the
//! boundary. The variant choice matters: `RateLimited`/`DownloadLimit`/
//! `AuthRequired` drive the engine's throttler, while `NotFound` is a soft miss
//! that lets the engine try the next provider.

use std::fmt;

/// Result alias used by every port.
pub type CoreResult<T> = Result<T, CoreError>;

/// The error type all ports return.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CoreError {
    /// Misconfiguration (missing required key, bad value).
    #[error("config error: {0}")]
    Config(String),

    /// Transport-level failure (DNS, TLS, connection, timeout).
    #[error("network error: {0}")]
    Network(String),

    /// The provider asked us to slow down. `retry_after_secs` is the hint, if any.
    #[error("rate limited{}", .retry_after_secs.map(|s| format!("; retry after {s}s")).unwrap_or_default())]
    RateLimited { retry_after_secs: Option<u64> },

    /// A daily/periodic download quota was exhausted.
    #[error("download limit reached{}", .reset.as_ref().map(|r| format!("; resets {r}")).unwrap_or_default())]
    DownloadLimit { reset: Option<String> },

    /// The provider needs credentials we don't have.
    #[error("authentication required: {0}")]
    AuthRequired(String),

    /// No result (soft miss — try the next provider).
    #[error("not found")]
    NotFound,

    /// The provider is in a throttle cooldown window.
    #[error("throttled: {0}")]
    Throttled(String),

    /// Response parsing failed.
    #[error("parse error: {0}")]
    Parse(String),

    /// Asked to do something unsupported (e.g. a provider that can't do movies).
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// Local I/O failure.
    #[error("io error: {0}")]
    Io(String),

    /// Catch-all provider error with a message.
    #[error("provider error: {0}")]
    Provider(String),
}

impl CoreError {
    /// Whether this error should cause the engine to back off this provider.
    pub fn is_throttling(&self) -> bool {
        matches!(
            self,
            CoreError::RateLimited { .. }
                | CoreError::DownloadLimit { .. }
                | CoreError::AuthRequired(_)
                | CoreError::Throttled(_)
        )
    }

    /// Whether this is a soft miss (continue to the next provider, don't throttle).
    pub fn is_soft_miss(&self) -> bool {
        matches!(self, CoreError::NotFound)
    }
}

/// Helper to wrap any `Display` as a `Network` error.
pub fn network<E: fmt::Display>(e: E) -> CoreError {
    CoreError::Network(e.to_string())
}
