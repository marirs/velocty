use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::security::{auth, mfa};
use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::AdminSlug;

use super::super::NoCacheTemplate;

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaForm {
    pub code: String,
}

#[get("/mfa")]
pub fn mfa_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>, cookies: &CookieJar<'_>) -> Result<NoCacheTemplate, Redirect> {
    // Only show MFA page if there's a pending token
    if mfa::get_pending_cookie(cookies).is_none() {
        return Err(Redirect::to(format!("/{}/login", admin_slug.0)));
    }
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert("admin_theme".to_string(), Setting::get_or(pool, "admin_theme", "dark"));
    ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
    Ok(NoCacheTemplate(Template::render("admin/mfa", &ctx)))
}

#[post("/mfa", data = "<form>")]
pub fn mfa_submit(
    form: Form<MfaForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    let theme = Setting::get_or(pool, "admin_theme", "dark");

    // Verify pending token exists
    if mfa::get_pending_cookie(cookies).is_none() {
        return Ok(Redirect::to(format!("/{}/login", admin_slug.0)));
    }

    let mfa_secret = Setting::get_or(pool, "mfa_secret", "");
    let code = form.code.trim();

    // Try TOTP code first
    let mut valid = mfa::verify_code(&mfa_secret, code);

    // If TOTP failed, try recovery code
    if !valid {
        let codes_json = Setting::get_or(pool, "mfa_recovery_codes", "[]");
        let mut codes: Vec<String> = serde_json::from_str(&codes_json).unwrap_or_default();
        let code_upper = code.to_uppercase();
        if let Some(pos) = codes.iter().position(|c| c == &code_upper) {
            codes.remove(pos);
            let updated = serde_json::to_string(&codes).unwrap_or_else(|_| "[]".to_string());
            let _ = Setting::set(pool, "mfa_recovery_codes", &updated);
            valid = true;
        }
    }

    if !valid {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid code. Please try again.".to_string());
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        return Err(Template::render("admin/mfa", &ctx));
    }

    // Clear the pending cookie
    let _ = mfa::take_pending_cookie(cookies);

    // Create session
    match auth::create_session(pool, None, None) {
        Ok(session_id) => {
            auth::set_session_cookie(cookies, &session_id);
            Ok(Redirect::to(format!("/{}", admin_slug.0)))
        }
        Err(_) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), "Session creation failed".to_string());
            ctx.insert("admin_theme".to_string(), theme);
            ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
            Err(Template::render("admin/mfa", &ctx))
        }
    }
}
