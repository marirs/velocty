use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use std::sync::Arc;

use crate::rate_limit::RateLimiter;
use crate::security::{self, auth, magic_link, mfa};
use crate::store::Store;
use crate::AdminSlug;

use super::login::inject_captcha_context;

use super::super::NoCacheTemplate;

#[derive(Debug, FromForm, Deserialize)]
pub struct MagicLinkForm {
    pub email: String,
    pub captcha_token: Option<String>,
}

// ── Request Magic Link ────────────────────────────────

#[get("/magic-link")]
pub fn magic_link_page(
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
) -> Result<NoCacheTemplate, Redirect> {
    let s: &dyn Store = &**store.inner();
    let login_method = s.setting_get_or("login_method", "password");
    if login_method != "magic_link" {
        return Err(Redirect::to(format!("/{}/login", admin_slug.get())));
    }
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        s.setting_get_or("admin_theme", "dark"),
    );
    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
    inject_captcha_context(s, &mut ctx);
    Ok(NoCacheTemplate(Template::render("admin/magic_link", &ctx)))
}

#[post("/magic-link", data = "<form>")]
pub fn magic_link_submit(
    form: Form<MagicLinkForm>,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    limiter: &State<RateLimiter>,
    client_ip: auth::ClientIp,
) -> Result<Template, Template> {
    let s: &dyn Store = &**store.inner();
    let theme = s.setting_get_or("admin_theme", "dark");

    let login_method = s.setting_get_or("login_method", "password");
    if login_method != "magic_link" {
        let mut ctx = HashMap::new();
        ctx.insert(
            "error".to_string(),
            "Magic link login is not enabled".to_string(),
        );
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
        return Err(Template::render("admin/magic_link", &ctx));
    }

    // Verify login captcha
    let captcha_token = form.captcha_token.as_deref().unwrap_or("");
    match security::verify_login_captcha(s, captcha_token, None) {
        Ok(false) => {
            let mut ctx = HashMap::new();
            ctx.insert(
                "error".to_string(),
                "Captcha verification failed. Please try again.".to_string(),
            );
            ctx.insert("admin_theme".to_string(), theme.clone());
            ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
            inject_captcha_context(s, &mut ctx);
            return Err(Template::render("admin/magic_link", &ctx));
        }
        Err(e) => log::warn!("Login captcha error (allowing): {}", e),
        _ => {}
    }

    // Rate limit magic link requests
    let rate_key = format!("magic_link:{}", client_ip.0);
    if !limiter.check_and_record(&rate_key, 3, std::time::Duration::from_secs(15 * 60)) {
        let mut ctx = HashMap::new();
        ctx.insert(
            "error".to_string(),
            "Too many requests. Please try again in 15 minutes.".to_string(),
        );
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
        return Err(Template::render("admin/magic_link", &ctx));
    }

    let admin_email = s.setting_get_or("admin_email", "");

    // Always show success message to prevent email enumeration
    let mut ctx = HashMap::new();
    ctx.insert("admin_theme".to_string(), theme.clone());
    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
    ctx.insert(
        "success".to_string(),
        "If that email is registered, a magic link has been sent. Check your inbox.".to_string(),
    );

    // Only actually send if the email matches a known user
    if !admin_email.is_empty() && form.email.trim() == admin_email {
        match magic_link::create_token(s, form.email.trim()) {
            Ok(token) => {
                if let Err(e) = magic_link::send_magic_link_email(s, &admin_email, &token) {
                    log::error!("Failed to send magic link email: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to create magic link token: {}", e);
            }
        }
    }

    Ok(Template::render("admin/magic_link", &ctx))
}

// ── Verify Magic Link ─────────────────────────────────

#[get("/magic-link/verify?<token>")]
pub fn magic_link_verify(
    token: &str,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    let s: &dyn Store = &**store.inner();
    let theme = s.setting_get_or("admin_theme", "dark");

    match magic_link::verify_token(s, token) {
        Ok(email) => {
            // Look up the user by email
            let user = match s.user_get_by_email(&email) {
                Some(u) => u,
                None => {
                    let mut ctx = HashMap::new();
                    ctx.insert("error".to_string(), "User not found".to_string());
                    ctx.insert("admin_theme".to_string(), theme);
                    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
                    return Err(Template::render("admin/magic_link", &ctx));
                }
            };

            if !user.is_active() {
                let mut ctx = HashMap::new();
                ctx.insert(
                    "error".to_string(),
                    "This account is suspended or locked.".to_string(),
                );
                ctx.insert("admin_theme".to_string(), theme);
                ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
                return Err(Template::render("admin/magic_link", &ctx));
            }

            // Check if MFA is required (per-user)
            if user.mfa_enabled && !user.mfa_secret.is_empty() {
                let pending_token = uuid::Uuid::new_v4().to_string();
                mfa::set_pending_cookie(cookies, &format!("{}:{}", user.id, pending_token));
                return Ok(Redirect::to(format!("/{}/mfa", admin_slug.get())));
            }

            // Create session directly
            let _ = s.user_touch_last_login(user.id);
            match auth::create_session(s, user.id, None, None) {
                Ok(session_id) => {
                    auth::set_session_cookie(cookies, &session_id);
                    Ok(Redirect::to(format!("/{}", admin_slug.get())))
                }
                Err(_) => {
                    let mut ctx = HashMap::new();
                    ctx.insert("error".to_string(), "Session creation failed".to_string());
                    ctx.insert("admin_theme".to_string(), theme);
                    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
                    Err(Template::render("admin/magic_link", &ctx))
                }
            }
        }
        Err(e) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), e);
            ctx.insert("admin_theme".to_string(), theme);
            ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
            Err(Template::render("admin/magic_link", &ctx))
        }
    }
}
