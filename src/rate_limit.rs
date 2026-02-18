use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// In-memory rate limiter keyed by (bucket, ip_hash).
/// Each bucket (e.g. "login", "comment") has its own max attempts and window.
pub struct RateLimiter {
    entries: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record an attempt and return true if the attempt is allowed (under the limit).
    /// `key` should be something like "login:<ip_hash>" or "comment:<ip_hash>".
    /// `max_attempts` is the maximum number of attempts allowed within `window`.
    pub fn check_and_record(&self, key: &str, max_attempts: u64, window: Duration) -> bool {
        let mut map = self.entries.lock().unwrap();
        let now = Instant::now();
        let cutoff = now - window;

        let attempts = map.entry(key.to_string()).or_default();

        // Prune old entries outside the window
        attempts.retain(|t| *t > cutoff);

        if (attempts.len() as u64) < max_attempts {
            attempts.push(now);
            true
        } else {
            false
        }
    }

    /// Check remaining attempts without recording a new one.
    pub fn remaining(&self, key: &str, max_attempts: u64, window: Duration) -> u64 {
        let mut map = self.entries.lock().unwrap();
        let now = Instant::now();
        let cutoff = now - window;

        let attempts = map.entry(key.to_string()).or_default();
        attempts.retain(|t| *t > cutoff);

        max_attempts.saturating_sub(attempts.len() as u64)
    }

    /// Periodically clean up stale entries (call from a fairing or timer).
    pub fn cleanup(&self, max_age: Duration) {
        let mut map = self.entries.lock().unwrap();
        let cutoff = Instant::now() - max_age;
        map.retain(|_, attempts| {
            attempts.retain(|t| *t > cutoff);
            !attempts.is_empty()
        });
    }
}
