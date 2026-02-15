pub mod auth;
pub mod firewall;
pub mod mfa;
pub mod magic_link;
pub mod password_reset;
pub mod recaptcha;
pub mod turnstile;
pub mod hcaptcha;
pub mod akismet;
pub mod cleantalk;
pub mod oopspam;

use std::collections::HashMap;

use crate::db::DbPool;
use crate::models::settings::Setting;

// ── Captcha Verification ──────────────────────────────

/// Verify a captcha token using the specified provider (or auto-detect).
/// Returns Ok(true) if verified, Ok(false) if failed, Err on config/network error.
/// If no captcha provider is enabled, returns Ok(true) (pass-through).
pub fn verify_captcha(
    pool: &DbPool,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    let settings: HashMap<String, String> = Setting::all(pool);
    verify_captcha_with_settings(&settings, token, remote_ip)
}

/// Verify a captcha token for login specifically.
/// Checks login_captcha_enabled and login_captcha_provider settings.
pub fn verify_login_captcha(
    pool: &DbPool,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    let settings: HashMap<String, String> = Setting::all(pool);

    if settings.get("login_captcha_enabled").map(|v| v.as_str()) != Some("true") {
        return Ok(true);
    }

    let provider = settings.get("login_captcha_provider").map(|v| v.as_str()).unwrap_or("");
    if provider.is_empty() {
        return Ok(true);
    }

    match provider {
        "recaptcha" => recaptcha::verify(&settings, token, remote_ip),
        "turnstile" => turnstile::verify(&settings, token, remote_ip),
        "hcaptcha" => hcaptcha::verify(&settings, token, remote_ip),
        _ => Ok(true),
    }
}

/// Internal: verify using auto-detected provider from settings.
fn verify_captcha_with_settings(
    settings: &HashMap<String, String>,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    if settings.get("security_recaptcha_enabled").map(|v| v.as_str()) == Some("true") {
        return recaptcha::verify(settings, token, remote_ip);
    }
    if settings.get("security_turnstile_enabled").map(|v| v.as_str()) == Some("true") {
        return turnstile::verify(settings, token, remote_ip);
    }
    if settings.get("security_hcaptcha_enabled").map(|v| v.as_str()) == Some("true") {
        return hcaptcha::verify(settings, token, remote_ip);
    }
    Ok(true)
}

/// Get login captcha info for rendering on the login page.
/// Returns None if login captcha is disabled.
pub fn login_captcha_info(pool: &DbPool) -> Option<CaptchaInfo> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("login_captcha_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let provider = settings.get("login_captcha_provider").map(|v| v.as_str()).unwrap_or("");
    match provider {
        "recaptcha" => Some(CaptchaInfo {
            provider: "recaptcha".into(),
            site_key: settings.get("security_recaptcha_site_key").cloned().unwrap_or_default(),
            version: settings.get("security_recaptcha_version").cloned().unwrap_or_else(|| "v3".to_string()),
        }),
        "turnstile" => Some(CaptchaInfo {
            provider: "turnstile".into(),
            site_key: settings.get("security_turnstile_site_key").cloned().unwrap_or_default(),
            version: String::new(),
        }),
        "hcaptcha" => Some(CaptchaInfo {
            provider: "hcaptcha".into(),
            site_key: settings.get("security_hcaptcha_site_key").cloned().unwrap_or_default(),
            version: String::new(),
        }),
        _ => None,
    }
}

/// Get the active captcha provider name and site key for frontend rendering.
/// Returns None if no captcha provider is enabled.
pub fn active_captcha(pool: &DbPool) -> Option<CaptchaInfo> {
    let settings: HashMap<String, String> = Setting::all(pool);

    if settings.get("security_recaptcha_enabled").map(|v| v.as_str()) == Some("true") {
        let version = settings.get("security_recaptcha_version").cloned().unwrap_or_else(|| "v3".to_string());
        return Some(CaptchaInfo {
            provider: "recaptcha".into(),
            site_key: settings.get("security_recaptcha_site_key").cloned().unwrap_or_default(),
            version,
        });
    }
    if settings.get("security_turnstile_enabled").map(|v| v.as_str()) == Some("true") {
        return Some(CaptchaInfo {
            provider: "turnstile".into(),
            site_key: settings.get("security_turnstile_site_key").cloned().unwrap_or_default(),
            version: String::new(),
        });
    }
    if settings.get("security_hcaptcha_enabled").map(|v| v.as_str()) == Some("true") {
        return Some(CaptchaInfo {
            provider: "hcaptcha".into(),
            site_key: settings.get("security_hcaptcha_site_key").cloned().unwrap_or_default(),
            version: String::new(),
        });
    }

    None
}

pub struct CaptchaInfo {
    pub provider: String,
    pub site_key: String,
    pub version: String,
}

// ── Spam Detection ────────────────────────────────────

/// Check content for spam using enabled spam detection providers.
/// Returns Ok(true) if spam, Ok(false) if clean.
/// Checks all enabled providers — if ANY flags it as spam, returns true.
pub fn check_spam(
    pool: &DbPool,
    site_url: &str,
    user_ip: &str,
    user_agent: &str,
    content: &str,
    author: Option<&str>,
    author_email: Option<&str>,
) -> Result<bool, String> {
    let settings: HashMap<String, String> = Setting::all(pool);

    let mut checked = false;

    if settings.get("security_akismet_enabled").map(|v| v.as_str()) == Some("true") {
        checked = true;
        match akismet::check_spam(&settings, site_url, user_ip, user_agent, content, author, author_email, Some("comment")) {
            Ok(true) => {
                log::info!("Akismet flagged content as spam from {}", user_ip);
                return Ok(true);
            }
            Ok(false) => {}
            Err(e) => log::warn!("Akismet check failed: {}", e),
        }
    }

    if settings.get("security_cleantalk_enabled").map(|v| v.as_str()) == Some("true") {
        checked = true;
        match cleantalk::check_spam(&settings, user_ip, user_agent, content, author, author_email) {
            Ok(true) => {
                log::info!("CleanTalk flagged content as spam from {}", user_ip);
                return Ok(true);
            }
            Ok(false) => {}
            Err(e) => log::warn!("CleanTalk check failed: {}", e),
        }
    }

    if settings.get("security_oopspam_enabled").map(|v| v.as_str()) == Some("true") {
        checked = true;
        match oopspam::check_spam(&settings, user_ip, content, author_email) {
            Ok(true) => {
                log::info!("OOPSpam flagged content as spam from {}", user_ip);
                return Ok(true);
            }
            Ok(false) => {}
            Err(e) => log::warn!("OOPSpam check failed: {}", e),
        }
    }

    if !checked {
        log::debug!("No spam detection provider enabled, allowing content");
    }

    Ok(false)
}

/// Check if any spam detection provider is enabled.
pub fn has_spam_provider(pool: &DbPool) -> bool {
    let settings: HashMap<String, String> = Setting::all(pool);
    ["akismet", "cleantalk", "oopspam"]
        .iter()
        .any(|p| settings.get(&format!("security_{}_enabled", p)).map(|v| v.as_str()) == Some("true"))
}

/// Check if any captcha provider is enabled.
pub fn has_captcha_provider(pool: &DbPool) -> bool {
    let settings: HashMap<String, String> = Setting::all(pool);
    ["recaptcha", "turnstile", "hcaptcha"]
        .iter()
        .any(|p| settings.get(&format!("security_{}_enabled", p)).map(|v| v.as_str()) == Some("true"))
}
