#![cfg(feature = "multi-site")]

use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use std::collections::HashMap;

use crate::site::{self, RegistryPool};

pub(crate) const SUPER_COOKIE: &str = "velocty_super_session";

pub(crate) fn is_super_authenticated(registry: &RegistryPool, cookies: &CookieJar<'_>) -> bool {
    cookies
        .get_private(SUPER_COOKIE)
        .map(|c| site::validate_super_session(registry, c.value()))
        .unwrap_or(false)
}

// ── Setup ────────────────────────────────────────────────────

#[get("/setup")]
pub fn setup_page(registry: &State<RegistryPool>) -> Result<Template, Redirect> {
    if site::super_admin_exists(registry) {
        return Err(Redirect::to("/super/login"));
    }
    let ctx: HashMap<String, String> = HashMap::new();
    Ok(Template::render("super/setup", &ctx))
}

#[derive(Debug, FromForm)]
pub struct SuperSetupForm {
    pub email: String,
    pub password: String,
    pub confirm_password: String,
}

#[post("/setup", data = "<form>")]
pub fn setup_submit(
    form: Form<SuperSetupForm>,
    registry: &State<RegistryPool>,
) -> Result<Redirect, Template> {
    if site::super_admin_exists(registry) {
        return Ok(Redirect::to("/super/login"));
    }

    if form.email.trim().is_empty() {
        let mut ctx = HashMap::new();
        ctx.insert("error", "Email is required.");
        return Err(Template::render("super/setup", &ctx));
    }
    if form.password.len() < 8 {
        let mut ctx = HashMap::new();
        ctx.insert("error", "Password must be at least 8 characters.");
        return Err(Template::render("super/setup", &ctx));
    }
    if form.password != form.confirm_password {
        let mut ctx = HashMap::new();
        ctx.insert("error", "Passwords do not match.");
        return Err(Template::render("super/setup", &ctx));
    }

    match site::create_super_admin(registry, form.email.trim(), &form.password) {
        Ok(_) => Ok(Redirect::to("/super/login")),
        Err(_e) => {
            let mut ctx = HashMap::new();
            ctx.insert("error", "Failed to create account.");
            Err(Template::render("super/setup", &ctx))
        }
    }
}

// ── Login ────────────────────────────────────────────────────

#[get("/login")]
pub fn login_page(registry: &State<RegistryPool>) -> Result<Template, Redirect> {
    if !site::super_admin_exists(registry) {
        return Err(Redirect::to("/super/setup"));
    }
    let ctx: HashMap<String, String> = HashMap::new();
    Ok(Template::render("super/login", &ctx))
}

#[derive(Debug, FromForm)]
pub struct SuperLoginForm {
    pub email: String,
    pub password: String,
}

#[post("/login", data = "<form>")]
pub fn login_submit(
    form: Form<SuperLoginForm>,
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    if site::verify_super_admin(registry, &form.email, &form.password) {
        match site::create_super_session(registry, &form.email) {
            Ok(token) => {
                let mut cookie = rocket::http::Cookie::new(SUPER_COOKIE, token);
                cookie.set_http_only(true);
                cookie.set_same_site(rocket::http::SameSite::Strict);
                cookie.set_path("/super");
                cookies.add_private(cookie);
                Ok(Redirect::to("/super/"))
            }
            Err(_) => {
                let mut ctx = HashMap::new();
                ctx.insert("error", "Session creation failed.");
                Err(Template::render("super/login", &ctx))
            }
        }
    } else {
        let mut ctx = HashMap::new();
        ctx.insert("error", "Invalid credentials.");
        Err(Template::render("super/login", &ctx))
    }
}

#[get("/logout")]
pub fn logout(registry: &State<RegistryPool>, cookies: &CookieJar<'_>) -> Redirect {
    if let Some(cookie) = cookies.get_private(SUPER_COOKIE) {
        let _ = site::destroy_super_session(registry, cookie.value());
    }
    cookies.remove_private(rocket::http::Cookie::from(SUPER_COOKIE));
    Redirect::to("/super/login")
}
