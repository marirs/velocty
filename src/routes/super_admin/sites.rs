#![cfg(feature = "multi-site")]

use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use std::collections::HashMap;

use crate::site::{self, RegistryPool};
use super::auth::is_super_authenticated;

// ── Create Site ──────────────────────────────────────────────

#[get("/sites/new")]
pub fn new_site_page(
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Template, Redirect> {
    if !is_super_authenticated(registry, cookies) {
        return Err(Redirect::to("/super/login"));
    }
    let ctx: HashMap<String, String> = HashMap::new();
    Ok(Template::render("super/site_new", &ctx))
}

#[derive(Debug, FromForm)]
pub struct NewSiteForm {
    pub hostname: String,
    pub display_name: String,
}

#[post("/sites/new", data = "<form>")]
pub fn new_site_submit(
    form: Form<NewSiteForm>,
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Redirect, Template> {
    if !is_super_authenticated(registry, cookies) {
        return Ok(Redirect::to("/super/login"));
    }

    if form.hostname.trim().is_empty() || form.display_name.trim().is_empty() {
        let mut ctx = HashMap::new();
        ctx.insert("error", "All fields are required.");
        return Err(Template::render("super/site_new", &ctx));
    }

    match site::create_site(registry, form.hostname.trim(), form.display_name.trim()) {
        Ok(_site) => Ok(Redirect::to("/super/")),
        Err(_e) => {
            let mut ctx = HashMap::new();
            ctx.insert("error", "Failed to create site. Hostname may already exist.");
            Err(Template::render("super/site_new", &ctx))
        }
    }
}

// ── Edit Site ────────────────────────────────────────────────

#[get("/sites/<id>")]
pub fn edit_site_page(
    id: i64,
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Result<Template, Redirect> {
    if !is_super_authenticated(registry, cookies) {
        return Err(Redirect::to("/super/login"));
    }

    let site = match site::find_site_by_id(registry, id) {
        Some(s) => s,
        None => return Err(Redirect::to("/super/")),
    };

    let mut ctx = HashMap::new();
    ctx.insert("site".to_string(), serde_json::to_value(&site).unwrap_or_default());
    Ok(Template::render("super/site_edit", &ctx))
}

#[derive(Debug, FromForm)]
pub struct EditSiteForm {
    pub status: String,
}

#[post("/sites/<id>", data = "<form>")]
pub fn edit_site_submit(
    id: i64,
    form: Form<EditSiteForm>,
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Redirect {
    if !is_super_authenticated(registry, cookies) {
        return Redirect::to("/super/login");
    }

    let _ = site::update_site_status(registry, id, &form.status);
    Redirect::to("/super/")
}

// ── Delete Site ──────────────────────────────────────────────

#[post("/sites/<id>/delete")]
pub fn delete_site(
    id: i64,
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
) -> Redirect {
    if !is_super_authenticated(registry, cookies) {
        return Redirect::to("/super/login");
    }

    let _ = site::delete_site(registry, id);
    Redirect::to("/super/")
}
