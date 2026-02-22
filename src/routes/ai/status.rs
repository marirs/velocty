use rocket::serde::json::Json;
use rocket::State;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::ai;
use crate::security::auth::EditorUser;
use crate::store::Store;

// ── Status Check ──────────────────────────────────────

#[get("/ai/status")]
pub fn ai_status(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let enabled = ai::is_enabled(&**store.inner());
    let flags = ai::suggestion_flags(&**store.inner());
    Json(json!({
        "enabled": enabled,
        "features": flags,
    }))
}
