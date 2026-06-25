//! Shared HTTP client factory + helpers used by provider adapters.

use os_core::CoreError;
use std::time::Duration;

/// Build a shared `reqwest::Client` with a timeout and a user agent.
pub fn client(user_agent: &str, timeout_secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(user_agent.to_string())
        .timeout(Duration::from_secs(timeout_secs.max(5)))
        .build()
        .unwrap_or_default()
}

/// Map an HTTP status to the right `CoreError` so the throttler reacts correctly.
pub fn status_to_error(status: reqwest::StatusCode, ctx: &str) -> CoreError {
    use reqwest::StatusCode;
    match status {
        StatusCode::NOT_FOUND => CoreError::NotFound,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            CoreError::AuthRequired(format!("{ctx}: {status}"))
        }
        StatusCode::TOO_MANY_REQUESTS => CoreError::RateLimited {
            retry_after_secs: None,
        },
        StatusCode::PAYMENT_REQUIRED => CoreError::DownloadLimit { reset: None },
        s if s.is_server_error() => CoreError::Provider(format!("{ctx}: server error {status}")),
        s => CoreError::Provider(format!("{ctx}: {s}")),
    }
}

/// Convert a transport error into `CoreError::Network`.
pub fn net_err(ctx: &str, e: reqwest::Error) -> CoreError {
    if e.is_timeout() {
        CoreError::Network(format!("{ctx}: timeout"))
    } else {
        CoreError::Network(format!("{ctx}: {e}"))
    }
}
