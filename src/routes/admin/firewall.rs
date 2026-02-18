use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::settings::Setting;
use crate::security::auth::AdminUser;
use crate::AdminSlug;

// ── Firewall Dashboard ─────────────────────────────────

#[get("/firewall?<ev_page>&<ban_page>&<audit_page>&<audit_action>&<audit_entity>&<audit_user>")]
pub fn firewall_dashboard(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    ev_page: Option<i64>,
    ban_page: Option<i64>,
    audit_page: Option<i64>,
    audit_action: Option<String>,
    audit_entity: Option<String>,
    audit_user: Option<i64>,
) -> Template {
    use crate::models::firewall::{FwBan, FwEvent};

    let per_page: i64 = 25;

    // Events pagination
    let ev_current = ev_page.unwrap_or(1).max(1);
    let ev_offset = (ev_current - 1) * per_page;
    let ev_total = FwEvent::count_all(pool, None);
    let ev_total_pages = ((ev_total as f64) / (per_page as f64)).ceil() as i64;

    // Bans pagination
    let ban_current = ban_page.unwrap_or(1).max(1);
    let ban_offset = (ban_current - 1) * per_page;
    let ban_total = FwBan::active_count(pool);
    let ban_total_pages = ((ban_total as f64) / (per_page as f64)).ceil() as i64;

    // Audit log pagination
    let audit_per_page: i64 = 50;
    let audit_current = audit_page.unwrap_or(1).max(1);
    let audit_offset = (audit_current - 1) * audit_per_page;
    let audit_entries = AuditEntry::list(
        pool,
        audit_action.as_deref(),
        audit_entity.as_deref(),
        audit_user,
        audit_per_page,
        audit_offset,
    );
    let audit_total = AuditEntry::count(
        pool,
        audit_action.as_deref(),
        audit_entity.as_deref(),
        audit_user,
    );
    let audit_total_pages = (audit_total as f64 / audit_per_page as f64).ceil() as i64;
    let audit_actions = AuditEntry::distinct_actions(pool);
    let audit_entity_types = AuditEntry::distinct_entity_types(pool);

    let settings = Setting::all(pool);
    let events_24h = FwEvent::count_since_hours(pool, 24);
    let events_1h = FwEvent::count_since_hours(pool, 1);
    let top_ips = FwEvent::top_ips(pool, 10);
    let event_counts = FwEvent::counts_by_type(pool);
    let events = FwEvent::recent(pool, None, per_page, ev_offset);
    let bans = FwBan::active_bans(pool, per_page, ban_offset);

    let context = json!({
        "page_title": "Firewall",
        "admin_slug": slug.0,
        "settings": settings,
        "active_bans": ban_total,
        "events_24h": events_24h,
        "events_1h": events_1h,
        "top_ips": top_ips,
        "event_counts": event_counts,
        "events": events,
        "bans": bans,
        "ev_current_page": ev_current,
        "ev_total_pages": ev_total_pages,
        "ev_total": ev_total,
        "ban_current_page": ban_current,
        "ban_total_pages": ban_total_pages,
        "ban_total": ban_total,
        "audit_entries": audit_entries,
        "audit_total": audit_total,
        "audit_current_page": audit_current,
        "audit_total_pages": audit_total_pages,
        "audit_action_filter": audit_action,
        "audit_entity_filter": audit_entity,
        "audit_user_filter": audit_user,
        "audit_actions": audit_actions,
        "audit_entity_types": audit_entity_types,
    });
    Template::render("admin/firewall", &context)
}

#[derive(Deserialize)]
pub struct BanForm {
    pub ip: String,
    pub reason: Option<String>,
    pub duration: Option<String>,
}

#[post("/api/firewall/ban", format = "json", data = "<form>")]
pub fn firewall_ban(_admin: AdminUser, pool: &State<DbPool>, form: Json<BanForm>) -> Json<Value> {
    use crate::models::firewall::FwBan;

    let ip = form.ip.trim();
    if ip.is_empty() {
        return Json(json!({"success": false, "error": "IP is required"}));
    }
    let reason = form.reason.as_deref().unwrap_or("manual");
    let duration = form.duration.as_deref().unwrap_or("7d");

    match FwBan::create_with_duration(
        pool,
        ip,
        reason,
        Some("Manual ban from admin"),
        duration,
        None,
        None,
    ) {
        Ok(id) => Json(json!({"success": true, "id": id})),
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[derive(Deserialize)]
pub struct UnbanForm {
    pub id: i64,
}

#[post("/api/firewall/unban", format = "json", data = "<form>")]
pub fn firewall_unban(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Json<UnbanForm>,
) -> Json<Value> {
    use crate::models::firewall::FwBan;

    match FwBan::unban_by_id(pool, form.id) {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}
