#![cfg(feature = "multi-site")]

use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use std::collections::HashMap;

use super::auth::is_super_authenticated;
use crate::site::{self, RegistryPool};

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
    pub admin_email: String,
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

    if form.hostname.trim().is_empty()
        || form.display_name.trim().is_empty()
        || form.admin_email.trim().is_empty()
    {
        let mut ctx = HashMap::new();
        ctx.insert("error", "All fields are required.");
        return Err(Template::render("super/site_new", &ctx));
    }

    if !form.admin_email.contains('@') {
        let mut ctx = HashMap::new();
        ctx.insert("error", "Please enter a valid email address.");
        return Err(Template::render("super/site_new", &ctx));
    }

    // Use empty settings — email will fail gracefully if no provider is configured.
    // The temp password is shown in the UI as a fallback.
    let email_settings = HashMap::new();

    match site::create_site(
        registry,
        form.hostname.trim(),
        form.display_name.trim(),
        form.admin_email.trim(),
        &email_settings,
    ) {
        Ok((_site, temp_password, email_sent)) => {
            if email_sent {
                Ok(Redirect::to("/super/"))
            } else {
                // Email failed — show the temp password so the super admin can share it manually
                let mut ctx = HashMap::new();
                ctx.insert("success".to_string(), format!(
                    "Site created. Email delivery failed — please share these credentials manually: Email: {} / Temporary password: {}",
                    form.admin_email.trim(),
                    temp_password,
                ));
                ctx.insert(
                    "sites".to_string(),
                    serde_json::to_value(site::list_sites(registry))
                        .unwrap_or_default()
                        .to_string(),
                );
                Err(Template::render("super/dashboard", &ctx))
            }
        }
        Err(_e) => {
            let mut ctx = HashMap::new();
            ctx.insert(
                "error".to_string(),
                "Failed to create site. Hostname may already exist.".to_string(),
            );
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
    ctx.insert(
        "site".to_string(),
        serde_json::to_value(&site).unwrap_or_default(),
    );
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
pub fn delete_site(id: i64, registry: &State<RegistryPool>, cookies: &CookieJar<'_>) -> Redirect {
    if !is_super_authenticated(registry, cookies) {
        return Redirect::to("/super/login");
    }

    let _ = site::delete_site(registry, id);
    Redirect::to("/super/")
}
