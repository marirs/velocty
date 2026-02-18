use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::{Header, Status};
use rocket::{Data, Request, Response};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::db::DbPool;
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::settings::Setting;

use super::inspect;

static FW_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct FwRateLimiter {
    buckets: Mutex<HashMap<String, (u64, Instant)>>,
}

impl FwRateLimiter {
    pub fn new() -> Self {
        FwRateLimiter {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Returns true if the IP has exceeded the rate limit
    fn check(&self, ip: &str, max_requests: u64, window_secs: u64) -> bool {
        let mut map = match self.buckets.lock() {
            Ok(m) => m,
            Err(_) => return false,
        };
        let now = Instant::now();
        let entry = map.entry(ip.to_string()).or_insert((0, now));

        if now.duration_since(entry.1).as_secs() >= window_secs {
            entry.0 = 1;
            entry.1 = now;
            false
        } else {
            entry.0 += 1;
            entry.0 > max_requests
        }
    }

    /// Periodic cleanup of stale entries (called occasionally)
    fn cleanup(&self) {
        let mut map = match self.buckets.lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        let now = Instant::now();
        map.retain(|_, (_, ts)| now.duration_since(*ts).as_secs() < 600);
    }
}

// ── Firewall Fairing ─────────────────────────────────────────

pub struct FirewallFairing;

#[rocket::async_trait]
impl Fairing for FirewallFairing {
    fn info(&self) -> Info {
        Info {
            name: "Firewall",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _data: &mut Data<'_>) {
        let pool = match request.rocket().state::<DbPool>() {
            Some(p) => p,
            None => return,
        };

        // Check if firewall is enabled
        if Setting::get_or(pool, "firewall_enabled", "false") != "true" {
            return;
        }

        let path = request.uri().path().to_string();

        // Skip static files
        if path.starts_with("/static") || path.starts_with("/uploads") || path == "/favicon.ico" {
            return;
        }

        let headers = request.headers();
        let ip = headers
            .get_one("CF-Connecting-IP")
            .or_else(|| headers.get_one("True-Client-IP"))
            .or_else(|| headers.get_one("X-Real-IP"))
            .map(|h| h.trim().to_string())
            .or_else(|| {
                headers
                    .get_one("X-Forwarded-For")
                    .and_then(|h| h.split(',').next())
                    .map(|h| h.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| {
                request
                    .client_ip()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            });

        let ua = request
            .headers()
            .get_one("User-Agent")
            .unwrap_or("")
            .to_string();

        // ── 1. Ban check ──
        if FwBan::is_banned(pool, &ip) {
            request.local_cache(|| FwBlock(true));
            return;
        }

        // ── 2. Rate limiting ──
        if Setting::get_or(pool, "fw_rate_limit_enabled", "true") == "true" {
            let max_req: u64 = Setting::get_or(pool, "fw_rate_limit_requests", "100")
                .parse()
                .unwrap_or(100);
            let window: u64 = Setting::get_or(pool, "fw_rate_limit_window", "60")
                .parse()
                .unwrap_or(60);

            let limiter = request.rocket().state::<FwRateLimiter>().unwrap();

            // Occasional cleanup (every ~200 requests)
            if FW_REQUEST_COUNTER
                .fetch_add(1, Ordering::Relaxed)
                .is_multiple_of(200)
            {
                limiter.cleanup();
            }

            if limiter.check(&ip, max_req, window) {
                let ban_dur = Setting::get_or(pool, "fw_rate_limit_ban_duration", "1h");
                let _ = FwBan::create_with_duration(
                    pool,
                    &ip,
                    "rate_limit",
                    Some("Rate limit exceeded"),
                    &ban_dur,
                    None,
                    Some(&ua),
                );
                FwEvent::log(
                    pool,
                    &ip,
                    "rate_limit",
                    Some("Rate limit exceeded"),
                    None,
                    Some(&ua),
                    Some(&path),
                );
                request.local_cache(|| FwBlock(true));
                return;
            }
        }

        // ── 3. Injection detection ──
        let query = request.uri().query().map(|q| q.as_str()).unwrap_or("");
        let check_input = format!("{} {}", path, query);

        if Setting::get_or(pool, "fw_xss_protection", "true") == "true"
            && inspect::contains_xss(&check_input)
        {
            let ban_dur = Setting::get_or(pool, "fw_injection_ban_duration", "7d");
            let _ = FwBan::create_with_duration(
                pool,
                &ip,
                "xss",
                Some("XSS attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            FwEvent::log(
                pool,
                &ip,
                "xss",
                Some(&check_input),
                None,
                Some(&ua),
                Some(&path),
            );
            request.local_cache(|| FwBlock(true));
            return;
        }

        if Setting::get_or(pool, "fw_sqli_protection", "true") == "true"
            && inspect::contains_sqli(&check_input)
        {
            let ban_dur = Setting::get_or(pool, "fw_injection_ban_duration", "7d");
            let _ = FwBan::create_with_duration(
                pool,
                &ip,
                "sqli",
                Some("SQL injection attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            FwEvent::log(
                pool,
                &ip,
                "sqli",
                Some(&check_input),
                None,
                Some(&ua),
                Some(&path),
            );
            request.local_cache(|| FwBlock(true));
            return;
        }

        if Setting::get_or(pool, "fw_path_traversal_protection", "true") == "true"
            && inspect::contains_path_traversal(&check_input)
        {
            let ban_dur = Setting::get_or(pool, "fw_injection_ban_duration", "7d");
            let _ = FwBan::create_with_duration(
                pool,
                &ip,
                "path_traversal",
                Some("Path traversal attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            FwEvent::log(
                pool,
                &ip,
                "path_traversal",
                Some(&check_input),
                None,
                Some(&ua),
                Some(&path),
            );
            request.local_cache(|| FwBlock(true));
            return;
        }

        // ── 4. Bot detection ──
        if Setting::get_or(pool, "fw_monitor_bots", "true") == "true"
            && inspect::is_suspicious_bot(&ua)
        {
            FwEvent::log(
                pool,
                &ip,
                "suspicious_bot",
                Some(&ua),
                None,
                Some(&ua),
                Some(&path),
            );

            if Setting::get_or(pool, "fw_bot_auto_ban", "false") == "true" {
                let threshold: i64 = Setting::get_or(pool, "fw_bot_ban_threshold", "10")
                    .parse()
                    .unwrap_or(10);
                let count = FwEvent::count_for_ip_since(pool, &ip, "suspicious_bot", 60);
                if count >= threshold {
                    let ban_dur = Setting::get_or(pool, "fw_bot_ban_duration", "24h");
                    let _ = FwBan::create_with_duration(
                        pool,
                        &ip,
                        "bot",
                        Some("Suspicious bot threshold exceeded"),
                        &ban_dur,
                        None,
                        Some(&ua),
                    );
                    request.local_cache(|| FwBlock(true));
                    return;
                }
            }
        }
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        // Check if this request was blocked
        let blocked = req.local_cache(|| FwBlock(false));
        if blocked.0 {
            res.set_status(Status::Forbidden);
            res.set_sized_body(None, std::io::Cursor::new("403 Forbidden"));
            return;
        }

        // Security headers
        let pool = match req.rocket().state::<DbPool>() {
            Some(p) => p,
            None => return,
        };

        if Setting::get_or(pool, "firewall_enabled", "false") != "true" {
            return;
        }

        if Setting::get_or(pool, "fw_security_headers", "true") == "true" {
            res.set_header(Header::new("X-Frame-Options", "SAMEORIGIN"));
            res.set_header(Header::new("X-Content-Type-Options", "nosniff"));
            res.set_header(Header::new(
                "Referrer-Policy",
                "strict-origin-when-cross-origin",
            ));
            res.set_header(Header::new(
                "Permissions-Policy",
                "camera=(), microphone=(), geolocation=()",
            ));
            res.set_header(Header::new("X-XSS-Protection", "1; mode=block"));
        }
    }
}

/// Local cache marker for blocked requests
#[derive(Clone, Copy)]
struct FwBlock(bool);
