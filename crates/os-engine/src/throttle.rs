//! Per-provider anti-ban throttling (Bazarr's model): a hard `exception →
//! cooldown` map plus a soft "5 strikes in 120 s" counter for transient errors.
//! In-memory; the engine consults it before calling a provider.

use os_core::CoreError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SOFT_WINDOW: Duration = Duration::from_secs(120);
const SOFT_LIMIT: usize = 5;
const SOFT_COOLDOWN: Duration = Duration::from_secs(20 * 60);

enum Cooldown {
    /// Throttle immediately for this duration.
    Hard(Duration),
    /// Count toward the soft window; throttle after the limit.
    Soft,
    /// Don't throttle (soft miss / unsupported).
    None,
}

fn classify(err: &CoreError) -> Cooldown {
    match err {
        CoreError::RateLimited { retry_after_secs } => {
            Cooldown::Hard(Duration::from_secs(retry_after_secs.unwrap_or(3600)))
        }
        CoreError::DownloadLimit { .. } => Cooldown::Hard(Duration::from_secs(3 * 3600)),
        CoreError::AuthRequired(_) => Cooldown::Hard(Duration::from_secs(12 * 3600)),
        CoreError::Throttled(_) => Cooldown::Hard(Duration::from_secs(10 * 60)),
        CoreError::Network(_) | CoreError::Provider(_) | CoreError::Parse(_) => Cooldown::Soft,
        CoreError::NotFound
        | CoreError::Unsupported(_)
        | CoreError::Config(_)
        | CoreError::Io(_) => Cooldown::None,
    }
}

#[derive(Default)]
struct Inner {
    until: HashMap<String, Instant>,
    soft: HashMap<String, Vec<Instant>>,
}

/// Tracks which providers are in a cooldown window.
#[derive(Default)]
pub struct Throttler {
    inner: Mutex<Inner>,
}

impl Throttler {
    pub fn new() -> Self {
        Throttler::default()
    }

    /// Remaining cooldown for a provider, if currently throttled.
    pub fn throttled_for(&self, provider: &str) -> Option<Duration> {
        let now = Instant::now();
        let inner = self.inner.lock().unwrap();
        inner
            .until
            .get(provider)
            .filter(|&&t| t > now)
            .map(|&t| t - now)
    }

    /// Whether a provider is currently throttled.
    pub fn is_throttled(&self, provider: &str) -> bool {
        self.throttled_for(provider).is_some()
    }

    /// Record a successful call (clears the soft counter).
    pub fn record_success(&self, provider: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.soft.remove(provider);
    }

    /// Record a failed call; applies a cooldown per the classification.
    pub fn record_error(&self, provider: &str, err: &CoreError) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        match classify(err) {
            Cooldown::Hard(d) => {
                inner.until.insert(provider.to_string(), now + d);
            }
            Cooldown::Soft => {
                let should_throttle = {
                    let hits = inner.soft.entry(provider.to_string()).or_default();
                    hits.push(now);
                    hits.retain(|&t| now.duration_since(t) <= SOFT_WINDOW);
                    hits.len() >= SOFT_LIMIT
                };
                if should_throttle {
                    inner
                        .until
                        .insert(provider.to_string(), now + SOFT_COOLDOWN);
                    inner.soft.remove(provider);
                }
            }
            Cooldown::None => {}
        }
    }

    /// Snapshot of throttled providers and remaining cooldown seconds.
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        let now = Instant::now();
        let inner = self.inner.lock().unwrap();
        inner
            .until
            .iter()
            .filter(|(_, &t)| t > now)
            .map(|(k, &t)| (k.clone(), (t - now).as_secs()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_error_throttles_immediately() {
        let t = Throttler::new();
        t.record_error(
            "p",
            &CoreError::RateLimited {
                retry_after_secs: Some(60),
            },
        );
        assert!(t.is_throttled("p"));
        assert!(t.throttled_for("p").unwrap().as_secs() > 50);
    }

    #[test]
    fn soft_errors_need_five_strikes() {
        let t = Throttler::new();
        for _ in 0..4 {
            t.record_error("p", &CoreError::Network("x".into()));
        }
        assert!(!t.is_throttled("p"));
        t.record_error("p", &CoreError::Network("x".into()));
        assert!(t.is_throttled("p"));
    }

    #[test]
    fn not_found_does_not_throttle() {
        let t = Throttler::new();
        for _ in 0..10 {
            t.record_error("p", &CoreError::NotFound);
        }
        assert!(!t.is_throttled("p"));
    }

    #[test]
    fn success_clears_soft_counter() {
        let t = Throttler::new();
        for _ in 0..4 {
            t.record_error("p", &CoreError::Network("x".into()));
        }
        t.record_success("p");
        t.record_error("p", &CoreError::Network("x".into()));
        assert!(!t.is_throttled("p"));
    }
}
