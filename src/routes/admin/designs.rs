use std::sync::Arc;

use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Design List ──────────────────────────────────────────

#[get("/designer")]
pub fn designs_list(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Template {
    let designs = store.design_list();

    let mut context = json!({
        "page_title": "Designer",
        "designs": designs,
        "admin_slug": slug.0,
        "settings": store.setting_all(),
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
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Flash<Redirect> {
    let name = store
        .design_find_by_id(id)
        .map(|d| d.name.clone())
        .unwrap_or_default();
    let _ = store.design_activate(id);
    store.audit_log(
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
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    design_slug: String,
) -> Option<Template> {
    let design = store.design_find_by_slug(&design_slug)?;
    let portfolio_categories: Vec<serde_json::Value> = store
        .category_list(Some("portfolio"))
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "slug": c.slug, "show_in_nav": c.show_in_nav}))
        .collect();
    let journal_categories: Vec<serde_json::Value> = store
        .category_list(Some("post"))
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "slug": c.slug, "show_in_nav": c.show_in_nav}))
        .collect();

    let context = json!({
        "page_title": format!("Design: {}", design.name),
        "design": design,
        "admin_slug": slug.0,
        "settings": store.setting_all(),
        "portfolio_categories": portfolio_categories,
        "journal_categories": journal_categories,
    });

    Some(Template::render("admin/designs/overview", &context))
}
