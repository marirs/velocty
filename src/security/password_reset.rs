use chrono::{Duration, Utc};
use rusqlite::params;

use crate::db::DbPool;
use crate::models::settings::Setting;

/// Create a password reset token for the given email. Stores in magic_links table with purpose.
pub fn create_token(pool: &DbPool, email: &str) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let token = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    let expires = now + Duration::minutes(30);

    conn.execute(
        "INSERT INTO magic_links (token, email, created_at, expires_at, used)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![token, email, now, expires],
    )
    .map_err(|e| format!("Failed to create reset token: {}", e))?;

    Ok(token)
}

/// Verify a password reset token. Returns the associated email if valid.
/// Marks the token as used so it cannot be reused.
pub fn verify_token(pool: &DbPool, token: &str) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let now = Utc::now().naive_utc();

    let result: Result<(String, bool), _> = conn.query_row(
        "SELECT email, used FROM magic_links WHERE token = ?1 AND expires_at > ?2",
        params![token, now],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((email, used)) => {
            if used {
                return Err("This reset link has already been used".into());
            }
            conn.execute(
                "UPDATE magic_links SET used = 1 WHERE token = ?1",
                params![token],
            )
            .map_err(|e| format!("Failed to mark token as used: {}", e))?;
            Ok(email)
        }
        Err(_) => Err("Invalid or expired reset link".into()),
    }
}

/// Send a password reset email.
pub fn send_reset_email(pool: &DbPool, email: &str, token: &str) -> Result<(), String> {
    let settings = Setting::all(pool);
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let admin_slug = settings
        .get("admin_slug")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());

    let link = format!(
        "{}/{}/reset-password?token={}",
        site_url.trim_end_matches('/'),
        admin_slug,
        token
    );

    let subject = format!("Reset your password — {}", site_name);
    let body = format!(
        "Hello,\n\n\
         A password reset was requested for your account on {}.\n\n\
         Click the link below to set a new password:\n\n\
         {}\n\n\
         This link expires in 30 minutes and can only be used once.\n\n\
         If you didn't request this, you can safely ignore this email.\n\n\
         — {}\n",
        site_name, link, site_name
    );

    let from_email = get_from_email(&settings)
        .ok_or("No email provider configured. Password reset requires an email provider.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Send an admin-initiated password reset email with a temporary password.
pub fn send_admin_reset_email(
    pool: &DbPool,
    email: &str,
    temp_password: &str,
) -> Result<(), String> {
    let settings = Setting::all(pool);
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let admin_slug = settings
        .get("admin_slug")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());

    let login_url = format!("{}/{}/login", site_url.trim_end_matches('/'), admin_slug);

    let subject = format!("Your password has been reset — {}", site_name);
    let body = format!(
        "Hello,\n\n\
         An administrator has reset your password on {}.\n\n\
         Your temporary password is:\n\n\
         {}\n\n\
         Please log in and change your password immediately:\n\
         {}\n\n\
         — {}\n",
        site_name, temp_password, login_url, site_name
    );

    let from_email = get_from_email(&settings).ok_or("No email provider configured.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Generate a random temporary password (12 chars, alphanumeric + symbols).
pub fn generate_temp_password() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut seed = ts as u64;
    let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789!@#$%";
    let mut password = String::with_capacity(12);
    for _ in 0..12 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let idx = ((seed >> 32) as usize) % chars.len();
        password.push(chars[idx] as char);
    }
    password
}

fn get_from_email(settings: &std::collections::HashMap<String, String>) -> Option<String> {
    let from_addr = settings
        .get("email_from_address")
        .cloned()
        .unwrap_or_default();
    if !from_addr.is_empty() {
        return Some(from_addr);
    }
    if settings.get("email_gmail_enabled").map(|v| v.as_str()) == Some("true") {
        return settings
            .get("email_gmail_address")
            .cloned()
            .filter(|s| !s.is_empty());
    }
    if settings.get("email_smtp_enabled").map(|v| v.as_str()) == Some("true") {
        return settings
            .get("email_smtp_username")
            .cloned()
            .filter(|s| !s.is_empty());
    }
    let api_providers = [
        "email_resend_enabled",
        "email_ses_enabled",
        "email_postmark_enabled",
        "email_brevo_enabled",
        "email_sendpulse_enabled",
        "email_mailgun_enabled",
        "email_moosend_enabled",
        "email_mandrill_enabled",
        "email_sparkpost_enabled",
    ];
    for provider in &api_providers {
        if settings.get(*provider).map(|v| v.as_str()) == Some("true") {
            return settings
                .get("admin_email")
                .cloned()
                .filter(|s| !s.is_empty());
        }
    }
    None
}
