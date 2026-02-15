use chrono::{Duration, Utc};
use rocket::http::{Cookie, CookieJar, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::models::user::User;

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
    let pool = request.guard::<&State<DbPool>>().await.succeeded()?;
    let cookies = request.cookies();
    let session_id = cookies.get_private(SESSION_COOKIE)?.value().to_string();

    match get_session_user(pool, &session_id) {
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

pub fn create_session(pool: &DbPool, user_id: i64, ip: Option<&str>, ua: Option<&str>) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let expiry_hours = Setting::get_i64(pool, "session_expiry_hours").max(1);
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    let expires = now + Duration::hours(expiry_hours);

    conn.execute(
        "INSERT INTO sessions (id, user_id, created_at, expires_at, ip_address, user_agent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, user_id, now, expires, ip, ua],
    )
    .map_err(|e| e.to_string())?;

    Ok(session_id)
}

/// Validate a session and return the associated user
pub fn get_session_user(pool: &DbPool, session_id: &str) -> Option<User> {
    let conn = pool.get().ok()?;
    let now = Utc::now().naive_utc();

    let user_id: i64 = conn
        .query_row(
            "SELECT user_id FROM sessions WHERE id = ?1 AND expires_at > ?2 AND user_id IS NOT NULL",
            params![session_id, now],
            |row| row.get(0),
        )
        .ok()?;

    User::get_by_id(pool, user_id)
}

/// Legacy: validate session exists (for backward compat during migration)
pub fn validate_session(pool: &DbPool, session_id: &str) -> bool {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let now = Utc::now().naive_utc();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND expires_at > ?2",
            params![session_id, now],
            |row| row.get(0),
        )
        .unwrap_or(0);

    count > 0
}

pub fn destroy_session(pool: &DbPool, session_id: &str) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_session_cookie(cookies: &CookieJar<'_>, session_id: &str) {
    let mut cookie = Cookie::new(SESSION_COOKIE, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(rocket::http::SameSite::Strict);
    cookie.set_path("/");
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

pub fn cleanup_expired_sessions(pool: &DbPool) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let now = Utc::now().naive_utc();
    conn.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn check_login_rate_limit(pool: &DbPool, ip: &str) -> bool {
    let max_attempts = Setting::get_i64(pool, "login_rate_limit").max(1);
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let ip_hash = hash_ip(ip);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions
             WHERE ip_address = ?1
             AND created_at > datetime('now', '-15 minutes')",
            params![ip_hash],
            |row| row.get(0),
        )
        .unwrap_or(0);

    count < max_attempts
}
