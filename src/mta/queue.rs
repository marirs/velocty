/// A queued email message for retry delivery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueuedEmail {
    pub id: i64,
    pub to_addr: String,
    pub from_addr: String,
    pub subject: String,
    pub body_text: String,
    pub attempts: i64,
    pub max_attempts: i64,
    pub next_retry_at: String,
    pub status: String, // "pending", "sending", "sent", "failed"
    pub error: String,
    pub created_at: String,
}

/// Retry schedule: delays in seconds after each failed attempt.
/// Attempt 1: immediate, 2: 60s, 3: 300s, 4: 1800s, 5: 7200s
const RETRY_DELAYS: [u64; 5] = [0, 60, 300, 1800, 7200];

/// Get the delay in seconds for the next retry after `attempts` failures.
pub fn retry_delay(attempts: i64) -> Option<u64> {
    let idx = attempts as usize;
    if idx >= RETRY_DELAYS.len() {
        None // give up
    } else {
        Some(RETRY_DELAYS[idx])
    }
}

/// Calculate the next retry timestamp given current attempts.
pub fn next_retry_timestamp(attempts: i64) -> Option<String> {
    retry_delay(attempts).map(|delay| {
        let now = chrono::Utc::now();
        let next = now + chrono::Duration::seconds(delay as i64);
        next.format("%Y-%m-%d %H:%M:%S").to_string()
    })
}

/// Maximum emails per hour (rate limit).
pub const DEFAULT_MAX_EMAILS_PER_HOUR: u64 = 30;
