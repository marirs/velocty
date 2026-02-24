use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

use crate::models::portfolio::PortfolioForm;
use crate::models::post::PostForm;
use crate::store::Store;

// ── Public Endpoints (key-authenticated, no admin session) ──────────

#[derive(Debug, Deserialize)]
pub struct PreflightRequest {
    pub key: String,
}

/// Preflight check: verify deploy key and return environment info.
/// Called by the sender (Dev/Staging) before starting a deploy.
#[post("/deploy/preflight", format = "json", data = "<body>")]
pub fn deploy_preflight(
    store: &State<Arc<dyn Store>>,
    body: Json<PreflightRequest>,
) -> Json<Value> {
    let env = store.setting_get_or("site_environment", "staging");
    let expected_key = store.setting_get("deploy_receive_key").unwrap_or_default();

    if expected_key.is_empty() {
        return Json(json!({
            "ok": false,
            "error": "This instance has no deploy receive key configured"
        }));
    }

    if !constant_time_eq(body.key.as_bytes(), expected_key.as_bytes()) {
        return Json(json!({
            "ok": false,
            "error": "Invalid deploy key"
        }));
    }

    let site_name = store.setting_get_or("site_name", "Velocty");

    Json(json!({
        "ok": true,
        "environment": env,
        "site_name": site_name,
    }))
}

// ── Receive Endpoint ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeployReceiveRequest {
    pub key: String,
    pub posts: Vec<DeployPost>,
    pub portfolio: Vec<DeployPortfolioItem>,
    pub categories: Vec<DeployCategory>,
    pub tags: Vec<DeployTag>,
    pub uploads: Vec<DeployUpload>,
    pub settings: Vec<DeploySetting>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeployPost {
    pub slug: String,
    pub title: String,
    pub content_json: String,
    pub content_html: String,
    pub excerpt: Option<String>,
    pub featured_image: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub published_at: Option<String>,
    pub category_slugs: Vec<String>,
    pub tag_names: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeployPortfolioItem {
    pub slug: String,
    pub title: String,
    pub description_json: Option<String>,
    pub description_html: Option<String>,
    pub image_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub published_at: Option<String>,
    pub category_slugs: Vec<String>,
    pub tag_names: Vec<String>,
    pub sell_enabled: Option<bool>,
    pub price: Option<f64>,
    pub purchase_note: Option<String>,
    pub payment_provider: Option<String>,
    pub download_file_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeployCategory {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub category_type: String,
    pub parent_slug: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeployTag {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeployUpload {
    pub path: String,
    pub data_base64: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeploySetting {
    pub key: String,
    pub value: String,
}

/// Receive a content bundle from a Dev/Staging instance.
/// Uses slug-based upsert: if a post/portfolio item with the same slug exists, update it;
/// otherwise create a new one.
#[post("/deploy/receive", format = "json", data = "<body>")]
pub fn deploy_receive(
    store: &State<Arc<dyn Store>>,
    body: Json<DeployReceiveRequest>,
) -> Json<Value> {
    let env = store.setting_get_or("site_environment", "staging");
    let expected_key = store.setting_get("deploy_receive_key").unwrap_or_default();

    if expected_key.is_empty() {
        return Json(json!({ "ok": false, "error": "No deploy receive key configured" }));
    }
    if !constant_time_eq(body.key.as_bytes(), expected_key.as_bytes()) {
        return Json(json!({ "ok": false, "error": "Invalid deploy key" }));
    }
    if env != "production" {
        return Json(json!({ "ok": false, "error": "This instance is not a production server" }));
    }

    let s: &dyn Store = &**store.inner();
    let mut stats = json!({
        "posts_created": 0,
        "posts_updated": 0,
        "portfolio_created": 0,
        "portfolio_updated": 0,
        "categories_synced": 0,
        "tags_synced": 0,
        "uploads_written": 0,
        "settings_synced": 0,
    });

    // ── 1. Sync categories ──
    for cat in &body.categories {
        let existing = s.category_find_by_slug(&cat.slug);
        if existing.is_none() {
            let form = crate::models::category::CategoryForm {
                name: cat.name.clone(),
                slug: cat.slug.clone(),
                r#type: cat.category_type.clone(),
            };
            let _ = s.category_create(&form);
        }
        *stats.get_mut("categories_synced").unwrap() =
            json!(stats["categories_synced"].as_i64().unwrap_or(0) + 1);
    }

    // ── 2. Sync tags ──
    for tag in &body.tags {
        let _ = s.tag_find_or_create(&tag.name);
        *stats.get_mut("tags_synced").unwrap() =
            json!(stats["tags_synced"].as_i64().unwrap_or(0) + 1);
    }

    // ── 3. Write uploads ──
    for upload in &body.uploads {
        if let Ok(bytes) = base64_decode(&upload.data_base64) {
            let full_path = format!("website/site/{}", upload.path.trim_start_matches('/'));
            if !is_safe_upload_path(&full_path) {
                log::warn!(
                    "Deploy receive: blocked path traversal attempt: {}",
                    upload.path
                );
                continue;
            }
            // Validate file extension against allowed media types
            if !is_allowed_deploy_extension(&full_path) {
                log::warn!(
                    "Deploy receive: blocked disallowed file type: {}",
                    upload.path
                );
                continue;
            }
            if let Some(parent) = Path::new(&full_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Sanitize SVG files before writing
            if full_path.to_lowercase().ends_with(".svg") {
                if let Some(clean) = crate::svg_sanitizer::sanitize_svg(&bytes) {
                    let _ = std::fs::write(&full_path, &clean);
                } else {
                    log::warn!("Deploy receive: SVG sanitization failed: {}", upload.path);
                    continue;
                }
            } else if std::fs::write(&full_path, &bytes).is_ok() {
                // written successfully
            } else {
                continue;
            }
            *stats.get_mut("uploads_written").unwrap() =
                json!(stats["uploads_written"].as_i64().unwrap_or(0) + 1);
        }
    }

    // ── 4. Sync posts (slug-based upsert) ──
    for post in &body.posts {
        let category_ids = resolve_category_ids(s, &post.category_slugs);
        let tag_ids = resolve_tag_ids(s, &post.tag_names);

        let form = PostForm {
            title: post.title.clone(),
            slug: post.slug.clone(),
            content_json: post.content_json.clone(),
            content_html: post.content_html.clone(),
            excerpt: post.excerpt.clone(),
            featured_image: post.featured_image.clone(),
            meta_title: post.meta_title.clone(),
            meta_description: post.meta_description.clone(),
            status: post.status.clone(),
            published_at: post.published_at.clone(),
            category_ids: if category_ids.is_empty() {
                None
            } else {
                Some(category_ids.clone())
            },
            tag_ids: if tag_ids.is_empty() {
                None
            } else {
                Some(tag_ids.clone())
            },
        };

        if let Some(existing) = s.post_find_by_slug(&post.slug) {
            let _ = s.post_update(existing.id, &form);
            if !category_ids.is_empty() {
                let _ = s.category_set_for_content(existing.id, "post", &category_ids);
            }
            if !tag_ids.is_empty() {
                let _ = s.tag_set_for_content(existing.id, "post", &tag_ids);
            }
            *stats.get_mut("posts_updated").unwrap() =
                json!(stats["posts_updated"].as_i64().unwrap_or(0) + 1);
        } else {
            if let Ok(new_id) = s.post_create(&form) {
                if !category_ids.is_empty() {
                    let _ = s.category_set_for_content(new_id, "post", &category_ids);
                }
                if !tag_ids.is_empty() {
                    let _ = s.tag_set_for_content(new_id, "post", &tag_ids);
                }
            }
            *stats.get_mut("posts_created").unwrap() =
                json!(stats["posts_created"].as_i64().unwrap_or(0) + 1);
        }
    }

    // ── 5. Sync portfolio items (slug-based upsert) ──
    for item in &body.portfolio {
        let category_ids = resolve_category_ids(s, &item.category_slugs);
        let tag_ids = resolve_tag_ids(s, &item.tag_names);

        let form = PortfolioForm {
            title: item.title.clone(),
            slug: item.slug.clone(),
            description_json: item.description_json.clone(),
            description_html: item.description_html.clone(),
            image_path: item.image_path.clone().unwrap_or_default(),
            thumbnail_path: item.thumbnail_path.clone(),
            meta_title: item.meta_title.clone(),
            meta_description: item.meta_description.clone(),
            sell_enabled: item.sell_enabled,
            price: item.price,
            purchase_note: item.purchase_note.clone(),
            payment_provider: item.payment_provider.clone(),
            download_file_path: item.download_file_path.clone(),
            status: item.status.clone(),
            published_at: item.published_at.clone(),
            category_ids: if category_ids.is_empty() {
                None
            } else {
                Some(category_ids.clone())
            },
            tag_ids: if tag_ids.is_empty() {
                None
            } else {
                Some(tag_ids.clone())
            },
        };

        if let Some(existing) = s.portfolio_find_by_slug(&item.slug) {
            let _ = s.portfolio_update(existing.id, &form);
            if !category_ids.is_empty() {
                let _ = s.category_set_for_content(existing.id, "portfolio", &category_ids);
            }
            if !tag_ids.is_empty() {
                let _ = s.tag_set_for_content(existing.id, "portfolio", &tag_ids);
            }
            *stats.get_mut("portfolio_updated").unwrap() =
                json!(stats["portfolio_updated"].as_i64().unwrap_or(0) + 1);
        } else {
            if let Ok(new_id) = s.portfolio_create(&form) {
                if !category_ids.is_empty() {
                    let _ = s.category_set_for_content(new_id, "portfolio", &category_ids);
                }
                if !tag_ids.is_empty() {
                    let _ = s.tag_set_for_content(new_id, "portfolio", &tag_ids);
                }
            }
            *stats.get_mut("portfolio_created").unwrap() =
                json!(stats["portfolio_created"].as_i64().unwrap_or(0) + 1);
        }
    }

    // ── 6. Sync settings (only safe keys) ──
    let safe_setting_keys = [
        "site_name",
        "site_caption",
        "site_logo",
        "site_favicon",
        "copyright_text",
        "blog_label",
        "portfolio_label",
        "contact_label",
    ];
    for setting in &body.settings {
        if safe_setting_keys.iter().any(|k| setting.key == *k) {
            let _ = s.setting_set(&setting.key, &setting.value);
            *stats.get_mut("settings_synced").unwrap() =
                json!(stats["settings_synced"].as_i64().unwrap_or(0) + 1);
        }
    }

    // ── 7. Rotate deploy key after successful receive ──
    {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        let _ = s.setting_set("deploy_receive_key", &hex::encode(bytes));
    }

    s.audit_log(
        None,
        Some("deploy"),
        "deploy_receive",
        None,
        None,
        None,
        Some(&format!(
            "Deploy received: {} posts, {} portfolio items, {} uploads",
            body.posts.len(),
            body.portfolio.len(),
            body.uploads.len()
        )),
        None,
    );

    Json(json!({
        "ok": true,
        "stats": stats,
    }))
}

// ── Admin Send Endpoint (gather content for deploy) ─────────────────

/// Gather all content from this instance for deployment.
/// Called by the deploy modal on the Dev/Staging side.
#[get("/deploy/gather")]
pub fn deploy_gather(
    _admin: crate::security::auth::AdminUser,
    store: &State<Arc<dyn Store>>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();

    // Gather all posts
    let total_posts = s.post_count(None);
    let posts = s.post_list(None, total_posts.max(1), 0);
    let deploy_posts: Vec<Value> = posts
        .iter()
        .map(|p| {
            let cats = s.category_for_content(p.id, "post");
            let tags = s.tag_for_content(p.id, "post");
            json!({
                "slug": p.slug,
                "title": p.title,
                "content_json": p.content_json,
                "content_html": p.content_html,
                "excerpt": p.excerpt,
                "featured_image": p.featured_image,
                "meta_title": p.meta_title,
                "meta_description": p.meta_description,
                "status": p.status,
                "published_at": p.published_at.map(|d| d.to_string()),
                "category_slugs": cats.iter().map(|c| c.slug.clone()).collect::<Vec<_>>(),
                "tag_names": tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Gather all portfolio items
    let total_portfolio = s.portfolio_count(None);
    let items = s.portfolio_list(None, total_portfolio.max(1), 0);
    let deploy_portfolio: Vec<Value> = items
        .iter()
        .map(|i| {
            let cats = s.category_for_content(i.id, "portfolio");
            let tags = s.tag_for_content(i.id, "portfolio");
            json!({
                "slug": i.slug,
                "title": i.title,
                "description_json": i.description_json,
                "description_html": i.description_html,
                "image_path": i.image_path,
                "thumbnail_path": i.thumbnail_path,
                "meta_title": i.meta_title,
                "meta_description": i.meta_description,
                "status": i.status,
                "published_at": i.published_at.map(|d| d.to_string()),
                "category_slugs": cats.iter().map(|c| c.slug.clone()).collect::<Vec<_>>(),
                "tag_names": tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
                "sell_enabled": i.sell_enabled,
                "price": i.price,
                "purchase_note": i.purchase_note,
                "payment_provider": i.payment_provider,
                "download_file_path": i.download_file_path,
            })
        })
        .collect();

    // Gather categories
    let categories = s.category_list(None);
    let deploy_categories: Vec<Value> = categories
        .iter()
        .map(|c| {
            json!({
                "slug": c.slug,
                "name": c.name,
                "category_type": c.r#type,
            })
        })
        .collect();

    // Gather tags
    let tags = s.tag_list();
    let deploy_tags: Vec<Value> = tags
        .iter()
        .map(|t| {
            json!({
                "slug": t.slug,
                "name": t.name,
            })
        })
        .collect();

    // Gather upload file paths (not the data — that's sent separately)
    let upload_dir = std::path::Path::new("website/site/uploads");
    let mut upload_paths: Vec<String> = Vec::new();
    if upload_dir.exists() {
        collect_files(upload_dir, "uploads", &mut upload_paths);
    }

    Json(json!({
        "ok": true,
        "posts": deploy_posts,
        "portfolio": deploy_portfolio,
        "categories": deploy_categories,
        "tags": deploy_tags,
        "upload_paths": upload_paths,
        "total_posts": deploy_posts.len(),
        "total_portfolio": deploy_portfolio.len(),
        "total_categories": deploy_categories.len(),
        "total_tags": deploy_tags.len(),
        "total_uploads": upload_paths.len(),
    }))
}

/// Read a single upload file as base64 for transfer.
#[get("/deploy/upload-data?<path>")]
pub fn deploy_upload_data(_admin: crate::security::auth::AdminUser, path: &str) -> Json<Value> {
    let full_path = format!("website/site/{}", path.trim_start_matches('/'));
    if !is_safe_upload_path(&full_path) {
        return Json(json!({ "ok": false, "error": "Invalid path" }));
    }
    match std::fs::read(&full_path) {
        Ok(bytes) => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Json(json!({ "ok": true, "data": encoded, "path": path }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn resolve_category_ids(store: &dyn Store, slugs: &[String]) -> Vec<i64> {
    slugs
        .iter()
        .filter_map(|slug| store.category_find_by_slug(slug).map(|c| c.id))
        .collect()
}

fn resolve_tag_ids(store: &dyn Store, names: &[String]) -> Vec<i64> {
    names
        .iter()
        .filter_map(|name| store.tag_find_or_create(name).ok())
        .collect()
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

/// Validate that a file path resolves within website/site/uploads/ to prevent path traversal.
fn is_safe_upload_path(path: &str) -> bool {
    // Reject null bytes
    if path.contains('\0') {
        return false;
    }
    let path = Path::new(path);
    // Reject paths containing .. components
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return false;
        }
    }
    // Must be under website/site/uploads/
    let normalized = path.to_string_lossy();
    if !normalized.starts_with("website/site/uploads/")
        && !normalized.starts_with("website/site/uploads\\")
    {
        return false;
    }
    // Double-check: if the path exists on disk, verify canonical path is still under uploads
    let uploads_base = Path::new("website/site/uploads");
    if let (Ok(canon_base), Ok(canon_path)) = (uploads_base.canonicalize(), path.canonicalize()) {
        if !canon_path.starts_with(&canon_base) {
            return false;
        }
    }
    true
}

/// Allowlist of file extensions permitted in deploy uploads.
fn is_allowed_deploy_extension(path: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "svg", "tiff", "ico", "avif", "mp4", "webm", "mov",
        "avi", "woff2", "woff", "ttf", "otf", "pdf",
    ];
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    ALLOWED.contains(&ext.as_str())
}

/// Constant-time comparison to prevent timing attacks on deploy keys.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn collect_files(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            if path.is_dir() {
                collect_files(&path, &rel, out);
            } else {
                out.push(rel);
            }
        }
    }
}

pub fn public_routes() -> Vec<rocket::Route> {
    routes![deploy_preflight, deploy_receive]
}

pub fn admin_routes() -> Vec<rocket::Route> {
    routes![deploy_gather, deploy_upload_data]
}
