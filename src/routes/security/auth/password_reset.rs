use rocket::form::Form;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::models::user::User;
use crate::rate_limit::RateLimiter;
use crate::security::{auth, password_reset};
use crate::AdminSlug;

#[derive(Debug, FromForm, Deserialize)]
pub struct ForgotForm {
    pub email: String,
}

#[derive(Debug, FromForm, Deserialize)]
pub struct ResetForm {
    pub token: String,
    pub password: String,
    pub confirm_password: String,
}

/// GET /forgot-password — show the forgot password form
#[get("/forgot-password")]
pub fn forgot_password_page(pool: &State<DbPool>, admin_slug: &State<AdminSlug>) -> Template {
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        Setting::get_or(pool, "admin_theme", "dark"),
    );
    ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
    Template::render("admin/forgot_password", &ctx)
}

/// POST /forgot-password — send reset email
#[post("/forgot-password", data = "<form>")]
pub fn forgot_password_submit(
    form: Form<ForgotForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
    limiter: &State<RateLimiter>,
    client_ip: auth::ClientIp,
) -> Template {
    let theme = Setting::get_or(pool, "admin_theme", "dark");
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert("admin_theme".to_string(), theme);
    ctx.insert("admin_slug".to_string(), admin_slug.0.clone());

    // Rate limit: 3 requests per 15 minutes per IP
    let rate_key = format!("pw_reset:{}", client_ip.0);
    if !limiter.check_and_record(&rate_key, 3, std::time::Duration::from_secs(15 * 60)) {
        ctx.insert(
            "error".to_string(),
            "Too many requests. Please try again in 15 minutes.".to_string(),
        );
        return Template::render("admin/forgot_password", &ctx);
    }

    // Always show success to prevent email enumeration
    ctx.insert(
        "success".to_string(),
        "If that email is registered, a password reset link has been sent. Check your inbox."
            .to_string(),
    );

    // Only actually send if the email matches a known user
    if let Some(user) = User::get_by_email(pool, form.email.trim()) {
        if user.is_active() && user.role != "subscriber" {
            match password_reset::create_token(pool, &user.email) {
                Ok(token) => {
                    if let Err(e) = password_reset::send_reset_email(pool, &user.email, &token) {
                        log::error!("Failed to send password reset email: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to create password reset token: {}", e);
                }
            }
        }
    }

    Template::render("admin/forgot_password", &ctx)
}

/// GET /reset-password?token=xxx — show the new password form
#[get("/reset-password?<token>")]
pub fn reset_password_page(
    token: &str,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
) -> Template {
    let mut ctx: HashMap<String, String> = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        Setting::get_or(pool, "admin_theme", "dark"),
    );
    ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
    ctx.insert("token".to_string(), token.to_string());
    Template::render("admin/reset_password", &ctx)
}

/// POST /reset-password — set the new password
#[post("/reset-password", data = "<form>")]
pub fn reset_password_submit(
    form: Form<ResetForm>,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
) -> Result<Redirect, Template> {
    let theme = Setting::get_or(pool, "admin_theme", "dark");

    let make_err = |msg: &str, token: &str| -> Template {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), msg.to_string());
        ctx.insert("admin_theme".to_string(), theme.clone());
        ctx.insert("admin_slug".to_string(), admin_slug.0.clone());
        ctx.insert("token".to_string(), token.to_string());
        Template::render("admin/reset_password", &ctx)
    };

    if form.password.len() < 8 {
        return Err(make_err(
            "Password must be at least 8 characters.",
            &form.token,
        ));
    }
    if form.password != form.confirm_password {
        return Err(make_err("Passwords do not match.", &form.token));
    }

    // Verify token
    let email = match password_reset::verify_token(pool, &form.token) {
        Ok(e) => e,
        Err(e) => return Err(make_err(&e, &form.token)),
    };

    // Find user
    let user = match User::get_by_email(pool, &email) {
        Some(u) => u,
        None => return Err(make_err("User not found.", &form.token)),
    };

    // Hash and update password
    let hash = match auth::hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => return Err(make_err(&e, &form.token)),
    };

    if let Err(e) = User::update_password(pool, user.id, &hash) {
        return Err(make_err(&e, &form.token));
    }

    // Redirect to login with a flash-like param
    Ok(Redirect::to(format!(
        "/{}/login?reset=success",
        admin_slug.0
    )))
}
