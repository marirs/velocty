use std::sync::Arc;

use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::security::auth::EditorUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Comments ───────────────────────────────────────────

#[get("/comments?<status>&<page>")]
pub fn comments_list(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let comments = store.comment_list(status.as_deref(), per_page, offset);
    let total = store.comment_count(status.as_deref());
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    let context = json!({
        "page_title": "Comments",
        "comments": comments,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": store.comment_count(None),
        "count_pending": store.comment_count(Some("pending")),
        "count_approved": store.comment_count(Some("approved")),
        "count_spam": store.comment_count(Some("spam")),
        "admin_slug": slug.0,
        "settings": store.setting_all(),
    });

    Template::render("admin/comments/list", &context)
}

#[post("/comments/<id>/approve")]
pub fn comment_approve(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let _ = store.comment_update_status(id, "approved");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/spam")]
pub fn comment_spam(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let _ = store.comment_update_status(id, "spam");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/delete")]
pub fn comment_delete(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let _ = store.comment_delete(id);
    Redirect::to(format!("{}/comments", admin_base(slug)))
}
