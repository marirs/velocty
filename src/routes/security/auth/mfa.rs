use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use std::sync::Arc;

use crate::security::{auth, mfa};
use crate::store::Store;
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
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
) -> Result<NoCacheTemplate, Redirect> {
    if pending_user_id(cookies).is_none() {
        return Err(Redirect::to(format!("/{}/login", admin_slug.get())));
    }
    let s: &dyn Store = &**store.inner();
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        s.setting_get_or("admin_theme", "dark"),
    );
    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
    Ok(NoCacheTemplate(Template::render("admin/mfa", &ctx)))
}

#[post("/mfa", data = "<form>")]
pub fn mfa_submit(
    form: Form<MfaForm>,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    let s: &dyn Store = &**store.inner();
    let theme = s.setting_get_or("admin_theme", "dark");

    // Extract user_id from pending cookie
    let user_id = match pending_user_id(cookies) {
        Some(id) => id,
        None => return Ok(Redirect::to(format!("/{}/login", admin_slug.get()))),
    };

    let user = match s.user_get_by_id(user_id) {
        Some(u) => u,
        None => return Ok(Redirect::to(format!("/{}/login", admin_slug.get()))),
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
            let _ = s.user_update_mfa(user.id, true, &user.mfa_secret, &updated);
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
        ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
        return Err(Template::render("admin/mfa", &ctx));
    }

    // Clear the pending cookie
    let _ = mfa::take_pending_cookie(cookies);

    // Create session with user_id
    let _ = s.user_touch_last_login(user.id);
    match auth::create_session(s, user.id, None, None) {
        Ok(session_id) => {
            let is_https = s.setting_get_or("site_url", "").starts_with("https://");
            auth::set_session_cookie_secure(cookies, &session_id, is_https);
            Ok(Redirect::to(format!("/{}", admin_slug.get())))
        }
        Err(_) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), "Session creation failed".to_string());
            ctx.insert("admin_theme".to_string(), theme);
            ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
            Err(Template::render("admin/mfa", &ctx))
        }
    }
}
