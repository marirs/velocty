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

// ── MFA / TOTP ───────────────────────────────────────────

const MFA_PENDING_COOKIE: &str = "velocty_mfa_pending";

/// Generate a new TOTP secret (base32-encoded)
pub fn mfa_generate_secret() -> String {
    use totp_rs::{Algorithm, Secret, TOTP};
    let secret = Secret::generate_secret();
    secret.to_encoded().to_string()
}

/// Build a TOTP instance from a base32 secret
fn mfa_totp(secret_b32: &str, issuer: &str, account: &str) -> Result<totp_rs::TOTP, String> {
    use totp_rs::{Algorithm, Secret, TOTP};
    let secret_bytes = Secret::Encoded(secret_b32.to_string())
        .to_bytes()
        .map_err(|e| format!("Invalid secret: {}", e))?;
    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes, Some(issuer.to_string()), account.to_string())
        .map_err(|e| format!("TOTP error: {}", e))
}

/// Generate a QR code as a data URI (PNG base64) for the TOTP secret
pub fn mfa_qr_data_uri(secret_b32: &str, issuer: &str, account: &str) -> Result<String, String> {
    let totp = mfa_totp(secret_b32, issuer, account)?;
    totp.get_qr_base64().map_err(|e| format!("QR error: {}", e))
        .map(|b64| format!("data:image/png;base64,{}", b64))
}

/// Verify a 6-digit TOTP code against the secret
pub fn mfa_verify_code(secret_b32: &str, code: &str) -> bool {
    let totp = match mfa_totp(secret_b32, "Velocty", "admin") {
        Ok(t) => t,
        Err(_) => return false,
    };
    totp.check_current(code).unwrap_or(false)
}

/// Generate 10 recovery codes (8 chars each, alphanumeric)
pub fn mfa_generate_recovery_codes() -> Vec<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    (0..10)
        .map(|_| {
            let code: String = (0..8)
                .map(|_| chars[rng.gen_range(0..chars.len())] as char)
                .collect();
            format!("{}-{}", &code[..4], &code[4..])
        })
        .collect()
}

/// Set the MFA pending cookie (stores session_id of the pending auth)
pub fn set_mfa_pending_cookie(cookies: &CookieJar<'_>, pending_token: &str) {
    let mut cookie = Cookie::new(MFA_PENDING_COOKIE, pending_token.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(rocket::http::SameSite::Strict);
    cookie.set_path("/");
    cookie.set_max_age(rocket::time::Duration::minutes(5));
    cookies.add_private(cookie);
}

/// Get and clear the MFA pending cookie
pub fn take_mfa_pending_cookie(cookies: &CookieJar<'_>) -> Option<String> {
    let val = cookies.get_private(MFA_PENDING_COOKIE).map(|c| c.value().to_string());
    cookies.remove_private(Cookie::from(MFA_PENDING_COOKIE));
    val
}

/// Get the MFA pending cookie without clearing it
pub fn get_mfa_pending_cookie(cookies: &CookieJar<'_>) -> Option<String> {
    cookies.get_private(MFA_PENDING_COOKIE).map(|c| c.value().to_string())
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
