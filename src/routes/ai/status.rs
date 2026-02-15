use rocket::serde::json::Json;
use rocket::State;
use serde_json::{json, Value};

use crate::ai;
use crate::security::auth::AdminUser;
use crate::db::DbPool;

// ── Status Check ──────────────────────────────────────

#[get("/ai/status")]
pub fn ai_status(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let enabled = ai::is_enabled(pool);
    let flags = ai::suggestion_flags(pool);
    Json(json!({
        "enabled": enabled,
        "features": flags,
    }))
}
