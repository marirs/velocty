use rocket::form::Form;
use rocket::http::{CookieJar, Header};
use rocket::response::{self, Redirect, Responder};
use rocket::{Request, State};
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Wrapper that adds no-cache headers to a Template response
pub struct NoCacheTemplate(Template);

impl<'r> Responder<'r, 'static> for NoCacheTemplate {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let mut resp = self.0.respond_to(req)?;
        resp.set_header(Header::new("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0"));
        resp.set_header(Header::new("Pragma", "no-cache"));
        Ok(resp)
    }
}

use crate::auth;
use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::rate_limit::RateLimiter;
use crate::AdminSlug;

#[derive(Debug, FromForm, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaForm {
    pub code: String,
}

/// Returns true if this is a fresh install (no admin email set)
fn needs_setup(pool: &DbPool) -> bool {
    let email = Setting::get_or(pool, "admin_email", "");
    let hash = Setting::get_or(pool, "admin_password_hash", "");
    email.is_empty() || hash.is_empty()
}

#[get("/login")]
pub fn login_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Result<Template, Redirect> {
    if needs_setup(pool) {
        return Err(Redirect::to(format!("/{}/setup", admin_slug.0)));
    }
    let mut context: HashMap<String, String> = HashMap::new();
    context.insert("admin_theme".to_string(), Setting::get_or(pool, "admin_theme", "dark"));
    context.insert("admin_slug".to_string(), admin_slug.0.clone());
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
        return Err(Template::render("admin/login", &ctx));
    }

    let stored_hash = Setting::get(pool, "admin_password_hash").unwrap_or_default();
    let admin_email = Setting::get_or(pool, "admin_email", "");

    if !admin_email.is_empty() && form.email != admin_email {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        return Err(Template::render("admin/login", &ctx));
    }

    if !auth::verify_password(&form.password, &stored_hash) {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
        ctx.insert("admin_theme".to_string(), theme.clone());
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        return Err(Template::render("admin/login", &ctx));
    }

    // Check MFA
    let mfa_enabled = Setting::get_bool(pool, "mfa_enabled");
    if mfa_enabled {
        // TODO: Store pending auth state and redirect to MFA page
        // For now, proceed without MFA
    }

    // Create session
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
            Err(Template::render("admin/login", &ctx))
        }
    }
}

#[get("/logout")]
pub fn logout(pool: &State<DbPool>, admin_slug: &State<AdminSlug>, cookies: &CookieJar<'_>) -> Redirect {
    if let Some(cookie) = cookies.get_private("velocty_session") {
        let _ = auth::destroy_session(pool, cookie.value());
    }
    auth::clear_session_cookie(cookies);
    Redirect::to(format!("/{}/login", admin_slug.0))
}

/// Catch-all for any /<admin_slug>/* route that failed the AdminUser guard.
/// This fires when the guard returns Forward(Unauthorized).
#[get("/<_path..>", rank = 99)]
pub fn admin_redirect_to_login(_path: std::path::PathBuf, pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Redirect {
    if needs_setup(pool) {
        Redirect::to(format!("/{}/setup", admin_slug.0))
    } else {
        Redirect::to(format!("/{}/login", admin_slug.0))
    }
}

// ── First-Time Setup Wizard ──────────────────────────────────────────

#[derive(Debug, Serialize)]
struct SetupContext {
    error: Option<String>,
    site_name: String,
    admin_email: String,
}

#[derive(Debug, FromForm, Deserialize)]
pub struct SetupForm {
    pub site_name: String,
    pub admin_email: String,
    pub password: String,
    pub confirm_password: String,
    pub accept_terms: Option<String>,
}

#[get("/setup")]
pub fn setup_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Result<NoCacheTemplate, Redirect> {
    if !needs_setup(pool) {
        return Err(Redirect::to(format!("/{}/login", admin_slug.0)));
    }
    let ctx = SetupContext {
        error: None,
        site_name: "Velocty".to_string(),
        admin_email: String::new(),
    };
    Ok(NoCacheTemplate(Template::render("admin/setup", &ctx)))
}

#[post("/setup", data = "<form>")]
pub fn setup_submit(
    form: Form<SetupForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
) -> Result<Redirect, Template> {
    if !needs_setup(pool) {
        return Ok(Redirect::to(format!("/{}/login", admin_slug.0)));
    }

    let make_err = |msg: &str, form: &SetupForm| {
        let ctx = SetupContext {
            error: Some(msg.to_string()),
            site_name: form.site_name.clone(),
            admin_email: form.admin_email.clone(),
        };
        Template::render("admin/setup", &ctx)
    };

    // Validate
    if form.admin_email.trim().is_empty() {
        return Err(make_err("Email is required.", &form));
    }
    if form.password.len() < 8 {
        return Err(make_err("Password must be at least 8 characters.", &form));
    }
    if form.password != form.confirm_password {
        return Err(make_err("Passwords do not match.", &form));
    }
    if form.accept_terms.as_deref() != Some("true") {
        return Err(make_err("You must accept the Terms of Use and Privacy Policy.", &form));
    }

    // Save
    let hash = auth::hash_password(&form.password)
        .map_err(|_| make_err("Failed to hash password.", &form))?;

    let _ = Setting::set(pool, "site_name", form.site_name.trim());
    let _ = Setting::set(pool, "admin_email", form.admin_email.trim());
    let _ = Setting::set(pool, "admin_password_hash", &hash);
    let _ = Setting::set(pool, "setup_completed", "true");

    Ok(Redirect::to(format!("/{}/login", admin_slug.0)))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![login_page, login_submit, logout, admin_redirect_to_login, setup_page, setup_submit]
}
