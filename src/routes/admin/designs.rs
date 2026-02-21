use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::category::Category;
use crate::models::design::Design;
use crate::models::settings::Setting;
use crate::security::auth::AdminUser;
use crate::AdminSlug;

// ── Design List ──────────────────────────────────────────

#[get("/designer")]
pub fn designs_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Template {
    let designs = Design::list(pool);

    let mut context = json!({
        "page_title": "Designer",
        "designs": designs,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    if let Some(ref f) = flash {
        context["flash_kind"] = json!(f.kind());
        context["flash_msg"] = json!(f.message());
    }

    Template::render("admin/designs/list", &context)
}

// ── Activate ─────────────────────────────────────────────

#[post("/designer/<id>/activate")]
pub fn design_activate(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Flash<Redirect> {
    let name = Design::find_by_id(pool, id)
        .map(|d| d.name.clone())
        .unwrap_or_default();
    let _ = Design::activate(pool, id);
    AuditEntry::log(
        pool,
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "activate",
        Some("design"),
        Some(id),
        Some(&name),
        None,
        None,
    );
    Flash::success(
        Redirect::to(format!("{}/designer", admin_base(slug))),
        format!("{} is activated", name),
    )
}

// ── Design Overview (live preview) ───────────────────────

#[get("/designer/<design_slug>")]
pub fn design_overview(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    design_slug: String,
) -> Option<Template> {
    let design = Design::find_by_slug(pool, &design_slug)?;
    let portfolio_categories: Vec<serde_json::Value> = Category::list(pool, Some("portfolio"))
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "slug": c.slug, "show_in_nav": c.show_in_nav}))
        .collect();
    let journal_categories: Vec<serde_json::Value> = Category::list(pool, Some("post"))
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "slug": c.slug, "show_in_nav": c.show_in_nav}))
        .collect();

    let context = json!({
        "page_title": format!("Design: {}", design.name),
        "design": design,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
        "portfolio_categories": portfolio_categories,
        "journal_categories": journal_categories,
    });

    Some(Template::render("admin/designs/overview", &context))
}
