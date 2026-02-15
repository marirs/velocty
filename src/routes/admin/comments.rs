use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::EditorUser;
use crate::db::DbPool;
use crate::models::comment::Comment;
use crate::models::settings::Setting;
use crate::AdminSlug;
use super::admin_base;

// ── Comments ───────────────────────────────────────────

#[get("/comments?<status>&<page>")]
pub fn comments_list(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let comments = Comment::list(pool, status.as_deref(), per_page, offset);
    let total = Comment::count(pool, status.as_deref());
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    let context = json!({
        "page_title": "Comments",
        "comments": comments,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": Comment::count(pool, None),
        "count_pending": Comment::count(pool, Some("pending")),
        "count_approved": Comment::count(pool, Some("approved")),
        "count_spam": Comment::count(pool, Some("spam")),
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/comments/list", &context)
}

#[post("/comments/<id>/approve")]
pub fn comment_approve(_admin: EditorUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "approved");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/spam")]
pub fn comment_spam(_admin: EditorUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "spam");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/delete")]
pub fn comment_delete(_admin: EditorUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::delete(pool, id);
    Redirect::to(format!("{}/comments", admin_base(slug)))
}
