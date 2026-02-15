use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::security::{self, auth, mfa};
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::settings::Setting;
use crate::models::user::User;
use crate::rate_limit::RateLimiter;
use crate::AdminSlug;

#[derive(Debug, FromForm, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub captcha_token: Option<String>,
}

/// Returns true if this is a fresh install (no users exist)
pub fn needs_setup(pool: &DbPool) -> bool {
    User::count(pool) == 0
}

#[get("/login?<reset>")]
pub fn login_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>, reset: Option<&str>) -> Result<Template, Redirect> {
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
    if reset == Some("success") {
        context.insert("reset_success".to_string(), "true".to_string());
    }
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
    client_ip: auth::ClientIp,
) -> Result<Redirect, Template> {
    let theme = Setting::get_or(pool, "admin_theme", "dark");
    let ip = &client_ip.0;
    let rate_key = format!("login:{}", ip);
    let max_attempts = Setting::get_i64(pool, "login_rate_limit").max(1) as u64;
    let window = std::time::Duration::from_secs(15 * 60);

    let make_err = |msg: &str, theme: &str, pool: &DbPool, slug: &str| -> Template {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), msg.to_string());
        ctx.insert("admin_theme".to_string(), theme.to_string());
        ctx.insert("admin_slug".to_string(), slug.to_string());
        inject_captcha_context(pool, &mut ctx);
        Template::render("admin/login", &ctx)
    };

    // Check rate limit before processing
    if !limiter.check_and_record(&rate_key, max_attempts, window) {
        return Err(make_err("Too many login attempts. Please try again in 15 minutes.", &theme, pool, &admin_slug.0));
    }

    // Verify login captcha
    let captcha_token = form.captcha_token.as_deref().unwrap_or("");
    match security::verify_login_captcha(pool, captcha_token, None) {
        Ok(false) => {
            return Err(make_err("Captcha verification failed. Please try again.", &theme, pool, &admin_slug.0));
        }
        Err(e) => log::warn!("Login captcha error (allowing): {}", e),
        _ => {}
    }

    // Look up user by email
    let user = match User::get_by_email(pool, &form.email) {
        Some(u) => u,
        None => {
            // Firewall: unknown user
            if Setting::get_or(pool, "firewall_enabled", "false") == "true"
                && Setting::get_or(pool, "fw_failed_login_tracking", "true") == "true"
            {
                FwEvent::log(pool, ip, "failed_login", Some(&format!("Unknown user: {}", form.email)), None, None, Some("login"));
                if Setting::get_or(pool, "fw_ban_unknown_users", "false") == "true" {
                    let dur = Setting::get_or(pool, "fw_unknown_user_ban_duration", "24h");
                    let _ = FwBan::create_with_duration(pool, ip, "unknown_user", Some(&format!("Login attempt with unknown user: {}", form.email)), &dur, None, None);
                }
            }
            return Err(make_err("Invalid credentials", &theme, pool, &admin_slug.0));
        }
    };

    // Check account status
    if !user.is_active() {
        return Err(make_err("This account is suspended or locked. Contact an administrator.", &theme, pool, &admin_slug.0));
    }

    // Check role — subscribers cannot log into admin
    if user.role == "subscriber" {
        return Err(make_err("Your account does not have admin panel access.", &theme, pool, &admin_slug.0));
    }

    // Verify password
    if !auth::verify_password(&form.password, &user.password_hash) {
        // Firewall: failed password
        if Setting::get_or(pool, "firewall_enabled", "false") == "true"
            && Setting::get_or(pool, "fw_failed_login_tracking", "true") == "true"
        {
            FwEvent::log(pool, ip, "failed_login", Some("Wrong password"), None, None, Some("login"));
            let threshold: i64 = Setting::get_or(pool, "fw_failed_login_ban_threshold", "5")
                .parse().unwrap_or(5);
            let count = FwEvent::count_for_ip_since(pool, ip, "failed_login", 15);
            if count >= threshold {
                let dur = Setting::get_or(pool, "fw_failed_login_ban_duration", "1h");
                let _ = FwBan::create_with_duration(pool, ip, "failed_login", Some("Too many failed login attempts"), &dur, None, None);
            }
        }
        AuditEntry::log(pool, Some(user.id), Some(&user.display_name), "login_failed", Some("user"), Some(user.id), Some(&user.email), Some("Wrong password"), Some(ip));
        return Err(make_err("Invalid credentials", &theme, pool, &admin_slug.0));
    }

    // Check MFA (per-user)
    if user.mfa_enabled && !user.mfa_secret.is_empty() {
        let pending_token = uuid::Uuid::new_v4().to_string();
        // Store user_id in a pending cookie so MFA page can complete login
        mfa::set_pending_cookie(cookies, &format!("{}:{}", user.id, pending_token));
        return Ok(Redirect::to(format!("/{}/mfa", admin_slug.0)));
    }

    // Create session
    let _ = User::touch_last_login(pool, user.id);
    match auth::create_session(pool, user.id, None, None) {
        Ok(session_id) => {
            auth::set_session_cookie(cookies, &session_id);
            AuditEntry::log(pool, Some(user.id), Some(&user.display_name), "login", Some("user"), Some(user.id), Some(&user.email), None, Some(ip));
            Ok(Redirect::to(format!("/{}", admin_slug.0)))
        }
        Err(_) => Err(make_err("Session creation failed", &theme, pool, &admin_slug.0)),
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
