use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use std::collections::HashMap;

use std::sync::Arc;

use crate::rate_limit::RateLimiter;
use crate::security::{self, auth, mfa};
use crate::store::Store;
use crate::AdminSlug;

#[derive(Debug, FromForm, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub captcha_token: Option<String>,
}

/// Returns true if this is a fresh install (no users exist)
pub fn needs_setup(store: &dyn Store) -> bool {
    store.user_count() == 0
}

#[get("/login?<reset>")]
pub fn login_page(
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    reset: Option<&str>,
) -> Result<Template, Redirect> {
    let s: &dyn Store = &**store.inner();
    if needs_setup(s) {
        return Err(Redirect::to(format!("/{}/setup", admin_slug.get())));
    }
    let login_method = s.setting_get_or("login_method", "password");
    if login_method == "magic_link" {
        return Err(Redirect::to(format!("/{}/magic-link", admin_slug.get())));
    }
    let mut context: HashMap<String, String> = HashMap::new();
    context.insert(
        "admin_theme".to_string(),
        s.setting_get_or("admin_theme", "dark"),
    );
    context.insert("admin_slug".to_string(), admin_slug.get().clone());
    if reset == Some("success") {
        context.insert("reset_success".to_string(), "true".to_string());
    }
    inject_captcha_context(s, &mut context);
    Ok(Template::render("admin/login", &context))
}

#[post("/login", data = "<form>")]
pub fn login_submit(
    form: Form<LoginForm>,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    limiter: &State<RateLimiter>,
    cookies: &CookieJar<'_>,
    client_ip: auth::ClientIp,
) -> Result<Redirect, Template> {
    let s: &dyn Store = &**store.inner();
    let theme = s.setting_get_or("admin_theme", "dark");
    let ip = &client_ip.0;
    let rate_key = format!("login:{}", ip);
    let max_attempts = s.setting_get_i64("login_rate_limit").max(1) as u64;
    let window = std::time::Duration::from_secs(15 * 60);

    let make_err = |msg: &str, theme: &str, st: &dyn Store, slug: &str| -> Template {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), msg.to_string());
        ctx.insert("admin_theme".to_string(), theme.to_string());
        ctx.insert("admin_slug".to_string(), slug.to_string());
        inject_captcha_context(st, &mut ctx);
        Template::render("admin/login", &ctx)
    };

    // Check rate limit before processing
    if !limiter.check_and_record(&rate_key, max_attempts, window) {
        return Err(make_err(
            "Too many login attempts. Please try again in 15 minutes.",
            &theme,
            s,
            &admin_slug.get(),
        ));
    }

    // Verify login captcha
    let captcha_token = form.captcha_token.as_deref().unwrap_or("");
    match security::verify_login_captcha(s, captcha_token, None) {
        Ok(false) => {
            return Err(make_err(
                "Captcha verification failed. Please try again.",
                &theme,
                s,
                &admin_slug.get(),
            ));
        }
        Err(e) => log::warn!("Login captcha error (allowing): {}", e),
        _ => {}
    }

    // Look up user by email
    let user = match s.user_get_by_email(&form.email) {
        Some(u) => u,
        None => {
            // Firewall: unknown user
            if s.setting_get_or("firewall_enabled", "false") == "true"
                && s.setting_get_or("fw_failed_login_tracking", "true") == "true"
            {
                s.fw_event_log(
                    ip,
                    "failed_login",
                    Some(&format!("Unknown user: {}", form.email)),
                    None,
                    None,
                    Some("login"),
                );
                if s.setting_get_or("fw_ban_unknown_users", "false") == "true" {
                    let dur = s.setting_get_or("fw_unknown_user_ban_duration", "24h");
                    let _ = s.fw_ban_create_with_duration(
                        ip,
                        "unknown_user",
                        Some(&format!("Login attempt with unknown user: {}", form.email)),
                        &dur,
                        None,
                        None,
                    );
                }
            }
            return Err(make_err(
                "Invalid credentials",
                &theme,
                s,
                &admin_slug.get(),
            ));
        }
    };

    // Check account status
    if !user.is_active() {
        return Err(make_err(
            "This account is suspended or locked. Contact an administrator.",
            &theme,
            s,
            &admin_slug.get(),
        ));
    }

    // Check role — subscribers cannot log into admin
    if user.role == "subscriber" {
        return Err(make_err(
            "Your account does not have admin panel access.",
            &theme,
            s,
            &admin_slug.get(),
        ));
    }

    // Verify password
    if !auth::verify_password(&form.password, &user.password_hash) {
        // Firewall: failed password
        if s.setting_get_or("firewall_enabled", "false") == "true"
            && s.setting_get_or("fw_failed_login_tracking", "true") == "true"
        {
            s.fw_event_log(
                ip,
                "failed_login",
                Some("Wrong password"),
                None,
                None,
                Some("login"),
            );
            let threshold: i64 = s
                .setting_get_or("fw_failed_login_ban_threshold", "5")
                .parse()
                .unwrap_or(5);
            let count = s.fw_event_count_for_ip_since(ip, "failed_login", 15);
            if count >= threshold {
                let dur = s.setting_get_or("fw_failed_login_ban_duration", "1h");
                let _ = s.fw_ban_create_with_duration(
                    ip,
                    "failed_login",
                    Some("Too many failed login attempts"),
                    &dur,
                    None,
                    None,
                );
            }
        }
        s.audit_log(
            Some(user.id),
            Some(&user.display_name),
            "login_failed",
            Some("user"),
            Some(user.id),
            Some(&user.email),
            Some("Wrong password"),
            Some(ip),
        );
        return Err(make_err(
            "Invalid credentials",
            &theme,
            s,
            &admin_slug.get(),
        ));
    }

    // Check MFA (per-user)
    if user.mfa_enabled && !user.mfa_secret.is_empty() {
        let pending_token = uuid::Uuid::new_v4().to_string();
        // Store user_id in a pending cookie so MFA page can complete login
        mfa::set_pending_cookie(cookies, &format!("{}:{}", user.id, pending_token));
        return Ok(Redirect::to(format!("/{}/mfa", admin_slug.get())));
    }

    // Create session
    let _ = s.user_touch_last_login(user.id);
    match auth::create_session(s, user.id, None, None) {
        Ok(session_id) => {
            auth::set_session_cookie(cookies, &session_id);
            s.audit_log(
                Some(user.id),
                Some(&user.display_name),
                "login",
                Some("user"),
                Some(user.id),
                Some(&user.email),
                None,
                Some(ip),
            );
            // Force password change on first login (e.g. multi-site temp password)
            if user.force_password_change {
                Ok(Redirect::to(format!(
                    "/{}/change-password",
                    admin_slug.get()
                )))
            } else {
                Ok(Redirect::to(format!("/{}", admin_slug.get())))
            }
        }
        Err(_) => Err(make_err(
            "Session creation failed",
            &theme,
            s,
            &admin_slug.get(),
        )),
    }
}

/// Inject captcha provider/site_key/version into template context if login captcha is enabled.
pub fn inject_captcha_context(store: &dyn Store, ctx: &mut HashMap<String, String>) {
    if let Some(info) = security::login_captcha_info(store) {
        ctx.insert("captcha_provider".to_string(), info.provider);
        ctx.insert("captcha_site_key".to_string(), info.site_key);
        ctx.insert("captcha_version".to_string(), info.version);
    }
}
