use chrono::{Duration, Utc};
use rocket::http::{Cookie, CookieJar, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::models::user::User;
use crate::store::Store;

const SESSION_COOKIE: &str = "velocty_session";

// ── Client IP request guard ──

/// Extracts the real client IP from the request.
/// Checks headers in priority order:
///   1. CF-Connecting-IP (Cloudflare)
///   2. True-Client-IP (Cloudflare Enterprise / Akamai)
///   3. X-Real-IP (nginx proxy_set_header)
///   4. X-Forwarded-For (first IP in the chain = original client)
///   5. Rocket's client_ip() (socket peer address)
pub struct ClientIp(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientIp {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let headers = request.headers();

        // Cloudflare
        if let Some(ip) = headers.get_one("CF-Connecting-IP") {
            let ip = ip.trim();
            if !ip.is_empty() {
                return Outcome::Success(ClientIp(ip.to_string()));
            }
        }

        // Cloudflare Enterprise / Akamai
        if let Some(ip) = headers.get_one("True-Client-IP") {
            let ip = ip.trim();
            if !ip.is_empty() {
                return Outcome::Success(ClientIp(ip.to_string()));
            }
        }

        // nginx X-Real-IP
        if let Some(ip) = headers.get_one("X-Real-IP") {
            let ip = ip.trim();
            if !ip.is_empty() {
                return Outcome::Success(ClientIp(ip.to_string()));
            }
        }

        // X-Forwarded-For: client, proxy1, proxy2 — take the first (leftmost)
        if let Some(forwarded) = headers.get_one("X-Forwarded-For") {
            if let Some(ip) = forwarded.split(',').next() {
                let ip = ip.trim();
                if !ip.is_empty() {
                    return Outcome::Success(ClientIp(ip.to_string()));
                }
            }
        }

        // Fallback to Rocket's socket peer address
        let ip = request
            .client_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Outcome::Success(ClientIp(ip))
    }
}

// ── Authenticated user guard (any active user with a valid session) ──

/// Guard: any authenticated user with an active account.
/// All role-specific guards deref to this.
pub struct AuthenticatedUser {
    pub user: User,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match resolve_session_user(request).await {
            Some(user) => Outcome::Success(AuthenticatedUser { user }),
            None => Outcome::Forward(Status::Unauthorized),
        }
    }
}

// ── Role-specific guards ──

/// Guard: requires role = admin
pub struct AdminUser {
    pub user: User,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match resolve_session_user(request).await {
            Some(user) if user.is_admin() => Outcome::Success(AdminUser { user }),
            Some(_) => Outcome::Forward(Status::Forbidden),
            None => Outcome::Forward(Status::Unauthorized),
        }
    }
}

/// Guard: requires role = admin or editor
pub struct EditorUser {
    pub user: User,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for EditorUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match resolve_session_user(request).await {
            Some(user) if user.is_editor_or_above() => Outcome::Success(EditorUser { user }),
            Some(_) => Outcome::Forward(Status::Forbidden),
            None => Outcome::Forward(Status::Unauthorized),
        }
    }
}

/// Guard: requires role = admin, editor, or author
pub struct AuthorUser {
    pub user: User,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthorUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match resolve_session_user(request).await {
            Some(user) if user.is_author_or_above() => Outcome::Success(AuthorUser { user }),
            Some(_) => Outcome::Forward(Status::Forbidden),
            None => Outcome::Forward(Status::Unauthorized),
        }
    }
}

// ── Shared session resolution ──

async fn resolve_session_user(request: &Request<'_>) -> Option<User> {
    let store = request
        .guard::<&State<Arc<dyn Store>>>()
        .await
        .succeeded()?;
    let cookies = request.cookies();
    let session_id = cookies.get_private(SESSION_COOKIE)?.value().to_string();

    match store.session_get_user(&session_id) {
        Some(user) if user.is_active() => Some(user),
        _ => {
            cookies.remove_private(Cookie::from(SESSION_COOKIE));
            None
        }
    }
}

// ── Password utilities ──

pub fn hash_password(password: &str) -> Result<String, String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|e| e.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

// ── Session management ──

pub fn create_session(
    store: &dyn Store,
    user_id: i64,
    ip: Option<&str>,
    ua: Option<&str>,
) -> Result<String, String> {
    let expiry_hours = store.setting_get_i64("session_expiry_hours").max(1);
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    let expires = now + Duration::hours(expiry_hours);
    let expires_str = expires.format("%Y-%m-%d %H:%M:%S").to_string();

    store.session_create_full(user_id, &session_id, &expires_str, ip, ua)?;

    Ok(session_id)
}

/// Validate a session and return the associated user
pub fn get_session_user(store: &dyn Store, session_id: &str) -> Option<User> {
    store.session_get_user(session_id)
}

/// Legacy: validate session exists (for backward compat during migration)
pub fn validate_session(store: &dyn Store, session_id: &str) -> bool {
    store.session_validate(session_id)
}

pub fn destroy_session(store: &dyn Store, session_id: &str) -> Result<(), String> {
    store.session_delete(session_id)
}

/// Set the session cookie with proper security flags.
///
/// The `Secure` flag is derived from the site_url setting AND the
/// site_environment setting — production sites always get Secure=true
/// when configured with an HTTPS URL, preventing misconfiguration from
/// leaking cookies over plaintext.
pub fn set_session_cookie_secure(cookies: &CookieJar<'_>, session_id: &str, store: &dyn Store) {
    let site_url = store.setting_get_or("site_url", "");
    let env = store.setting_get_or("site_environment", "staging");
    let is_secure = site_url.starts_with("https://") || env == "production";

    let mut cookie = Cookie::new(SESSION_COOKIE, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(rocket::http::SameSite::Strict);
    cookie.set_path("/");
    if is_secure {
        cookie.set_secure(true);
    }
    cookies.add_private(cookie);
}

pub fn clear_session_cookie(cookies: &CookieJar<'_>) {
    cookies.remove_private(Cookie::from(SESSION_COOKIE));
}

pub fn hash_ip(ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn cleanup_expired_sessions(store: &dyn Store) {
    store.session_cleanup_expired();
}

pub fn check_login_rate_limit(store: &dyn Store, ip: &str) -> bool {
    let max_attempts = store.setting_get_i64("login_rate_limit").max(1);
    let ip_hash = hash_ip(ip);
    let count = store.session_count_recent_by_ip(&ip_hash, 15);
    count < max_attempts
}
