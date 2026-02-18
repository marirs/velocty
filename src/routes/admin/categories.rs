use rocket::form::Form;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::{json, Value};

use super::admin_base;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::category::{Category, CategoryForm};
use crate::models::settings::Setting;
use crate::models::tag::Tag;
use crate::security::auth::EditorUser;
use crate::AdminSlug;

// ── Categories ─────────────────────────────────────────

#[get("/categories?<type_filter>&<page>")]
pub fn categories_list(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    type_filter: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let categories = Category::list_paginated(pool, type_filter.as_deref(), per_page, offset);
    let total = Category::count(pool, type_filter.as_deref());
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    let categories_with_count: Vec<serde_json::Value> = categories
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "slug": c.slug,
                "type": c.r#type,
                "count": Category::count_items(pool, c.id),
                "show_in_nav": c.show_in_nav,
            })
        })
        .collect();

    let context = json!({
        "page_title": "Categories",
        "categories": categories_with_count,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "type_filter": type_filter,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/categories/list", &context)
}

// ── Tags ───────────────────────────────────────────────

#[get("/tags?<page>")]
pub fn tags_list(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let tags = Tag::list_paginated(pool, per_page, offset);
    let total = Tag::count(pool);
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    let tags_with_count: Vec<serde_json::Value> = tags
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "slug": t.slug,
                "count": Tag::count_items(pool, t.id),
            })
        })
        .collect();

    let context = json!({
        "page_title": "Tags",
        "tags": tags_with_count,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/tags/list", &context)
}

// ── POST: Category Create/Update/Delete ────────────────

#[derive(FromForm)]
pub struct CategoryFormData {
    pub name: String,
    pub slug: String,
    pub r#type: String,
}

#[post("/categories/new", data = "<form>")]
pub fn category_create(
    _admin: EditorUser,
    pool: &State<DbPool>,
    admin_slug: &State<AdminSlug>,
    form: Form<CategoryFormData>,
) -> Redirect {
    let cat_slug = if form.slug.is_empty() {
        slug::slugify(&form.name)
    } else {
        form.slug.clone()
    };
    let _ = Category::create(
        pool,
        &CategoryForm {
            name: form.name.clone(),
            slug: cat_slug,
            r#type: form.r#type.clone(),
        },
    );
    Redirect::to(format!("{}/categories", admin_base(admin_slug)))
}

#[post("/api/categories/create", format = "json", data = "<data>")]
pub fn api_category_create(
    _admin: EditorUser,
    pool: &State<DbPool>,
    data: Json<Value>,
) -> Json<Value> {
    let name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let cat_type = data
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("post")
        .to_string();
    if name.is_empty() {
        return Json(json!({"ok": false, "error": "Name is required"}));
    }
    let cat_slug = slug::slugify(&name);
    match Category::create(
        pool,
        &CategoryForm {
            name: name.clone(),
            slug: cat_slug,
            r#type: cat_type,
        },
    ) {
        Ok(id) => Json(json!({"ok": true, "id": id, "name": name})),
        Err(e) => Json(json!({"ok": false, "error": e})),
    }
}

#[post("/categories/<id>/edit", data = "<form>")]
pub fn category_update(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
    form: Form<CategoryFormData>,
) -> Redirect {
    let cat_slug = if form.slug.is_empty() {
        slug::slugify(&form.name)
    } else {
        form.slug.clone()
    };
    let _ = Category::update(
        pool,
        id,
        &CategoryForm {
            name: form.name.clone(),
            slug: cat_slug,
            r#type: form.r#type.clone(),
        },
    );
    Redirect::to(format!("{}/categories", admin_base(slug)))
}

#[post("/categories/<id>/delete")]
pub fn category_delete(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let name = Category::find_by_id(pool, id)
        .map(|c| c.name)
        .unwrap_or_default();
    let _ = Category::delete(pool, id);
    AuditEntry::log(
        pool,
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "delete",
        Some("category"),
        Some(id),
        Some(&name),
        None,
        None,
    );
    Redirect::to(format!("{}/categories", admin_base(slug)))
}

// ── POST: Category Nav Visibility Toggle ────────────────

#[post("/api/categories/<id>/toggle-nav", format = "json", data = "<data>")]
pub fn api_category_toggle_nav(
    _admin: EditorUser,
    pool: &State<DbPool>,
    id: i64,
    data: Json<Value>,
) -> Json<Value> {
    let show = data.get("show").and_then(|v| v.as_bool()).unwrap_or(true);
    match Category::set_show_in_nav(pool, id, show) {
        Ok(()) => Json(json!({"ok": true, "show_in_nav": show})),
        Err(e) => Json(json!({"ok": false, "error": e})),
    }
}

// ── POST: Tag Delete ───────────────────────────────────

#[post("/tags/<id>/delete")]
pub fn tag_delete(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let _ = Tag::delete(pool, id);
    Redirect::to(format!("{}/tags", admin_base(slug)))
}
