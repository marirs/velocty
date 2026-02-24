use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::{Header, Status};
use rocket::{Data, Request, Response};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::store::Store;

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
        let store = match request.rocket().state::<std::sync::Arc<dyn Store>>() {
            Some(s) => s,
            None => return,
        };

        // Check if firewall is enabled
        if store.setting_get_or("firewall_enabled", "false") != "true" {
            return;
        }

        let path = request.uri().path().to_string();

        // Skip truly static files (no user input in path)
        if path.starts_with("/static") || path == "/favicon.ico" {
            return;
        }

        let headers = request.headers();
        let socket_ip = request
            .client_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Only trust forwarded headers if the direct connection is from a
        // loopback/private address (i.e. a reverse proxy). This prevents
        // remote clients from spoofing their IP via X-Forwarded-For etc.
        let from_proxy = socket_ip == "127.0.0.1"
            || socket_ip == "::1"
            || socket_ip.starts_with("10.")
            || socket_ip.starts_with("172.")
            || socket_ip.starts_with("192.168.");

        let ip = if from_proxy {
            headers
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
                .unwrap_or_else(|| socket_ip.clone())
        } else {
            socket_ip.clone()
        };

        let ua = request
            .headers()
            .get_one("User-Agent")
            .unwrap_or("")
            .to_string();

        // ── 0. Exempt actual loopback connections (socket-level, not header-spoofed) ──
        let is_loopback = socket_ip == "127.0.0.1" || socket_ip == "::1";
        if is_loopback && ip == socket_ip {
            return;
        }

        // ── 1. Ban check ──
        if store.fw_is_banned(&ip) {
            request.local_cache(|| FwBlock(true));
            return;
        }

        // ── 2. Rate limiting ──
        if store.setting_get_or("fw_rate_limit_enabled", "true") == "true" {
            let max_req: u64 = store
                .setting_get_or("fw_rate_limit_requests", "100")
                .parse()
                .unwrap_or(100);
            let window: u64 = store
                .setting_get_or("fw_rate_limit_window", "60")
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
                let ban_dur = store.setting_get_or("fw_rate_limit_ban_duration", "1h");
                let _ = store.fw_ban_create_with_duration(
                    &ip,
                    "rate_limit",
                    Some("Rate limit exceeded"),
                    &ban_dur,
                    None,
                    Some(&ua),
                );
                store.fw_event_log(
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

        if store.setting_get_or("fw_xss_protection", "true") == "true"
            && inspect::contains_xss(&check_input)
        {
            let ban_dur = store.setting_get_or("fw_injection_ban_duration", "7d");
            let _ = store.fw_ban_create_with_duration(
                &ip,
                "xss",
                Some("XSS attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            store.fw_event_log(&ip, "xss", Some(&check_input), None, Some(&ua), Some(&path));
            request.local_cache(|| FwBlock(true));
            return;
        }

        if store.setting_get_or("fw_sqli_protection", "true") == "true"
            && inspect::contains_sqli(&check_input)
        {
            let ban_dur = store.setting_get_or("fw_injection_ban_duration", "7d");
            let _ = store.fw_ban_create_with_duration(
                &ip,
                "sqli",
                Some("SQL injection attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            store.fw_event_log(
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

        if store.setting_get_or("fw_path_traversal_protection", "true") == "true"
            && inspect::contains_path_traversal(&check_input)
        {
            let ban_dur = store.setting_get_or("fw_injection_ban_duration", "7d");
            let _ = store.fw_ban_create_with_duration(
                &ip,
                "path_traversal",
                Some("Path traversal attempt detected"),
                &ban_dur,
                None,
                Some(&ua),
            );
            store.fw_event_log(
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
        if store.setting_get_or("fw_monitor_bots", "true") == "true"
            && inspect::is_suspicious_bot(&ua)
        {
            store.fw_event_log(
                &ip,
                "suspicious_bot",
                Some(&ua),
                None,
                Some(&ua),
                Some(&path),
            );

            if store.setting_get_or("fw_bot_auto_ban", "false") == "true" {
                let threshold: i64 = store
                    .setting_get_or("fw_bot_ban_threshold", "10")
                    .parse()
                    .unwrap_or(10);
                let count = store.fw_event_count_for_ip_since(&ip, "suspicious_bot", 60);
                if count >= threshold {
                    let ban_dur = store.setting_get_or("fw_bot_ban_duration", "24h");
                    let _ = store.fw_ban_create_with_duration(
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

        // Security headers — always set regardless of firewall state
        let store = match req.rocket().state::<std::sync::Arc<dyn Store>>() {
            Some(s) => s,
            None => return,
        };

        if store.setting_get_or("fw_security_headers", "true") == "true" {
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

            // HSTS: only set when site_url starts with https to avoid breaking HTTP-only dev setups
            let site_url = store.setting_get_or("site_url", "");
            if site_url.starts_with("https://") {
                // max-age=31536000 (1 year); includeSubDomains for full coverage
                res.set_header(Header::new(
                    "Strict-Transport-Security",
                    "max-age=31536000; includeSubDomains",
                ));
            }
        }
    }
}

/// Local cache marker for blocked requests
#[derive(Clone, Copy)]
struct FwBlock(bool);
