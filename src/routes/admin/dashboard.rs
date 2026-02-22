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

    let settings = store.setting_all();
    let mta_enabled = settings
        .get("email_builtin_enabled")
        .map(|v| v == "true")
        .unwrap_or(false);

    let (mta_sent, mta_pending, mta_failed, mta_total) = if mta_enabled {
        store.mta_queue_stats()
    } else {
        (0, 0, 0, 0)
    };
    let mta_sent_hour = if mta_enabled {
        store.mta_queue_sent_last_hour().unwrap_or(0)
    } else {
        0
    };
    let mta_max_hour: u64 = if mta_enabled {
        settings
            .get("mta_max_emails_per_hour")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    } else {
        30
    };

    let context = json!({
        "page_title": "Dashboard",
        "admin_slug": slug.0,
        "posts_count": posts_count,
        "posts_draft": posts_draft,
        "portfolio_count": portfolio_count,
        "comments_pending": comments_pending,
        "mta_enabled": mta_enabled,
        "mta_sent": mta_sent,
        "mta_pending": mta_pending,
        "mta_failed": mta_failed,
        "mta_total": mta_total,
        "mta_sent_hour": mta_sent_hour,
        "mta_max_hour": mta_max_hour,
        "settings": settings,
    });

    Template::render("admin/dashboard", &context)
}
