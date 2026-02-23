use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::models::category::CategoryForm;
use crate::models::comment::CommentForm;
use crate::models::post::PostForm;
use crate::store::Store;

/// Result of a WordPress import
pub struct WpImportResult {
    pub posts_imported: i64,
    pub portfolio_imported: i64,
    pub comments_imported: i64,
    pub skipped: i64,
    pub media_downloaded: i64,
    pub media_failed: i64,
    pub log: Vec<String>,
}

/// Parsed WP comment with parent_id for threading
struct WpComment {
    wp_id: i64,
    parent_id: i64,
    author: String,
    email: String,
    content: String,
    status: String,
    date: String,
}

/// Parsed WP item (post or attachment)
struct WpItem {
    wp_id: i64,
    title: String,
    content: String,
    excerpt: String,
    slug: String,
    status: String,
    post_type: String,
    date: String,
    categories: Vec<String>,
    tags: Vec<String>,
    comments: Vec<WpComment>,
    meta: Vec<(String, String)>,
    attachment_url: String,
}

/// Parse and import a WordPress WXR XML export file
pub fn import_wxr(store: &dyn Store, xml_content: &str) -> Result<WpImportResult, String> {
    let mut result = WpImportResult {
        posts_imported: 0,
        portfolio_imported: 0,
        comments_imported: 0,
        skipped: 0,
        media_downloaded: 0,
        media_failed: 0,
        log: Vec::new(),
    };

    // Phase 1: Parse all items from XML
    let items = parse_wxr_items(xml_content)?;

    // Phase 2: Build attachment map (wp_post_id -> attachment_url)
    let mut attachment_map: HashMap<i64, String> = HashMap::new();
    for item in &items {
        if item.post_type == "attachment" && !item.attachment_url.is_empty() {
            attachment_map.insert(item.wp_id, item.attachment_url.clone());
        }
    }

    // Phase 3: Download media and import posts
    let media_dir = Path::new("website/site/uploads/wp-import");
    let _ = std::fs::create_dir_all(media_dir);

    for item in &items {
        match item.post_type.as_str() {
            "post" => {
                // Find featured image via _thumbnail_id meta
                let featured_image = find_featured_image(item, &attachment_map);
                let featured_local = if let Some(ref url) = featured_image {
                    match download_media(url, media_dir) {
                        Ok(local) => {
                            result.media_downloaded += 1;
                            Some(local)
                        }
                        Err(e) => {
                            result.media_failed += 1;
                            result
                                .log
                                .push(format!("Media failed for '{}': {}", item.title, e));
                            None
                        }
                    }
                } else {
                    None
                };

                // Rewrite inline wp-content/uploads URLs in content
                let content = rewrite_inline_images(&item.content, media_dir, &mut result);

                match import_post(
                    store,
                    &item.title,
                    &item.slug,
                    &content,
                    &item.excerpt,
                    &item.status,
                    &item.date,
                    &item.categories,
                    &item.tags,
                    &item.comments,
                    featured_local.as_deref(),
                ) {
                    Ok(_) => {
                        result.posts_imported += 1;
                        result.comments_imported += item.comments.len() as i64;
                        result.log.push(format!("Imported post: {}", item.title));
                    }
                    Err(e) => {
                        result.skipped += 1;
                        result
                            .log
                            .push(format!("Skipped post '{}': {}", item.title, e));
                    }
                }
            }
            "page" | "attachment" | "nav_menu_item" | "revision" => {}
            _ => {
                result.skipped += 1;
            }
        }
    }

    // Record import in history
    let log_json = serde_json::to_string(&result.log).unwrap_or_default();
    let _ = store.import_create(
        "wordpress",
        None,
        result.posts_imported,
        result.portfolio_imported,
        result.comments_imported,
        result.skipped,
        Some(&log_json),
    );

    Ok(result)
}

fn parse_wxr_items(xml_content: &str) -> Result<Vec<WpItem>, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut items: Vec<WpItem> = Vec::new();
    let mut buf = Vec::new();
    let mut current_element = String::new();
    let mut in_item = false;

    // Current item fields
    let mut item_wp_id: i64 = 0;
    let mut item_title = String::new();
    let mut item_content = String::new();
    let mut item_excerpt = String::new();
    let mut item_slug = String::new();
    let mut item_status = String::new();
    let mut item_post_type = String::new();
    let mut item_date = String::new();
    let mut item_categories: Vec<String> = Vec::new();
    let mut item_tags: Vec<String> = Vec::new();
    let mut item_attachment_url = String::new();
    let mut item_meta: Vec<(String, String)> = Vec::new();
    let mut meta_key = String::new();
    let mut meta_value = String::new();
    let mut in_postmeta = false;

    // Comment fields
    let mut in_comment = false;
    let mut comment_wp_id: i64 = 0;
    let mut comment_parent: i64 = 0;
    let mut comment_author = String::new();
    let mut comment_email = String::new();
    let mut comment_content = String::new();
    let mut comment_approved = String::new();
    let mut comment_date = String::new();
    let mut item_comments: Vec<WpComment> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.clone();

                if name == "item" {
                    in_item = true;
                    item_wp_id = 0;
                    item_title.clear();
                    item_content.clear();
                    item_excerpt.clear();
                    item_slug.clear();
                    item_status.clear();
                    item_post_type.clear();
                    item_date.clear();
                    item_categories.clear();
                    item_tags.clear();
                    item_comments.clear();
                    item_attachment_url.clear();
                    item_meta.clear();
                }

                if name == "wp:postmeta" {
                    in_postmeta = true;
                    meta_key.clear();
                    meta_value.clear();
                }

                if name == "wp:comment" {
                    in_comment = true;
                    comment_wp_id = 0;
                    comment_parent = 0;
                    comment_author.clear();
                    comment_email.clear();
                    comment_content.clear();
                    comment_approved.clear();
                    comment_date.clear();
                }

                if in_item && name == "category" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        if key == "domain" {
                            if val == "category" {
                                current_element = "wp:item_category".to_string();
                            } else if val == "post_tag" {
                                current_element = "wp:item_tag".to_string();
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();

                if in_comment {
                    match current_element.as_str() {
                        "wp:comment_id" => comment_wp_id = text.parse().unwrap_or(0),
                        "wp:comment_parent" => comment_parent = text.parse().unwrap_or(0),
                        "wp:comment_author" => comment_author = text,
                        "wp:comment_author_email" => comment_email = text,
                        "wp:comment_content" => comment_content = text,
                        "wp:comment_approved" => comment_approved = text,
                        "wp:comment_date" => comment_date = text,
                        _ => {}
                    }
                } else if in_postmeta {
                    match current_element.as_str() {
                        "wp:meta_key" => meta_key = text,
                        "wp:meta_value" => meta_value = text,
                        _ => {}
                    }
                } else if in_item {
                    match current_element.as_str() {
                        "title" => item_title = text,
                        "content:encoded" => item_content = text,
                        "excerpt:encoded" => item_excerpt = text,
                        "wp:post_id" => item_wp_id = text.parse().unwrap_or(0),
                        "wp:post_name" => item_slug = text,
                        "wp:status" => item_status = text,
                        "wp:post_type" => item_post_type = text,
                        "wp:post_date" => item_date = text,
                        "wp:attachment_url" => item_attachment_url = text,
                        "wp:item_category" => item_categories.push(text),
                        "wp:item_tag" => item_tags.push(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();

                if in_comment {
                    match current_element.as_str() {
                        "wp:comment_content" => comment_content = text,
                        "wp:comment_author" => comment_author = text,
                        _ => {}
                    }
                } else if in_postmeta {
                    if current_element == "wp:meta_value" {
                        meta_value = text;
                    }
                } else if in_item {
                    match current_element.as_str() {
                        "content:encoded" => item_content = text,
                        "excerpt:encoded" => item_excerpt = text,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if name == "wp:postmeta" && in_postmeta {
                    in_postmeta = false;
                    if !meta_key.is_empty() {
                        item_meta.push((meta_key.clone(), meta_value.clone()));
                    }
                }

                if name == "wp:comment" && in_comment {
                    in_comment = false;
                    // Import all comments (approved + pending), not just approved
                    let status = match comment_approved.as_str() {
                        "1" | "approve" => "approved",
                        "0" | "hold" => "pending",
                        "spam" => "spam",
                        _ => "pending",
                    };
                    item_comments.push(WpComment {
                        wp_id: comment_wp_id,
                        parent_id: comment_parent,
                        author: comment_author.clone(),
                        email: comment_email.clone(),
                        content: comment_content.clone(),
                        status: status.to_string(),
                        date: comment_date.clone(),
                    });
                }

                if name == "item" && in_item {
                    in_item = false;
                    items.push(WpItem {
                        wp_id: item_wp_id,
                        title: item_title.clone(),
                        content: item_content.clone(),
                        excerpt: item_excerpt.clone(),
                        slug: item_slug.clone(),
                        status: item_status.clone(),
                        post_type: item_post_type.clone(),
                        date: item_date.clone(),
                        categories: item_categories.clone(),
                        tags: item_tags.clone(),
                        comments: std::mem::take(&mut item_comments),
                        meta: item_meta.clone(),
                        attachment_url: item_attachment_url.clone(),
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(items)
}

fn find_featured_image(item: &WpItem, attachment_map: &HashMap<i64, String>) -> Option<String> {
    for (key, value) in &item.meta {
        if key == "_thumbnail_id" {
            if let Ok(att_id) = value.parse::<i64>() {
                return attachment_map.get(&att_id).cloned();
            }
        }
    }
    None
}

fn download_media(url: &str, dest_dir: &Path) -> Result<String, String> {
    let filename = url.rsplit('/').next().unwrap_or("image.jpg").to_string();
    let dest_path = dest_dir.join(&filename);

    // Skip if already downloaded
    if dest_path.exists() {
        return Ok(format!("/uploads/wp-import/{}", filename));
    }

    let resp = reqwest::blocking::get(url).map_err(|e| format!("Download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| format!("Read failed: {}", e))?;

    let mut file = std::fs::File::create(&dest_path).map_err(|e| format!("Write failed: {}", e))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Write failed: {}", e))?;

    Ok(format!("/uploads/wp-import/{}", filename))
}

fn rewrite_inline_images(content: &str, dest_dir: &Path, result: &mut WpImportResult) -> String {
    let mut output = content.to_string();

    // Find all wp-content/uploads URLs in the HTML
    let re_pattern = "https?://[^\"'\\s]+/wp-content/uploads/[^\"'\\s]+";
    if let Ok(re) = regex::Regex::new(re_pattern) {
        let urls: Vec<String> = re
            .find_iter(content)
            .map(|m| m.as_str().to_string())
            .collect();
        for url in urls {
            match download_media(&url, dest_dir) {
                Ok(local_path) => {
                    output = output.replace(&url, &local_path);
                    result.media_downloaded += 1;
                }
                Err(_) => {
                    result.media_failed += 1;
                }
            }
        }
    }

    output
}

#[allow(clippy::too_many_arguments)]
fn import_post(
    store: &dyn Store,
    title: &str,
    slug: &str,
    content: &str,
    excerpt: &str,
    status: &str,
    date: &str,
    categories: &[String],
    tags: &[String],
    comments: &[WpComment],
    featured_image: Option<&str>,
) -> Result<i64, String> {
    if title.is_empty() || slug.is_empty() {
        return Err("Missing title or slug".to_string());
    }

    if store.post_find_by_slug(slug).is_some() {
        return Err("Duplicate slug".to_string());
    }

    let velocty_status = match status {
        "publish" => "published",
        "draft" => "draft",
        "private" => "draft",
        "pending" => "draft",
        _ => "draft",
    };

    let form = PostForm {
        title: title.to_string(),
        slug: slug.to_string(),
        content_json: "{}".to_string(),
        content_html: content.to_string(),
        excerpt: if excerpt.is_empty() {
            None
        } else {
            Some(excerpt.to_string())
        },
        featured_image: featured_image.map(|s| s.to_string()),
        meta_title: None,
        meta_description: None,
        status: velocty_status.to_string(),
        published_at: if !date.is_empty() {
            Some(date.to_string())
        } else {
            None
        },
        category_ids: None,
        tag_ids: None,
    };

    let post_id = store.post_create(&form)?;

    // Import categories
    for cat_name in categories {
        let cat_slug = slug::slugify(cat_name);
        let cat_id = match store.category_find_by_slug(&cat_slug) {
            Some(c) => c.id,
            None => store.category_create(&CategoryForm {
                name: cat_name.clone(),
                slug: cat_slug,
                r#type: "post".to_string(),
            })?,
        };
        store.category_set_for_content(post_id, "post", &[cat_id])?;
    }

    // Import tags
    for tag_name in tags {
        let tag_id = store.tag_find_or_create(tag_name)?;
        store.tag_set_for_content(post_id, "post", &[tag_id])?;
    }

    // Import comments with threading (two-pass)
    let mut wp_comment_map: HashMap<i64, i64> = HashMap::new(); // wp_comment_id -> velocty_id
    for c in comments {
        let comment_form = CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: c.author.clone(),
            author_email: if c.email.is_empty() {
                None
            } else {
                Some(c.email.clone())
            },
            body: c.content.clone(),
            honeypot: None,
            parent_id: None,
        };
        if let Ok(cid) = store.comment_create(&comment_form) {
            let _ = store.comment_update_status(cid, &c.status);
            if c.wp_id > 0 {
                wp_comment_map.insert(c.wp_id, cid);
            }
        }
    }
    // Second pass: set parent_id for threaded comments
    for c in comments {
        if c.parent_id > 0 {
            if let (Some(&new_id), Some(&new_parent)) = (
                wp_comment_map.get(&c.wp_id),
                wp_comment_map.get(&c.parent_id),
            ) {
                let _ = store.comment_set_parent(new_id, Some(new_parent));
            }
        }
    }

    Ok(post_id)
}
