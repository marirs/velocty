use chrono::{Duration, Utc};
use rusqlite::params;

use crate::db::DbPool;
use crate::models::settings::Setting;

/// Create a magic link token for the given email. Stores in DB, returns the token.
pub fn create_token(pool: &DbPool, email: &str) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    let token = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc();
    let expires = now + Duration::minutes(15);

    conn.execute(
        "INSERT INTO magic_links (token, email, created_at, expires_at, used)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![token, email, now, expires],
    )
    .map_err(|e| format!("Failed to create magic link: {}", e))?;

    Ok(token)
}

/// Verify a magic link token. Returns the associated email if valid.
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
                return Err("This magic link has already been used".into());
            }
            // Mark as used
            conn.execute(
                "UPDATE magic_links SET used = 1 WHERE token = ?1",
                params![token],
            )
            .map_err(|e| format!("Failed to mark token as used: {}", e))?;
            Ok(email)
        }
        Err(_) => Err("Invalid or expired magic link".into()),
    }
}

/// Send a magic link email to the admin.
pub fn send_magic_link_email(pool: &DbPool, email: &str, token: &str) -> Result<(), String> {
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
        "{}/{}/magic-link/verify?token={}",
        site_url.trim_end_matches('/'),
        admin_slug,
        token
    );

    let subject = format!("Sign in to {} — Magic Link", site_name);
    let body = format!(
        "Hello,\n\n\
         Click the link below to sign in to {}:\n\n\
         {}\n\n\
         This link expires in 15 minutes and can only be used once.\n\n\
         If you didn't request this, you can safely ignore this email.\n\n\
         — {}\n",
        site_name, link, site_name
    );

    // Use the email module's configured provider to send
    let from_email = get_from_email(&settings)
        .ok_or("No email provider configured. Magic link requires an email provider.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Determine the "from" email address (mirrors email module logic)
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

/// Clean up expired magic link tokens
pub fn cleanup_expired(pool: &DbPool) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let now = Utc::now().naive_utc();
    conn.execute(
        "DELETE FROM magic_links WHERE expires_at < ?1",
        params![now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
