#![cfg(feature = "multi-site")]

use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use std::collections::HashMap;

use super::auth::is_super_authenticated;
use crate::site::{self, RegistryPool};

// ── Dashboard ────────────────────────────────────────────────

#[get("/")]
pub fn dashboard(
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Template, Redirect> {
    if !is_super_authenticated(registry, cookies) {
        return Err(Redirect::to("/super/login"));
    }

    let sites = site::list_sites(registry);
    let mut ctx = HashMap::new();
    ctx.insert(
        "sites".to_string(),
        serde_json::to_value(&sites).unwrap_or_default(),
    );
    Ok(Template::render("super/dashboard", &ctx))
}

// ── Settings ────────────────────────────────────────────────

#[get("/settings")]
pub fn settings_page(
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Template, Redirect> {
    if !is_super_authenticated(registry, cookies) {
        return Err(Redirect::to("/super/login"));
    }

    let ctx: HashMap<String, String> = HashMap::new();
    Ok(Template::render("super/settings", &ctx))
}
