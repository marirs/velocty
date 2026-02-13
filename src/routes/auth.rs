use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::auth;
use crate::db::DbPool;
use crate::models::settings::Setting;

#[derive(Debug, FromForm, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaForm {
    pub code: String,
}

#[get("/admin/login")]
pub fn login_page() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("admin/login", &context)
}

#[post("/admin/login", data = "<form>")]
pub fn login_submit(
    form: Form<LoginForm>,
    pool: &State<DbPool>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    let stored_hash = Setting::get(pool, "admin_password_hash").unwrap_or_default();
    let admin_email = Setting::get_or(pool, "admin_email", "");

    // Verify credentials
    if !admin_email.is_empty() && form.email != admin_email {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
        return Err(Template::render("admin/login", &ctx));
    }

    if !auth::verify_password(&form.password, &stored_hash) {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), "Invalid credentials".to_string());
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
            Ok(Redirect::to("/admin"))
        }
        Err(_) => {
            let mut ctx = HashMap::new();
            ctx.insert("error".to_string(), "Session creation failed".to_string());
            Err(Template::render("admin/login", &ctx))
        }
    }
}

#[get("/admin/logout")]
pub fn logout(pool: &State<DbPool>, cookies: &CookieJar<'_>) -> Redirect {
    if let Some(cookie) = cookies.get_private("velocty_session") {
        let _ = auth::destroy_session(pool, cookie.value());
    }
    auth::clear_session_cookie(cookies);
    Redirect::to("/admin/login")
}

/// Catch-all for any /admin/* route that failed the AdminUser guard.
/// This fires when the guard returns Forward(Unauthorized).
#[get("/admin/<_path..>", rank = 99)]
pub fn admin_redirect_to_login(_path: std::path::PathBuf) -> Redirect {
    Redirect::to("/admin/login")
}

pub fn routes() -> Vec<rocket::Route> {
    routes![login_page, login_submit, logout, admin_redirect_to_login]
}
