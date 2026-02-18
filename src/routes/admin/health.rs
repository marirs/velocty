use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::security::auth::AdminUser;
use crate::AdminSlug;

// ── Health ─────────────────────────────────────────────────

#[get("/health")]
pub fn health_page(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let report = crate::health::gather(pool);
    let context = json!({
        "page_title": "Health",
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
        "report": report,
    });
    Template::render("admin/health", &context)
}

#[post("/health/vacuum")]
pub fn health_vacuum(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_vacuum(pool);
    json_tool_result(r)
}

#[post("/health/wal-checkpoint")]
pub fn health_wal_checkpoint(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_wal_checkpoint(pool);
    json_tool_result(r)
}

#[post("/health/integrity-check")]
pub fn health_integrity_check(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_integrity_check(pool);
    json_tool_result(r)
}

#[post("/health/session-cleanup")]
pub fn health_session_cleanup(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_session_cleanup(pool);
    json_tool_result(r)
}

#[post("/health/orphan-scan")]
pub fn health_orphan_scan(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_orphan_scan(pool, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/orphan-delete")]
pub fn health_orphan_delete(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_orphan_delete(pool, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/unused-tags")]
pub fn health_unused_tags(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_unused_tags_cleanup(pool);
    json_tool_result(r)
}

#[derive(Debug, Deserialize)]
pub struct AnalyticsPruneForm {
    pub days: u64,
}

#[post("/health/analytics-prune", format = "json", data = "<body>")]
pub fn health_analytics_prune(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<AnalyticsPruneForm>,
) -> Json<Value> {
    let r = crate::health::run_analytics_prune(pool, body.days);
    json_tool_result(r)
}

#[post("/health/export-db")]
pub fn health_export_db(_admin: AdminUser) -> Json<Value> {
    let r = crate::health::export_database();
    json_tool_result(r)
}

#[post("/health/export-content")]
pub fn health_export_content(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::export_content(pool);
    json_tool_result(r)
}

#[post("/health/mongo-ping")]
pub fn health_mongo_ping(_admin: AdminUser) -> Json<Value> {
    let uri = crate::health::read_db_backend();
    if uri != "mongodb" {
        return Json(
            json!({ "ok": false, "message": "Not using MongoDB backend.", "details": null }),
        );
    }
    let mongo_uri = std::fs::read_to_string("velocty.toml")
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|v| {
            v.get("database")?
                .get("uri")?
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "mongodb://localhost:27017".to_string());

    let start = std::time::Instant::now();
    let report = crate::health::gather_mongo_ping(&mongo_uri);
    let latency = start.elapsed().as_millis();

    if report.0 {
        Json(
            json!({ "ok": true, "message": format!("MongoDB is reachable. Latency: {} ms", report.1), "details": null }),
        )
    } else {
        Json(
            json!({ "ok": false, "message": format!("MongoDB unreachable ({}ms timeout)", latency), "details": null }),
        )
    }
}

fn json_tool_result(r: crate::health::ToolResult) -> Json<Value> {
    Json(json!({
        "ok": r.ok,
        "message": r.message,
        "details": r.details,
    }))
}
