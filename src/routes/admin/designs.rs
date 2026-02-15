use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AdminUser;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::design::Design;
use crate::models::settings::Setting;
use crate::AdminSlug;
use super::admin_base;

// ── Designs ────────────────────────────────────────────

#[get("/designer")]
pub fn designs_list(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let designs = Design::list(pool);

    let context = json!({
        "page_title": "Designer",
        "designs": designs,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/designs/list", &context)
}

#[post("/designer/<id>/activate")]
pub fn design_activate(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let name = Design::find_by_id(pool, id).map(|d| d.name).unwrap_or_default();
    let _ = Design::activate(pool, id);
    AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "activate", Some("design"), Some(id), Some(&name), None, None);
    Redirect::to(format!("{}/designer", admin_base(slug)))
}
