use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AuthorUser;
use crate::db::DbPool;
use crate::models::comment::Comment;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::Setting;
use crate::AdminSlug;

// ── Dashboard ──────────────────────────────────────────

#[get("/")]
pub fn dashboard(_admin: AuthorUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let posts_count = Post::count(pool, None);
    let posts_draft = Post::count(pool, Some("draft"));
    let portfolio_count = PortfolioItem::count(pool, None);
    let comments_pending = Comment::count(pool, Some("pending"));

    let context = json!({
        "page_title": "Dashboard",
        "admin_slug": slug.0,
        "posts_count": posts_count,
        "posts_draft": posts_draft,
        "portfolio_count": portfolio_count,
        "comments_pending": comments_pending,
        "settings": Setting::all(pool),
    });

    Template::render("admin/dashboard", &context)
}
