use std::collections::HashMap;
use std::io::{Cursor, Read};
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
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Template {
    let history = store.import_list();

    let mut context = json!({
        "page_title": "Import",
        "history": history,
        "admin_slug": slug.get(),
        "settings": store.setting_all(),
    });

    if let Some(ref f) = flash {
        context["flash_kind"] = json!(f.kind());
        context["flash_msg"] = json!(f.message());
    }

    Template::render("admin/import/index", &context)
}

// ── POST: WordPress Import ─────────────────────────────

#[post("/import/wordpress", data = "<data>")]
pub async fn import_wordpress(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Flash<Redirect> {
    let redirect_url = format!("{}/import", admin_base(slug));

    let bytes = match data.open(100.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => {
            return Flash::error(
                Redirect::to(redirect_url),
                "Failed to read upload data (max 100 MB).",
            )
        }
    };

    let s: &dyn Store = &**store.inner();
    let xml_content = String::from_utf8_lossy(&bytes).to_string();

    match crate::import::wordpress::import_wxr(s, &xml_content) {
        Ok(r) => {
            let mut msg = format!(
                "Imported {} posts, {} comments",
                r.posts_imported, r.comments_imported
            );
            if r.media_downloaded > 0 {
                msg.push_str(&format!(", {} media files downloaded", r.media_downloaded));
            }
            if r.media_failed > 0 {
                msg.push_str(&format!(", {} media failed", r.media_failed));
            }
            if r.skipped > 0 {
                msg.push_str(&format!(", {} skipped", r.skipped));
            }
            msg.push('.');
            Flash::success(Redirect::to(redirect_url), msg)
        }
        Err(e) => Flash::error(
            Redirect::to(redirect_url),
            format!("WordPress import failed: {}", e),
        ),
    }
}

// ── Import: Velocty (ZIP or JSON) ─────────────────────────

#[post("/import/velocty", data = "<data>")]
pub async fn import_velocty(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Flash<Redirect> {
    let redirect_url = format!("{}/import", admin_base(slug));

    let bytes = match data.open(500.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => {
            return Flash::error(
                Redirect::to(redirect_url),
                "Failed to read upload data (max 500 MB).",
            )
        }
    };

    // Try to parse as ZIP first, fall back to raw JSON
    let (export, zip_uploads) = match extract_velocty_zip(&bytes) {
        Some(result) => result,
        None => {
            // Try raw JSON
            let json_str = String::from_utf8_lossy(&bytes).to_string();
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(v) => (v, Vec::new()),
                Err(e) => {
                    return Flash::error(
                        Redirect::to(redirect_url),
                        format!("Invalid file. Expected ZIP or JSON: {}", e),
                    )
                }
            }
        }
    };

    let s: &dyn Store = &**store.inner();
    let result = run_velocty_import(s, &export);

    // Extract uploads from ZIP to disk
    let mut files_extracted = 0u64;
    for (path, data) in &zip_uploads {
        let dest = format!("website/site/{}", path);
        if let Some(parent) = std::path::Path::new(&dest).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&dest, data).is_ok() {
            files_extracted += 1;
        }
    }

    // Log import
    let _ = s.import_create(
        "velocty",
        Some("velocty_export.zip"),
        result.posts as i64,
        result.portfolio as i64,
        result.comments as i64,
        0i64,
        None,
    );

    let mut msg = format!(
        "Imported {} posts, {} portfolio, {} comments, {} categories, {} tags",
        result.posts, result.portfolio, result.comments, result.categories, result.tags
    );
    if result.designs > 0 {
        msg.push_str(&format!(", {} designs", result.designs));
    }
    if result.users > 0 {
        msg.push_str(&format!(", {} users", result.users));
    }
    if files_extracted > 0 {
        msg.push_str(&format!(", {} media files", files_extracted));
    }
    msg.push('.');

    Flash::success(Redirect::to(redirect_url), msg)
}

/// Extract export.json + uploads/ from a ZIP archive.
/// Returns None if the bytes are not a valid ZIP.
#[allow(clippy::type_complexity)]
fn extract_velocty_zip(bytes: &[u8]) -> Option<(serde_json::Value, Vec<(String, Vec<u8>)>)> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;

    let mut json_data: Option<serde_json::Value> = None;
    let mut uploads: Vec<(String, Vec<u8>)> = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).ok()?;
        let name = file.name().to_string();

        if name == "export.json" {
            let mut content = String::new();
            file.read_to_string(&mut content).ok()?;
            json_data = serde_json::from_str(&content).ok();
        } else if name.starts_with("uploads/") && !file.is_dir() {
            let mut data = Vec::new();
            file.read_to_end(&mut data).ok()?;
            uploads.push((name, data));
        }
    }

    json_data.map(|j| (j, uploads))
}

struct ImportCounts {
    posts: u64,
    portfolio: u64,
    comments: u64,
    categories: u64,
    tags: u64,
    designs: u64,
    users: u64,
}

fn run_velocty_import(s: &dyn Store, export: &serde_json::Value) -> ImportCounts {
    let mut counts = ImportCounts {
        posts: 0,
        portfolio: 0,
        comments: 0,
        categories: 0,
        tags: 0,
        designs: 0,
        users: 0,
    };

    // ── ID remap tables ──
    let mut cat_map: HashMap<i64, i64> = HashMap::new(); // old_id → new_id
    let mut tag_map: HashMap<i64, i64> = HashMap::new();
    let mut post_map: HashMap<i64, i64> = HashMap::new();
    let mut portfolio_map: HashMap<i64, i64> = HashMap::new();
    let mut design_map: HashMap<i64, i64> = HashMap::new();
    let mut comment_map: HashMap<i64, i64> = HashMap::new();

    // ── 1. Categories (with correct type) ──
    if let Some(cats) = export.get("categories").and_then(|v| v.as_array()) {
        for cat in cats {
            let old_id = cat.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let cat_type = cat.get("type").and_then(|v| v.as_str()).unwrap_or("post");
            if name.is_empty() {
                continue;
            }

            let new_id = if let Some(existing) = s.category_find_by_slug(slug_val) {
                existing.id
            } else {
                match s.category_create(&crate::models::category::CategoryForm {
                    name: name.to_string(),
                    slug: slug_val.to_string(),
                    r#type: cat_type.to_string(),
                }) {
                    Ok(id) => id,
                    Err(_) => continue,
                }
            };
            if old_id > 0 {
                cat_map.insert(old_id, new_id);
            }
            counts.categories += 1;
        }
    }

    // ── 2. Tags ──
    if let Some(tags) = export.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            let old_id = tag.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            if let Ok(new_id) = s.tag_find_or_create(name) {
                if old_id > 0 {
                    tag_map.insert(old_id, new_id);
                }
                counts.tags += 1;
            }
        }
    }

    // ── 3. Users (random password, they use forgot-password to recover) ──
    if let Some(users) = export.get("users").and_then(|v| v.as_array()) {
        for user in users {
            let email = user.get("email").and_then(|v| v.as_str()).unwrap_or("");
            let display_name = user
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let role = user
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("subscriber");
            if email.is_empty() {
                continue;
            }

            // Skip if user with this email already exists
            if s.user_get_by_email(email).is_some() {
                continue;
            }

            // Generate random password
            let temp_pass: String = (0..16)
                .map(|_| {
                    let idx = rand::random::<u8>() % 62;
                    match idx {
                        0..=9 => (b'0' + idx) as char,
                        10..=35 => (b'a' + idx - 10) as char,
                        _ => (b'A' + idx - 36) as char,
                    }
                })
                .collect();
            let hash = bcrypt::hash(&temp_pass, bcrypt::DEFAULT_COST).unwrap_or_default();

            if let Ok(new_id) = s.user_create(email, &hash, display_name, role) {
                let _ = s.user_set_force_password_change(new_id, true);
                counts.users += 1;
            }
        }
    }

    // ── 4. Posts (all fields) ──
    if let Some(posts) = export.get("posts").and_then(|v| v.as_array()) {
        for post in posts {
            let old_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if title.is_empty() {
                continue;
            }
            if s.post_find_by_slug(slug_val).is_some() {
                continue;
            }

            let content_html = post
                .get("content_html")
                .or_else(|| post.get("body"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content_json = post
                .get("content_json")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
            let featured_image = post
                .get("featured_image")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let meta_title = post
                .get("meta_title")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let meta_desc = post
                .get("meta_description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = post
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");
            let published_at = post
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let form = crate::models::post::PostForm {
                title: title.to_string(),
                slug: slug_val.to_string(),
                content_json: content_json.to_string(),
                content_html: content_html.to_string(),
                excerpt: nonempty(excerpt),
                featured_image: nonempty(featured_image),
                meta_title: nonempty(meta_title),
                meta_description: nonempty(meta_desc),
                status: status.to_string(),
                published_at: nonempty(published_at),
                category_ids: None,
                tag_ids: None,
            };
            if let Ok(new_id) = s.post_create(&form) {
                if old_id > 0 {
                    post_map.insert(old_id, new_id);
                }
                counts.posts += 1;
            }
        }
    }

    // ── 5. Portfolio (all fields) ──
    if let Some(items) = export
        .get("portfolio")
        .or_else(|| export.get("portfolio_items"))
        .and_then(|v| v.as_array())
    {
        for item in items {
            let old_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if title.is_empty() {
                continue;
            }
            if s.portfolio_find_by_slug(slug_val).is_some() {
                continue;
            }

            let desc_html = item
                .get("description_html")
                .or_else(|| item.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let desc_json = item
                .get("description_json")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let image_path = item
                .get("image_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let thumb = item
                .get("thumbnail_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let meta_title = item
                .get("meta_title")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let meta_desc = item
                .get("meta_description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let sell = item.get("sell_enabled").and_then(|v| v.as_i64());
            let price = item.get("price").and_then(|v| v.as_f64());
            let purchase_note = item
                .get("purchase_note")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let payment_provider = item
                .get("payment_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let download_file = item
                .get("download_file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("draft");
            let published_at = item
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let form = crate::models::portfolio::PortfolioForm {
                title: title.to_string(),
                slug: slug_val.to_string(),
                description_json: nonempty(desc_json),
                description_html: nonempty(desc_html),
                image_path: image_path.to_string(),
                thumbnail_path: nonempty(thumb),
                meta_title: nonempty(meta_title),
                meta_description: nonempty(meta_desc),
                sell_enabled: sell.map(|v| v != 0),
                price,
                purchase_note: nonempty(purchase_note),
                payment_provider: nonempty(payment_provider),
                download_file_path: nonempty(download_file),
                status: status.to_string(),
                published_at: nonempty(published_at),
                category_ids: None,
                tag_ids: None,
            };
            if let Ok(new_id) = s.portfolio_create(&form) {
                if old_id > 0 {
                    portfolio_map.insert(old_id, new_id);
                }
                counts.portfolio += 1;
            }
        }
    }

    // ── 6. Restore content_categories relationships ──
    for (key, content_type, id_map) in [
        ("post_categories", "post", &post_map),
        ("portfolio_categories", "portfolio", &portfolio_map),
    ] {
        if let Some(rels) = export.get(key).and_then(|v| v.as_array()) {
            for rel in rels {
                let old_content_id = rel.get("content_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let old_cat_id = rel.get("category_id").and_then(|v| v.as_i64()).unwrap_or(0);
                if let (Some(&new_cid), Some(&new_cat)) =
                    (id_map.get(&old_content_id), cat_map.get(&old_cat_id))
                {
                    let _ = s.category_set_for_content(new_cid, content_type, &[new_cat]);
                }
            }
        }
    }

    // ── 7. Restore content_tags relationships ──
    for (key, content_type, id_map) in [
        ("post_tags", "post", &post_map),
        ("portfolio_tags", "portfolio", &portfolio_map),
    ] {
        if let Some(rels) = export.get(key).and_then(|v| v.as_array()) {
            for rel in rels {
                let old_content_id = rel.get("content_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let old_tag_id = rel.get("tag_id").and_then(|v| v.as_i64()).unwrap_or(0);
                if let (Some(&new_cid), Some(&new_tid)) =
                    (id_map.get(&old_content_id), tag_map.get(&old_tag_id))
                {
                    let _ = s.tag_set_for_content(new_cid, content_type, &[new_tid]);
                }
            }
        }
    }

    // ── 8. Comments (remap post_id, content_type, parent_id) ──
    if let Some(comments) = export.get("comments").and_then(|v| v.as_array()) {
        // First pass: create all comments, build old→new map
        for comment in comments {
            let old_id = comment.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let old_post_id = comment.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let content_type = comment
                .get("content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("post");
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

            if body.is_empty() {
                continue;
            }

            // Remap post_id based on content_type
            let new_post_id = match content_type {
                "portfolio" => portfolio_map
                    .get(&old_post_id)
                    .copied()
                    .unwrap_or(old_post_id),
                _ => post_map.get(&old_post_id).copied().unwrap_or(old_post_id),
            };

            let form = crate::models::comment::CommentForm {
                post_id: new_post_id,
                content_type: Some(content_type.to_string()),
                author_name: author_name.to_string(),
                author_email: nonempty(author_email),
                body: body.to_string(),
                honeypot: None,
                parent_id: None, // Set in second pass
            };
            if let Ok(new_id) = s.comment_create(&form) {
                let _ = s.comment_update_status(new_id, status);
                if old_id > 0 {
                    comment_map.insert(old_id, new_id);
                }
                counts.comments += 1;
            }
        }
        // Second pass: update parent_id for threaded comments
        for comment in comments {
            let old_id = comment.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let old_parent = comment.get("parent_id").and_then(|v| v.as_i64());
            if let Some(old_pid) = old_parent {
                if old_pid > 0 {
                    if let (Some(&new_id), Some(&new_pid)) =
                        (comment_map.get(&old_id), comment_map.get(&old_pid))
                    {
                        let _ = s.comment_set_parent(new_id, Some(new_pid));
                    }
                }
            }
        }
    }

    // ── 9. Designs ──
    if let Some(designs) = export.get("designs").and_then(|v| v.as_array()) {
        for design in designs {
            let old_id = design.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = design.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = design.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let layout_html = design
                .get("layout_html")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let style_css = design
                .get("style_css")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_active = design
                .get("is_active")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if name.is_empty() {
                continue;
            }

            // Skip if design with this slug already exists
            if !slug_val.is_empty() {
                if let Some(existing) = s.design_find_by_slug(slug_val) {
                    if old_id > 0 {
                        design_map.insert(old_id, existing.id);
                    }
                    continue;
                }
            }

            if let Ok(new_id) = s.design_create(name) {
                // Update with full data via raw SQL
                let _ = s.design_update_full(new_id, slug_val, layout_html, style_css);
                if is_active != 0 {
                    let _ = s.design_activate(new_id);
                }
                if old_id > 0 {
                    design_map.insert(old_id, new_id);
                }
                counts.designs += 1;
            }
        }
    }

    // ── 10. Design templates ──
    if let Some(templates) = export.get("design_templates").and_then(|v| v.as_array()) {
        for tmpl in templates {
            let old_design_id = tmpl.get("design_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let template_type = tmpl
                .get("template_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let layout_html = tmpl
                .get("layout_html")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let style_css = tmpl.get("style_css").and_then(|v| v.as_str()).unwrap_or("");
            if template_type.is_empty() {
                continue;
            }

            let new_design_id = design_map
                .get(&old_design_id)
                .copied()
                .unwrap_or(old_design_id);
            let _ = s.design_template_upsert(new_design_id, template_type, layout_html, style_css);
        }
    }

    // ── 11. Settings (skip sensitive) ──
    let skip_keys = [
        "admin_password_hash",
        "setup_completed",
        "admin_email",
        "session_secret",
        "image_proxy_secret",
        "image_proxy_secret_old",
        "image_proxy_secret_old_expires",
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

    counts
}

fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}
