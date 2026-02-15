#![cfg(feature = "multi-site")]

use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::site::{self, RegistryPool, SitePoolManager};
use super::auth::is_super_authenticated;

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
