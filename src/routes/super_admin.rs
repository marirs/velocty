#![cfg(feature = "multi-site")]

use rocket::form::Form;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::site::{self, RegistryPool, SitePoolManager};

const SUPER_COOKIE: &str = "velocty_super_session";

// ── Guards ───────────────────────────────────────────────────

fn is_super_authenticated(registry: &RegistryPool, cookies: &CookieJar<'_>) -> bool {
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
        Err(e) => {
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
    ctx.insert("sites".to_string(), serde_json::to_value(&sites).unwrap_or_default());
    Ok(Template::render("super/dashboard", &ctx))
}

// ── Health ───────────────────────────────────────────────────

#[get("/health")]
pub fn health_page(
    registry: &State<RegistryPool>,
    cookies: &CookieJar<'_>,
    pool: &State<crate::db::DbPool>,
) -> Result<Template, Redirect> {
    if !is_super_authenticated(registry, cookies) {
        return Err(Redirect::to("/super/login"));
    }

    let report = crate::health::gather(pool);
    let sites = site::list_sites(registry);
    let mut ctx = HashMap::new();
    ctx.insert("report".to_string(), serde_json::to_value(&report).unwrap_or_default());
    ctx.insert("sites".to_string(), serde_json::to_value(&sites).unwrap_or_default());
    Ok(Template::render("super/health", &ctx))
}

// ── Health Tools (per-site) ──────────────────────────────────

/// Resolve a site ID to its DbPool via the registry + pool manager.
fn get_site_pool(
    registry: &RegistryPool,
    pool_mgr: &SitePoolManager,
    site_id: i64,
) -> Result<crate::db::DbPool, String> {
    let site = site::find_site_by_id(registry, site_id)
        .ok_or_else(|| "Site not found".to_string())?;
    pool_mgr.get_pool(&site.slug)
}

fn json_tool(r: crate::health::ToolResult) -> Json<Value> {
    Json(json!({ "ok": r.ok, "message": r.message, "details": r.details }))
}

#[post("/health/tool/<site_id>/vacuum")]
pub fn tool_vacuum(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::run_vacuum(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/wal-checkpoint")]
pub fn tool_wal_checkpoint(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::run_wal_checkpoint(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/integrity-check")]
pub fn tool_integrity_check(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::run_integrity_check(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/session-cleanup")]
pub fn tool_session_cleanup(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::run_session_cleanup(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/orphan-scan")]
pub fn tool_orphan_scan(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    let site = match site::find_site_by_id(registry, site_id) {
        Some(s) => s,
        None => return Json(json!({ "ok": false, "message": "Site not found" })),
    };
    match pool_mgr.get_pool(&site.slug) {
        Ok(pool) => {
            let uploads_dir = format!("website/sites/{}/uploads", site.slug);
            json_tool(crate::health::run_orphan_scan(&pool, &uploads_dir))
        }
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/orphan-delete")]
pub fn tool_orphan_delete(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    let site = match site::find_site_by_id(registry, site_id) {
        Some(s) => s,
        None => return Json(json!({ "ok": false, "message": "Site not found" })),
    };
    match pool_mgr.get_pool(&site.slug) {
        Ok(pool) => {
            let uploads_dir = format!("website/sites/{}/uploads", site.slug);
            json_tool(crate::health::run_orphan_delete(&pool, &uploads_dir))
        }
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/unused-tags")]
pub fn tool_unused_tags(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::run_unused_tags_cleanup(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
}

#[post("/health/tool/<site_id>/export-content")]
pub fn tool_export_content(
    site_id: i64,
    registry: &State<RegistryPool>,
    pool_mgr: &State<SitePoolManager>,
    cookies: &CookieJar<'_>,
) -> Json<Value> {
    if !is_super_authenticated(registry, cookies) {
        return Json(json!({ "ok": false, "message": "Unauthorized" }));
    }
    match get_site_pool(registry, pool_mgr, site_id) {
        Ok(pool) => json_tool(crate::health::export_content(&pool)),
        Err(e) => Json(json!({ "ok": false, "message": e })),
    }
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
        Err(e) => {
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

// ── Route collection ─────────────────────────────────────────

pub fn routes() -> Vec<rocket::Route> {
    routes![
        setup_page,
        setup_submit,
        login_page,
        login_submit,
        logout,
        dashboard,
        health_page,
        tool_vacuum,
        tool_wal_checkpoint,
        tool_integrity_check,
        tool_session_cleanup,
        tool_orphan_scan,
        tool_orphan_delete,
        tool_unused_tags,
        tool_export_content,
        settings_page,
        new_site_page,
        new_site_submit,
        edit_site_page,
        edit_site_submit,
        delete_site,
    ]
}
