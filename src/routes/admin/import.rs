use rocket::data::{Data, ToByteUnit};
use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::db::DbPool;
use crate::models::import::Import;
use crate::models::settings::Setting;
use crate::security::auth::AdminUser;
use crate::AdminSlug;

// ── Import ─────────────────────────────────────────────

#[get("/import")]
pub fn import_page(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let history = Import::list(pool);

    let context = json!({
        "page_title": "Import",
        "history": history,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/import/index", &context)
}

// ── POST: WordPress Import ─────────────────────────────

#[post("/import/wordpress", data = "<data>")]
pub async fn import_wordpress(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Redirect {
    // Read up to 50MB of upload data
    let bytes = match data.open(50.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => return Redirect::to(format!("{}/import", admin_base(slug))),
    };

    let xml_content = String::from_utf8_lossy(&bytes).to_string();
    let _ = crate::import::wordpress::import_wxr(pool, &xml_content);
    Redirect::to(format!("{}/import", admin_base(slug)))
}

// ── Import: Velocty ───────────────────────────────────────

#[post("/import/velocty", data = "<data>")]
pub async fn import_velocty(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Flash<Redirect> {
    let bytes = match data.open(100.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => {
            return Flash::error(
                Redirect::to(format!("{}/import", admin_base(slug))),
                "Failed to read upload data.",
            )
        }
    };

    let json_str = String::from_utf8_lossy(&bytes).to_string();
    let export: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            return Flash::error(
                Redirect::to(format!("{}/import", admin_base(slug))),
                format!("Invalid JSON: {}", e),
            )
        }
    };

    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            return Flash::error(
                Redirect::to(format!("{}/import", admin_base(slug))),
                format!("DB error: {}", e),
            )
        }
    };

    let mut imported_posts = 0u64;
    let mut imported_portfolio = 0u64;
    let mut imported_comments = 0u64;
    let mut imported_categories = 0u64;
    let mut imported_tags = 0u64;

    // Import categories
    if let Some(cats) = export.get("categories").and_then(|v| v.as_array()) {
        for cat in cats {
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO categories (name, slug) VALUES (?1, ?2)",
                    rusqlite::params![name, slug_val],
                );
                imported_categories += 1;
            }
        }
    }

    // Import tags
    if let Some(tags) = export.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO tags (name, slug) VALUES (?1, ?2)",
                    rusqlite::params![name, slug_val],
                );
                imported_tags += 1;
            }
        }
    }

    // Import posts
    if let Some(posts) = export.get("posts").and_then(|v| v.as_array()) {
        for post in posts {
            let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let body = post.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
            let featured_image = post
                .get("featured_image")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = post
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");
            let created_at = post
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let updated_at = post
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !title.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO posts (title, slug, body, excerpt, featured_image, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![title, slug_val, body, excerpt, featured_image, status, created_at, updated_at],
                );
                imported_posts += 1;
            }
        }
    }

    // Import portfolio items
    if let Some(items) = export.get("portfolio_items").and_then(|v| v.as_array()) {
        for item in items {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let image_path = item
                .get("image_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");
            let created_at = item
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let updated_at = item
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !title.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO portfolio_items (title, slug, description, image_path, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![title, slug_val, description, image_path, status, created_at, updated_at],
                );
                imported_portfolio += 1;
            }
        }
    }

    // Import comments
    if let Some(comments) = export.get("comments").and_then(|v| v.as_array()) {
        for comment in comments {
            let post_id = comment.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let author_name = comment
                .get("author_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let author_email = comment
                .get("author_email")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let body = comment.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let status = comment
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            let created_at = comment
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if post_id > 0 && !body.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO comments (post_id, author_name, author_email, body, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![post_id, author_name, author_email, body, status, created_at],
                );
                imported_comments += 1;
            }
        }
    }

    // Import settings (skip sensitive ones)
    let skip_keys = [
        "admin_password_hash",
        "setup_completed",
        "admin_slug",
        "admin_email",
        "session_secret",
    ];
    if let Some(settings) = export.get("settings").and_then(|v| v.as_array()) {
        for setting in settings {
            let key = setting.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let value = setting.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if !key.is_empty() && !skip_keys.contains(&key) {
                let _ = Setting::set(pool, key, value);
            }
        }
    }

    // Log import
    let _ = conn.execute(
        "INSERT INTO imports (source, filename, posts_count, portfolio_count, comments_count, skipped_count, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
        rusqlite::params![
            "velocty",
            "velocty_export.json",
            imported_posts,
            imported_portfolio,
            imported_comments,
            0i64,
        ],
    );

    Flash::success(
        Redirect::to(format!("{}/import", admin_base(slug))),
        format!(
            "Imported {} posts, {} portfolio items, {} comments, {} categories, {} tags.",
            imported_posts,
            imported_portfolio,
            imported_comments,
            imported_categories,
            imported_tags
        ),
    )
}
