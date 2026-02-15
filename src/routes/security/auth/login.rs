use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::security::{self, auth, mfa};
use crate::db::DbPool;
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::settings::Setting;
use crate::rate_limit::RateLimiter;
use crate::AdminSlug;

#[derive(Debug, FromForm, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub captcha_token: Option<String>,
}

/// Returns true if this is a fresh install (no admin email set)
pub fn needs_setup(pool: &DbPool) -> bool {
    let email = Setting::get_or(pool, "admin_email", "");
    let hash = Setting::get_or(pool, "admin_password_hash", "");
    email.is_empty() || hash.is_empty()
}

#[get("/login")]
pub fn login_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Result<Template, Redirect> {
    if needs_setup(pool) {
        return Err(Redirect::to(format!("/{}/setup", admin_slug.0)));
    }
    let login_method = Setting::get_or(pool, "login_method", "password");
    if login_method == "magic_link" {
        return Err(Redirect::to(format!("/{}/magic-link", admin_slug.0)));
    }
    let mut context: HashMap<String, String> = HashMap::new();
    context.insert("admin_theme".to_string(), Setting::get_or(pool, "admin_theme", "dark"));
    context.insert("admin_slug".to_string(), admin_slug.0.clone());
    inject_captcha_context(pool, &mut context);
    Ok(Template::render("admin/login", &context))
}

#[post("/login", data = "<form>")]
pub fn login_submit(
    form: Form<LoginForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
    limiter: &State<RateLimiter>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    let theme = Setting::get_or(pool, "admin_theme", "dark");
    let ip_hash = auth::hash_ip(&form.email);
    let rate_key = format!("login:{}", ip_hash);
    let max_attempts = Setting::get_i64(pool, "login_rate_limit").max(1) as u64;
    let window = std::time::Duration::from_secs(15 * 60);

    // Check rate limit before processing
    if !limiter.check_and_record(&rate_key, max_attempts, window) {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Too many login attempts. Please try again in 15 minutes.".to_string());
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        inject_captcha_context(pool, &mut ctx);
        return Err(Template::render("admin/login", &ctx));
    }

    // Verify login captcha
    let captcha_token = form.captcha_token.as_deref().unwrap_or("");
    match security::verify_login_captcha(pool, captcha_token, None) {
        Ok(false) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), "Captcha verification failed. Please try again.".to_string());
            ctx.insert("admin_theme".to_string(), theme);
            ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
            inject_captcha_context(pool, &mut ctx);
            return Err(Template::render("admin/login", &ctx));
        }
        Err(e) => log::warn!("Login captcha error (allowing): {}", e),
        _ => {}
    }

    let stored_hash = Setting::get(pool, "admin_password_hash").unwrap_or_default();
    let admin_email = Setting::get_or(pool, "admin_email", "");

    if !admin_email.is_empty() && form.email != admin_email {
        // Firewall: log failed login with unknown user
        if Setting::get_or(pool, "firewall_enabled", "false") == "true"
            && Setting::get_or(pool, "fw_failed_login_tracking", "true") == "true"
        {
            FwEvent::log(pool, &ip_hash, "failed_login", Some(&format!("Unknown user: {}", form.email)), None, None, Some("login"));

            // Ban unknown users immediately if configured
            if Setting::get_or(pool, "fw_ban_unknown_users", "false") == "true" {
                let dur = Setting::get_or(pool, "fw_unknown_user_ban_duration", "24h");
                let _ = FwBan::create_with_duration(pool, &ip_hash, "unknown_user", Some(&format!("Login attempt with unknown user: {}", form.email)), &dur, None, None);
            }
        }
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        inject_captcha_context(pool, &mut ctx);
        return Err(Template::render("admin/login", &ctx));
    }

    if !auth::verify_password(&form.password, &stored_hash) {
        // Firewall: log failed login and check ban threshold
        if Setting::get_or(pool, "firewall_enabled", "false") == "true"
            && Setting::get_or(pool, "fw_failed_login_tracking", "true") == "true"
        {
            FwEvent::log(pool, &ip_hash, "failed_login", Some("Wrong password"), None, None, Some("login"));

            let threshold: i64 = Setting::get_or(pool, "fw_failed_login_ban_threshold", "5")
                .parse().unwrap_or(5);
            let count = FwEvent::count_for_ip_since(pool, &ip_hash, "failed_login", 15);
            if count >= threshold {
                let dur = Setting::get_or(pool, "fw_failed_login_ban_duration", "1h");
                let _ = FwBan::create_with_duration(pool, &ip_hash, "failed_login", Some("Too many failed login attempts"), &dur, None, None);
            }
        }
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
        ctx.insert("admin_theme".to_string(), theme.clone());
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        inject_captcha_context(pool, &mut ctx);
        return Err(Template::render("admin/login", &ctx));
    }

    // Check MFA
    let mfa_enabled = Setting::get_bool(pool, "mfa_enabled");
    let mfa_secret = Setting::get_or(pool, "mfa_secret", "");
    if mfa_enabled && !mfa_secret.is_empty() {
        // Store a pending token so the MFA page knows password was verified
        let pending_token = uuid::Uuid::new_v4().to_string();
        mfa::set_pending_cookie(cookies, &pending_token);
        return Ok(Redirect::to(format!("/{}/mfa", admin_slug.0)));
    }

    // Create session (no MFA)
    match auth::create_session(pool, None, None) {
        Ok(session_id) => {
            auth::set_session_cookie(cookies, &session_id);
            Ok(Redirect::to(format!("/{}", admin_slug.0)))
        }
        Err(_) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), "Session creation failed".to_string());
            ctx.insert("admin_theme".to_string(), theme.clone());
            ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
            inject_captcha_context(pool, &mut ctx);
            Err(Template::render("admin/login", &ctx))
        }
    }
}

/// Inject captcha provider/site_key/version into template context if login captcha is enabled.
pub fn inject_captcha_context(pool: &DbPool, ctx: &mut HashMap<String, String>) {
    if let Some(info) = security::login_captcha_info(pool) {
        ctx.insert("captcha_provider".to_string(), info.provider);
        ctx.insert("captcha_site_key".to_string(), info.site_key);
        ctx.insert("captcha_version".to_string(), info.version);
    }
}
