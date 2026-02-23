use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::category::CategoryForm;
use crate::models::portfolio::PortfolioForm;
use crate::models::post::PostForm;
use crate::store::Store;

// ── Public Types ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TumblrConfig {
    pub api_key: String,
    pub blog_url: String,
}

#[derive(Debug, Serialize)]
pub struct TumblrStartResult {
    pub total_posts: u64,
    pub blog_title: String,
    pub blog_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TumblrImportedItem {
    pub id: i64,
    pub item_type: String, // "journal", "photo", "video"
    pub title: String,
    pub slug: String,
    pub image_path: String,
    pub tumblr_url: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TumblrPageResult {
    pub items: Vec<TumblrImportedItem>,
    pub offset: u64,
    pub has_more: bool,
    pub skipped: u64,
}

// ── Validate & Count ─────────────────────────────────

pub fn validate_and_count(config: &TumblrConfig) -> Result<TumblrStartResult, String> {
    let blog = normalize_blog(&config.blog_url);
    let url = format!(
        "https://api.tumblr.com/v2/blog/{}/info?api_key={}",
        blog, config.api_key
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("Tumblr API request failed: {}", e))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let msg = match code {
            401 => "Invalid API key. Please check your Tumblr configuration.".to_string(),
            404 => "Blog not found. Please check the blog URL in your configuration.".to_string(),
            429 => "Rate limited by Tumblr. Please wait a moment and try again.".to_string(),
            _ => format!("Tumblr API error ({}). Please try again later.", code),
        };
        return Err(msg);
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("Failed to parse Tumblr response: {}", e))?;

    let blog_info = json
        .get("response")
        .and_then(|r| r.get("blog"))
        .ok_or("Invalid Tumblr API response: missing blog info")?;

    let total_posts = blog_info
        .get("total_posts")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let blog_title = blog_info
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let blog_description = blog_info
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(TumblrStartResult {
        total_posts,
        blog_title,
        blog_description,
    })
}

// ── Fetch & Import One Page ──────────────────────────

pub fn import_page(
    store: &dyn Store,
    config: &TumblrConfig,
    offset: u64,
    settings: &HashMap<String, String>,
) -> Result<TumblrPageResult, String> {
    let blog = normalize_blog(&config.blog_url);
    let limit = 20u64;
    let url = format!(
        "https://api.tumblr.com/v2/blog/{}/posts?api_key={}&offset={}&limit={}&reblog_info=true&npf_flat=true",
        blog, config.api_key, offset, limit
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("Tumblr API request failed: {}", e))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let msg = match code {
            401 => "Invalid API key. Please check your Tumblr configuration.".to_string(),
            404 => "Blog not found. Please check the blog URL.".to_string(),
            429 => "Rate limited by Tumblr. Please wait and try again.".to_string(),
            _ => format!("Tumblr API error ({}). Please try again later.", code),
        };
        return Err(msg);
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("Failed to parse Tumblr response: {}", e))?;

    let total = json
        .get("response")
        .and_then(|r| r.get("total_posts"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let posts = json
        .get("response")
        .and_then(|r| r.get("posts"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let video_enabled = settings
        .get("video_upload_enabled")
        .map(|v| v == "true")
        .unwrap_or(false);

    let media_dir = Path::new("website/site/uploads/tumblr-import");
    let _ = std::fs::create_dir_all(media_dir);

    let mut items: Vec<TumblrImportedItem> = Vec::new();
    let mut skipped = 0u64;

    for post in &posts {
        // Skip reblogs
        if is_reblog(post) {
            skipped += 1;
            continue;
        }

        let raw_type = post.get("type").and_then(|v| v.as_str()).unwrap_or("");
        // Detect effective type: NPF may return everything as "text" even for photo/video posts.
        // Check for photo/video content in the post data to reclassify.
        let post_type = detect_effective_type(raw_type, post);
        let _tumblr_url = post
            .get("post_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tags: Vec<String> = post
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let date = post.get("date").and_then(|v| v.as_str()).unwrap_or("");

        match post_type.as_str() {
            "photo" => {
                if let Some(item) = import_photo_post(store, post, &tags, date, media_dir) {
                    items.push(item);
                } else {
                    skipped += 1;
                }
            }
            "video" if video_enabled => {
                if let Some(item) = import_video_post(store, post, &tags, date, media_dir) {
                    items.push(item);
                } else {
                    skipped += 1;
                }
            }
            "video" => {
                // Video uploads disabled, skip
                skipped += 1;
            }
            "text" => {
                if let Some(item) = import_text_post(store, post, &tags, date) {
                    items.push(item);
                } else {
                    skipped += 1;
                }
            }
            _ => {
                skipped += 1;
            }
        }

        // Set tags for the imported item
        if let Some(last) = items.last() {
            let content_type = match last.item_type.as_str() {
                "journal" => "post",
                _ => "portfolio",
            };
            for tag_name in &tags {
                if let Ok(tag_id) = store.tag_find_or_create(tag_name) {
                    let _ = store.tag_set_for_content(last.id, content_type, &[tag_id]);
                }
            }
        }
    }

    let has_more = offset + limit < total;

    Ok(TumblrPageResult {
        items,
        offset,
        has_more,
        skipped,
    })
}

// ── Text Post → Journal ──────────────────────────────

fn import_text_post(
    store: &dyn Store,
    post: &Value,
    tags: &[String],
    date: &str,
) -> Option<TumblrImportedItem> {
    let tumblr_title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let body = post.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let tumblr_url = post
        .get("post_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if body.is_empty() {
        return None;
    }

    // Extract first image from body HTML as featured image
    let featured_image = extract_first_img_src(body);

    // Generate title: use Tumblr title, or first tag title-cased, or first 60 chars of body
    let title = if !tumblr_title.is_empty() {
        tumblr_title.to_string()
    } else {
        fallback_title_text(tags, body, date)
    };

    let slug = slug::slugify(&title);

    // Skip duplicate slugs
    if store.post_find_by_slug(&slug).is_some() {
        return None;
    }

    let published_at = parse_tumblr_date(date);

    let form = PostForm {
        title: title.clone(),
        slug: slug.clone(),
        content_json: "{}".to_string(),
        content_html: body.to_string(),
        excerpt: None,
        featured_image: featured_image.clone(),
        meta_title: None,
        meta_description: None,
        status: "published".to_string(),
        published_at,
        category_ids: None,
        tag_ids: None,
    };

    let post_id = store.post_create(&form).ok()?;

    Some(TumblrImportedItem {
        id: post_id,
        item_type: "journal".to_string(),
        title,
        slug,
        image_path: featured_image.unwrap_or_default(),
        tumblr_url,
        tags: tags.to_vec(),
    })
}

// ── Photo Post → Portfolio ───────────────────────────

fn import_photo_post(
    store: &dyn Store,
    post: &Value,
    tags: &[String],
    date: &str,
    media_dir: &Path,
) -> Option<TumblrImportedItem> {
    let caption = post
        .get("caption")
        .and_then(|v| v.as_str())
        .or_else(|| post.get("summary").and_then(|v| v.as_str()))
        .unwrap_or("");
    let tumblr_url = post
        .get("post_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Get photo URLs — try legacy "photos" array first, then NPF content blocks
    let photo_urls: Vec<String> =
        if let Some(photos) = post.get("photos").and_then(|v| v.as_array()) {
            photos.iter().filter_map(best_photo_url).collect()
        } else if let Some(content) = post.get("content").and_then(|c| c.as_array()) {
            content
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("image"))
                .filter_map(|b| {
                    b.get("media")
                        .and_then(|m| m.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|m| m.get("url"))
                        .and_then(|u| u.as_str())
                        .map(String::from)
                })
                .collect()
        } else {
            vec![]
        };

    if photo_urls.is_empty() {
        return None;
    }

    // Download first photo as featured image
    let image_path = download_media(&photo_urls[0], media_dir).ok()?;

    // Build description HTML with remaining photos
    let mut desc_html = String::new();
    if !caption.is_empty() {
        desc_html.push_str(caption);
    }
    if photo_urls.len() > 1 {
        desc_html.push_str("<div class=\"tumblr-photoset\">");
        for url in &photo_urls[1..] {
            if let Ok(local) = download_media(url, media_dir) {
                desc_html.push_str(&format!(
                    "<img src=\"{}\" alt=\"\" loading=\"lazy\">",
                    local
                ));
            }
        }
        desc_html.push_str("</div>");
    }

    let title = fallback_title_media(tags, "Photo", date);
    let slug = slug::slugify(&title);

    if store.portfolio_find_by_slug(&slug).is_some() {
        return None;
    }

    let published_at = parse_tumblr_date(date);

    let form = PortfolioForm {
        title: title.clone(),
        slug: slug.clone(),
        description_json: None,
        description_html: if desc_html.is_empty() {
            None
        } else {
            Some(desc_html)
        },
        image_path: image_path.clone(),
        thumbnail_path: None,
        meta_title: None,
        meta_description: None,
        sell_enabled: None,
        price: None,
        purchase_note: None,
        payment_provider: None,
        download_file_path: None,
        status: "published".to_string(),
        published_at,
        category_ids: None,
        tag_ids: None,
    };

    let item_id = store.portfolio_create(&form).ok()?;

    Some(TumblrImportedItem {
        id: item_id,
        item_type: "photo".to_string(),
        title,
        slug,
        image_path,
        tumblr_url,
        tags: tags.to_vec(),
    })
}

// ── Video Post → Portfolio ───────────────────────────

fn import_video_post(
    store: &dyn Store,
    post: &Value,
    tags: &[String],
    date: &str,
    media_dir: &Path,
) -> Option<TumblrImportedItem> {
    let caption = post.get("caption").and_then(|v| v.as_str()).unwrap_or("");
    let tumblr_url = post
        .get("post_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Try to get video URL — legacy field first, then NPF content blocks
    let video_url = post
        .get("video_url")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            // NPF: look for video block with media URL
            post.get("content")
                .and_then(|c| c.as_array())
                .and_then(|blocks| {
                    blocks
                        .iter()
                        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("video"))
                        .and_then(|b| {
                            b.get("media")
                                .and_then(|m| m.get("url"))
                                .or_else(|| b.get("url"))
                                .and_then(|u| u.as_str())
                                .map(String::from)
                        })
                })
        });

    // Download video file if available, or try thumbnail
    let image_path = if let Some(ref url) = video_url {
        download_media(url, media_dir).ok()
    } else {
        // Try thumbnail_url as fallback for the featured media
        post.get("thumbnail_url")
            .and_then(|v| v.as_str())
            .and_then(|url| download_media(url, media_dir).ok())
            .or_else(|| {
                // NPF: try poster image from video block
                post.get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|blocks| {
                        blocks
                            .iter()
                            .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("video"))
                            .and_then(|b| b.get("poster").and_then(|p| p.as_array()))
                            .and_then(|posters| posters.first())
                            .and_then(|p| p.get("url").and_then(|u| u.as_str()))
                            .and_then(|url| download_media(url, media_dir).ok())
                    })
            })
    };

    let image_path = image_path.unwrap_or_default();
    if image_path.is_empty() {
        return None;
    }

    let title = fallback_title_media(tags, "Video", date);
    let slug = slug::slugify(&title);

    if store.portfolio_find_by_slug(&slug).is_some() {
        return None;
    }

    let published_at = parse_tumblr_date(date);

    let desc_html = if !caption.is_empty() {
        Some(caption.to_string())
    } else {
        None
    };

    let form = PortfolioForm {
        title: title.clone(),
        slug: slug.clone(),
        description_json: None,
        description_html: desc_html,
        image_path: image_path.clone(),
        thumbnail_path: None,
        meta_title: None,
        meta_description: None,
        sell_enabled: None,
        price: None,
        purchase_note: None,
        payment_provider: None,
        download_file_path: None,
        status: "published".to_string(),
        published_at,
        category_ids: None,
        tag_ids: None,
    };

    let item_id = store.portfolio_create(&form).ok()?;

    Some(TumblrImportedItem {
        id: item_id,
        item_type: "video".to_string(),
        title,
        slug,
        image_path,
        tumblr_url,
        tags: tags.to_vec(),
    })
}

// ── AI Suggest ───────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AiSuggestion {
    pub id: i64,
    pub item_type: String,
    pub title: String,
    pub description: String,
    pub category: String,
}

pub fn suggest_for_item(
    store: &dyn Store,
    item_id: i64,
    item_type: &str,
    current_title: &str,
    tags: &[String],
    image_path: &str,
) -> Result<AiSuggestion, String> {
    let content_type = match item_type {
        "journal" => "post",
        _ => "portfolio",
    };

    // Get existing categories for context
    let existing_cats: Vec<String> = store
        .category_list(Some(content_type))
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let tags_str = tags.join(", ");
    let prompt = format!(
        "Analyze this imported Tumblr {} and suggest improvements.\n\
         Current title: \"{}\"\n\
         Tags: {}\n\
         Image path: {}\n\
         Existing categories: [{}]\n\n\
         Respond in JSON:\n\
         {{\n  \
           \"title\": \"A better, descriptive title\",\n  \
           \"description\": \"A short meta description (under 160 chars)\",\n  \
           \"category\": \"Best matching category from existing list, or suggest a new one\"\n\
         }}",
        item_type,
        current_title,
        if tags_str.is_empty() {
            "none".to_string()
        } else {
            tags_str
        },
        if image_path.is_empty() {
            "none"
        } else {
            image_path
        },
        existing_cats.join(", "),
    );

    // Try to load image for vision-capable providers
    let image_base64 = if !image_path.is_empty() {
        load_image_base64(image_path)
    } else {
        None
    };

    let req = crate::ai::AiRequest {
        system: "You are a content assistant for a CMS. Suggest a better title, \
                 a short meta description, and the best category for imported content. \
                 Always respond in valid JSON. Do not include markdown fences."
            .to_string(),
        prompt,
        max_tokens: Some(512),
        temperature: Some(0.7),
        image_base64,
    };

    let resp = crate::ai::complete(store, &req).map_err(|e| e.0)?;

    // Parse JSON from response
    let parsed: Value = crate::routes::ai::parse_json_from_text(&resp.text)
        .ok_or("Failed to parse AI response as JSON")?;

    Ok(AiSuggestion {
        id: item_id,
        item_type: item_type.to_string(),
        title: parsed
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(current_title)
            .to_string(),
        description: parsed
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        category: parsed
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

// ── Apply Suggestions ────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApplyUpdate {
    pub id: i64,
    pub item_type: String,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub category_name: Option<String>,
}

pub fn apply_updates(store: &dyn Store, updates: &[ApplyUpdate]) -> Result<u64, String> {
    let mut applied = 0u64;

    for update in updates {
        let content_type = match update.item_type.as_str() {
            "journal" => "post",
            _ => "portfolio",
        };

        // Update title + meta_description
        match content_type {
            "post" => {
                if let Some(post) = store.post_find_by_id(update.id) {
                    let form = PostForm {
                        title: update.title.clone().unwrap_or(post.title),
                        slug: post.slug,
                        content_json: post.content_json,
                        content_html: post.content_html,
                        excerpt: post.excerpt,
                        featured_image: post.featured_image,
                        meta_title: update.title.clone(),
                        meta_description: update.meta_description.clone(),
                        status: post.status,
                        published_at: post.published_at.map(|d| d.to_string()),
                        category_ids: None,
                        tag_ids: None,
                    };
                    let _ = store.post_update(update.id, &form);
                }
            }
            _ => {
                if let Some(item) = store.portfolio_find_by_id(update.id) {
                    let form = PortfolioForm {
                        title: update.title.clone().unwrap_or(item.title),
                        slug: item.slug,
                        description_json: item.description_json,
                        description_html: item.description_html,
                        image_path: item.image_path,
                        thumbnail_path: item.thumbnail_path,
                        meta_title: update.title.clone(),
                        meta_description: update.meta_description.clone(),
                        sell_enabled: Some(item.sell_enabled),
                        price: item.price,
                        purchase_note: Some(item.purchase_note),
                        payment_provider: Some(item.payment_provider),
                        download_file_path: Some(item.download_file_path),
                        status: item.status,
                        published_at: item.published_at.map(|d| d.to_string()),
                        category_ids: None,
                        tag_ids: None,
                    };
                    let _ = store.portfolio_update(update.id, &form);
                }
            }
        }

        // Set category if provided
        if let Some(ref cat_name) = update.category_name {
            if !cat_name.is_empty() {
                let cat_slug = slug::slugify(cat_name);
                let cat_id = match store.category_find_by_slug(&cat_slug) {
                    Some(c) => c.id,
                    None => {
                        match store.category_create(&CategoryForm {
                            name: cat_name.clone(),
                            slug: cat_slug,
                            r#type: content_type.to_string(),
                        }) {
                            Ok(id) => id,
                            Err(_) => continue,
                        }
                    }
                };
                let _ = store.category_set_for_content(update.id, content_type, &[cat_id]);
            }
        }

        applied += 1;
    }

    Ok(applied)
}

// ── Helpers ──────────────────────────────────────────

/// Extract the first `src` attribute from an `<img>` tag in HTML.
fn extract_first_img_src(html: &str) -> Option<String> {
    // Simple extraction: find <img ... src="..." ...>
    let lower = html.to_lowercase();
    let img_pos = lower.find("<img ")?;
    let after_img = &html[img_pos..];
    let src_pos = after_img.to_lowercase().find("src=")?;
    let after_src = &after_img[src_pos + 4..];
    let quote = after_src.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after_src[1..];
    let end = rest.find(quote)?;
    let url = &rest[..end];
    if url.is_empty() {
        return None;
    }
    Some(url.to_string())
}

/// Detect the effective post type. Tumblr's NPF format may return photo/video
/// posts as type "text" with image/video content blocks. We check for legacy
/// fields (photos, video_url) and NPF content blocks to reclassify.
fn detect_effective_type(raw_type: &str, post: &Value) -> String {
    // If the API already says photo or video, trust it
    if raw_type == "photo" || raw_type == "video" {
        return raw_type.to_string();
    }

    // Check legacy photo field: "photos" array
    if let Some(photos) = post.get("photos") {
        if photos.is_array() && !photos.as_array().unwrap_or(&vec![]).is_empty() {
            return "photo".to_string();
        }
    }

    // Check legacy video field: "video_url"
    if let Some(v) = post.get("video_url") {
        if v.is_string() && !v.as_str().unwrap_or("").is_empty() {
            return "video".to_string();
        }
    }

    // Check NPF content blocks for image or video types
    if let Some(content) = post.get("content").and_then(|c| c.as_array()) {
        let mut has_image = false;
        let mut has_video = false;
        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("image") => has_image = true,
                Some("video") => has_video = true,
                _ => {}
            }
        }
        if has_video {
            return "video".to_string();
        }
        if has_image {
            return "photo".to_string();
        }
    }

    // Check for photo_url fields (some legacy formats)
    if post.get("image_permalink").is_some() {
        return "photo".to_string();
    }

    raw_type.to_string()
}

fn normalize_blog(url: &str) -> String {
    let s = url.trim();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let s = s.strip_suffix('/').unwrap_or(s);
    // If it doesn't contain a dot, append .tumblr.com
    if !s.contains('.') {
        format!("{}.tumblr.com", s)
    } else {
        s.to_string()
    }
}

fn is_reblog(post: &Value) -> bool {
    // If reblogged_from_id exists and is non-null, it's a reblog
    if let Some(v) = post.get("reblogged_from_id") {
        return !v.is_null();
    }
    false
}

fn best_photo_url(photo: &Value) -> Option<String> {
    // Get the original size URL from alt_sizes or original_size
    photo
        .get("original_size")
        .and_then(|s| s.get("url"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            // Fallback: first alt_size (usually largest)
            photo
                .get("alt_sizes")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|s| s.get("url"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

fn download_media(url: &str, dest_dir: &Path) -> Result<String, String> {
    let filename = url.rsplit('/').next().unwrap_or("media").to_string();
    // Clean query params from filename
    let filename = filename.split('?').next().unwrap_or(&filename).to_string();
    let dest_path = dest_dir.join(&filename);

    // Skip if already downloaded
    if dest_path.exists() {
        return Ok(format!("/uploads/tumblr-import/{}", filename));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Download returned {}", resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| format!("Read failed: {}", e))?;

    let mut file = std::fs::File::create(&dest_path).map_err(|e| format!("Write failed: {}", e))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Write failed: {}", e))?;

    Ok(format!("/uploads/tumblr-import/{}", filename))
}

fn fallback_title_text(tags: &[String], body: &str, date: &str) -> String {
    if let Some(first_tag) = tags.first() {
        return title_case(first_tag);
    }
    // Strip HTML and take first 60 chars
    let plain = strip_html(body);
    if plain.len() > 5 {
        let truncated: String = plain.chars().take(60).collect();
        return if truncated.len() < plain.len() {
            format!("{}…", truncated.trim())
        } else {
            truncated.trim().to_string()
        };
    }
    format_date_title("Text", date)
}

fn fallback_title_media(tags: &[String], media_type: &str, date: &str) -> String {
    if let Some(first_tag) = tags.first() {
        return title_case(first_tag);
    }
    format_date_title(media_type, date)
}

fn format_date_title(prefix: &str, date: &str) -> String {
    // Tumblr dates: "2024-01-15 10:30:00 GMT"
    if date.len() >= 10 {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&date[..10], "%Y-%m-%d") {
            return format!("{} — {}", prefix, d.format("%b %d, %Y"));
        }
    }
    format!("{} — Untitled", prefix)
}

fn parse_tumblr_date(date: &str) -> Option<String> {
    if date.is_empty() {
        return None;
    }
    // Tumblr format: "2024-01-15 10:30:00 GMT"
    // We need: "2024-01-15 10:30:00" or similar
    let cleaned = date.replace(" GMT", "").replace(" UTC", "");
    if cleaned.len() >= 19 {
        Some(cleaned[..19].to_string())
    } else if cleaned.len() >= 10 {
        Some(format!("{} 00:00:00", &cleaned[..10]))
    } else {
        None
    }
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

fn load_image_base64(image_path: &str) -> Option<String> {
    // image_path is like /uploads/tumblr-import/photo.jpg
    let disk_path = format!("website/site{}", image_path);
    let bytes = std::fs::read(&disk_path).ok()?;
    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    ))
}
