use std::sync::Arc;

use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AuthorUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Dashboard ──────────────────────────────────────────

#[get("/")]
pub fn dashboard(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
) -> Template {
    let posts_count = store.post_count(None);
    let posts_draft = store.post_count(Some("draft"));
    let portfolio_count = store.portfolio_count(None);
    let comments_pending = store.comment_count(Some("pending"));

    let context = json!({
        "page_title": "Dashboard",
        "admin_slug": slug.0,
        "posts_count": posts_count,
        "posts_draft": posts_draft,
        "portfolio_count": portfolio_count,
        "comments_pending": comments_pending,
        "settings": store.setting_all(),
    });

    Template::render("admin/dashboard", &context)
}
