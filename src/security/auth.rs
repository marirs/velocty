use chrono::{Duration, Utc};
use rocket::http::{Cookie, CookieJar, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::db::DbPool;
use crate::models::settings::Setting;

const SESSION_COOKIE: &str = "velocty_session";

/// Guard that ensures the request is from an authenticated admin
pub struct AdminUser;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let pool = match request.guard::<&State<DbPool>>().await {
            Outcome::Success(p) => p,
            _ => return Outcome::Forward(Status::Unauthorized),
        };

        let cookies = request.cookies();
        let session_id = match cookies.get_private(SESSION_COOKIE) {
            Some(c) => c.value().to_string(),
            None => return Outcome::Forward(Status::Unauthorized),
        };

        if validate_session(pool, &session_id) {
            Outcome::Success(AdminUser)
        } else {
            cookies.remove_private(Cookie::from(SESSION_COOKIE));
            Outcome::Forward(Status::Unauthorized)
        }
    }
}

pub fn hash_password(password: &str) -> Result<String, String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|e| e.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

pub fn create_session(pool: &DbPool, ip: Option<&str>, ua: Option<&str>) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let expiry_hours = Setting::get_i64(pool, "session_expiry_hours").max(1);
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    let expires = now + Duration::hours(expiry_hours);

    conn.execute(
        "INSERT INTO sessions (id, created_at, expires_at, ip_address, user_agent)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_id, now, expires, ip, ua],
    )
    .map_err(|e| e.to_string())?;

    Ok(session_id)
}

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
    // Count recent failed sessions from this IP (sessions created in last 15 min)
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
