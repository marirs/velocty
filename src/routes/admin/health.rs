use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Health ─────────────────────────────────────────────────

#[get("/health")]
pub fn health_page(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
) -> Template {
    let s: &dyn Store = &**store.inner();
    let report = crate::health::gather(None, s);
    let context = json!({
        "page_title": "Health",
        "admin_slug": slug.0,
        "settings": store.setting_all(),
        "report": report,
        "db_backend": s.db_backend(),
    });
    Template::render("admin/health", &context)
}

#[post("/health/vacuum")]
pub fn health_vacuum(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    if s.db_backend() != "sqlite" {
        return json_tool_result(crate::health::ToolResult {
            ok: false,
            message: "VACUUM is only available for SQLite backend".to_string(),
            details: None,
        });
    }
    match s.raw_execute("VACUUM") {
        Ok(_) => json_tool_result(crate::health::ToolResult {
            ok: true,
            message: "VACUUM completed successfully".to_string(),
            details: None,
        }),
        Err(e) => json_tool_result(crate::health::ToolResult {
            ok: false,
            message: format!("VACUUM failed: {}", e),
            details: None,
        }),
    }
}

#[post("/health/wal-checkpoint")]
pub fn health_wal_checkpoint(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    if s.db_backend() != "sqlite" {
        return json_tool_result(crate::health::ToolResult {
            ok: false,
            message: "WAL checkpoint is only available for SQLite backend".to_string(),
            details: None,
        });
    }
    match s.raw_execute("PRAGMA wal_checkpoint(TRUNCATE)") {
        Ok(_) => json_tool_result(crate::health::ToolResult {
            ok: true,
            message: "WAL checkpoint completed successfully".to_string(),
            details: None,
        }),
        Err(e) => json_tool_result(crate::health::ToolResult {
            ok: false,
            message: format!("WAL checkpoint failed: {}", e),
            details: None,
        }),
    }
}

#[post("/health/integrity-check")]
pub fn health_integrity_check(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    if s.db_backend() != "sqlite" {
        return json_tool_result(crate::health::ToolResult {
            ok: false,
            message: "Integrity check is only available for SQLite backend".to_string(),
            details: None,
        });
    }
    let r = s.raw_query_i64("SELECT CASE WHEN (SELECT integrity_check FROM pragma_integrity_check() LIMIT 1) = 'ok' THEN 1 ELSE 0 END");
    match r {
        Ok(1) => json_tool_result(crate::health::ToolResult {
            ok: true,
            message: "Integrity check passed".to_string(),
            details: None,
        }),
        Ok(_) => json_tool_result(crate::health::ToolResult {
            ok: false,
            message: "Integrity check found issues".to_string(),
            details: None,
        }),
        Err(e) => json_tool_result(crate::health::ToolResult {
            ok: false,
            message: format!("Integrity check failed: {}", e),
            details: None,
        }),
    }
}

#[post("/health/session-cleanup")]
pub fn health_session_cleanup(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::run_session_cleanup(s);
    json_tool_result(r)
}

#[post("/health/orphan-scan")]
pub fn health_orphan_scan(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::run_orphan_scan(s, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/orphan-delete")]
pub fn health_orphan_delete(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::run_orphan_delete(s, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/unused-tags")]
pub fn health_unused_tags(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::run_unused_tags_cleanup(s);
    json_tool_result(r)
}

#[derive(Debug, Deserialize)]
pub struct AnalyticsPruneForm {
    pub days: u64,
}

#[post("/health/analytics-prune", format = "json", data = "<body>")]
pub fn health_analytics_prune(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<AnalyticsPruneForm>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::run_analytics_prune(s, body.days);
    json_tool_result(r)
}

#[post("/health/export-db")]
pub fn health_export_db(_admin: AdminUser) -> Json<Value> {
    let r = crate::health::export_database();
    json_tool_result(r)
}

#[post("/health/export-content")]
pub fn health_export_content(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let r = crate::health::export_content(s);
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
