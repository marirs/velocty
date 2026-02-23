use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Data, Request};
use sha2::{Digest, Sha256};

use crate::store::Store;
use crate::ADMIN_INTERNAL_MOUNT;

/// Middleware that logs page views for every public request.
/// Admin routes are excluded.
pub struct AnalyticsFairing;

#[rocket::async_trait]
impl Fairing for AnalyticsFairing {
    fn info(&self) -> Info {
        Info {
            name: "Analytics Page View Logger",
            kind: Kind::Request,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _data: &mut Data<'_>) {
        let path = request.uri().path().to_string();

        // Skip admin routes (already rewritten to /__adm), static files, and API endpoints
        if path.starts_with(ADMIN_INTERNAL_MOUNT)
            || path.starts_with("/static")
            || path.starts_with("/uploads")
            || path.starts_with("/api")
            || path == "/favicon.ico"
        {
            return;
        }

        let store = match request.rocket().state::<std::sync::Arc<dyn Store>>() {
            Some(s) => s,
            None => return,
        };

        let ip = request
            .client_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let ip_hash = hash_ip(&ip);

        let referrer = request.headers().get_one("Referer").map(extract_domain);

        let ua_string = request.headers().get_one("User-Agent").unwrap_or("");
        let (device_type, browser) = parse_user_agent(ua_string);

        // GeoIP lookup would happen here with maxminddb
        // For now, country/city are None until GeoLite2 DB is configured
        let country: Option<&str> = None;
        let city: Option<&str> = None;

        let _ = store.analytics_record(
            &path,
            &ip_hash,
            country,
            city,
            referrer.as_deref(),
            Some(ua_string),
            Some(device_type),
            Some(browser),
        );
    }
}

fn hash_ip(ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    hex::encode(hasher.finalize())
}

fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| url.to_string())
}

fn parse_user_agent(ua: &str) -> (&str, &str) {
    let device = if ua.contains("Mobile") || ua.contains("Android") {
        "mobile"
    } else if ua.contains("Tablet") || ua.contains("iPad") {
        "tablet"
    } else {
        "desktop"
    };

    let browser = if ua.contains("Firefox") {
        "Firefox"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome") {
        "Chrome"
    } else if ua.contains("Safari") {
        "Safari"
    } else if ua.contains("Opera") || ua.contains("OPR") {
        "Opera"
    } else {
        "Other"
    };

    (device, browser)
}
