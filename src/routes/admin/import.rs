use std::sync::Arc;

use rocket::data::{Data, ToByteUnit};
use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Import ─────────────────────────────────────────────

#[get("/import")]
pub fn import_page(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
) -> Template {
    let history = store.import_list();

    let context = json!({
        "page_title": "Import",
        "history": history,
        "admin_slug": slug.0,
        "settings": store.setting_all(),
    });

    Template::render("admin/import/index", &context)
}

// ── POST: WordPress Import ─────────────────────────────

#[post("/import/wordpress", data = "<data>")]
pub async fn import_wordpress(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Redirect {
    // Read up to 50MB of upload data
    let bytes = match data.open(50.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => return Redirect::to(format!("{}/import", admin_base(slug))),
    };

    let s: &dyn Store = &**store.inner();
    let xml_content = String::from_utf8_lossy(&bytes).to_string();
    let _ = crate::import::wordpress::import_wxr(s, &xml_content);
    Redirect::to(format!("{}/import", admin_base(slug)))
}

// ── Import: Velocty ───────────────────────────────────────

#[post("/import/velocty", data = "<data>")]
pub async fn import_velocty(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
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

    let s: &dyn Store = &**store.inner();

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
                if s.category_find_by_slug(slug_val).is_none() {
                    let _ = s.category_create(&crate::models::category::CategoryForm {
                        name: name.to_string(),
                        slug: slug_val.to_string(),
                        r#type: "post".to_string(),
                    });
                }
                imported_categories += 1;
            }
        }
    }

    // Import tags
    if let Some(tags) = export.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                let _ = s.tag_find_or_create(name);
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
            if !title.is_empty() && s.post_find_by_slug(slug_val).is_none() {
                let form = crate::models::post::PostForm {
                    title: title.to_string(),
                    slug: slug_val.to_string(),
                    content_json: "{}".to_string(),
                    content_html: body.to_string(),
                    excerpt: if excerpt.is_empty() {
                        None
                    } else {
                        Some(excerpt.to_string())
                    },
                    featured_image: if featured_image.is_empty() {
                        None
                    } else {
                        Some(featured_image.to_string())
                    },
                    meta_title: None,
                    meta_description: None,
                    status: status.to_string(),
                    published_at: None,
                    category_ids: None,
                    tag_ids: None,
                };
                let _ = s.post_create(&form);
                imported_posts += 1;
            }
        }
    }

    // Import portfolio items
    if let Some(items) = export
        .get("portfolio")
        .or_else(|| export.get("portfolio_items"))
        .and_then(|v| v.as_array())
    {
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
            if !title.is_empty() && s.portfolio_find_by_slug(slug_val).is_none() {
                let form = crate::models::portfolio::PortfolioForm {
                    title: title.to_string(),
                    slug: slug_val.to_string(),
                    description_json: None,
                    description_html: Some(description.to_string()),
                    image_path: image_path.to_string(),
                    thumbnail_path: None,
                    meta_title: None,
                    meta_description: None,
                    sell_enabled: None,
                    price: None,
                    purchase_note: None,
                    payment_provider: None,
                    download_file_path: None,
                    status: status.to_string(),
                    published_at: None,
                    category_ids: None,
                    tag_ids: None,
                };
                let _ = s.portfolio_create(&form);
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
            if post_id > 0 && !body.is_empty() {
                let form = crate::models::comment::CommentForm {
                    post_id,
                    content_type: Some("post".to_string()),
                    author_name: author_name.to_string(),
                    author_email: if author_email.is_empty() {
                        None
                    } else {
                        Some(author_email.to_string())
                    },
                    body: body.to_string(),
                    honeypot: None,
                    parent_id: None,
                };
                if let Ok(cid) = s.comment_create(&form) {
                    let _ = s.comment_update_status(cid, status);
                }
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
                let _ = s.setting_set(key, value);
            }
        }
    }

    // Log import
    let _ = s.import_create(
        "velocty",
        Some("velocty_export.json"),
        imported_posts as i64,
        imported_portfolio as i64,
        imported_comments as i64,
        0i64,
        None,
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
