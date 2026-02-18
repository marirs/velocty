use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::models::user::User;
use crate::security::{auth, mfa};
use crate::AdminSlug;

use super::super::NoCacheTemplate;

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaForm {
    pub code: String,
}

/// Extract user_id from pending cookie value "user_id:token"
fn pending_user_id(cookies: &CookieJar<'_>) -> Option<i64> {
    let val = mfa::get_pending_cookie(cookies)?;
    val.split(':').next()?.parse().ok()
}

#[get("/mfa")]
pub fn mfa_page(
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
) -> Result<NoCacheTemplate, Redirect> {
    // Only show MFA page if there's a pending token with a valid user_id
    if pending_user_id(cookies).is_none() {
        return Err(Redirect::to(format!("/{}/login", admin_slug.0)));
    }
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        Setting::get_or(pool, "admin_theme", "dark"),
    );
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

    // Extract user_id from pending cookie
    let user_id = match pending_user_id(cookies) {
        Some(id) => id,
        None => return Ok(Redirect::to(format!("/{}/login", admin_slug.0))),
    };

    let user = match User::get_by_id(pool, user_id) {
        Some(u) => u,
        None => return Ok(Redirect::to(format!("/{}/login", admin_slug.0))),
    };

    let code = form.code.trim();

    // Try TOTP code first (against user's own secret)
    let mut valid = mfa::verify_code(&user.mfa_secret, code);

    // If TOTP failed, try recovery code
    if !valid {
        let mut codes: Vec<String> =
            serde_json::from_str(&user.mfa_recovery_codes).unwrap_or_default();
        let code_upper = code.to_uppercase();
        if let Some(pos) = codes.iter().position(|c| c == &code_upper) {
            codes.remove(pos);
            let updated = serde_json::to_string(&codes).unwrap_or_else(|_| "[]".to_string());
            let _ = User::update_mfa(pool, user.id, true, &user.mfa_secret, &updated);
            valid = true;
        }
    }

    if !valid {
        let mut ctx = HashMap::new();
        ctx.insert(
            "error".to_string(),
            "Invalid code. Please try again.".to_string(),
        );
        ctx.insert("admin_theme".to_string(), theme);
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        return Err(Template::render("admin/mfa", &ctx));
    }

    // Clear the pending cookie
    let _ = mfa::take_pending_cookie(cookies);

    // Create session with user_id
    let _ = User::touch_last_login(pool, user.id);
    match auth::create_session(pool, user.id, None, None) {
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
