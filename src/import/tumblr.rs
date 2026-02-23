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
        "https://api.tumblr.com/v2/blog/{}/posts?api_key={}&offset={}&limit={}&reblog_info=true",
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
                if let Some(item) = import_photo_post(store, post, &tags, date) {
                    items.push(item);
                } else {
                    skipped += 1;
                }
            }
            "video" if video_enabled => {
                if let Some(item) = import_video_post(store, post, &tags, date) {
                    items.push(item);
                } else {
                    skipped += 1;
                }
            }
            "video" => {
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

    let post_dt = parse_tumblr_date_to_naive(date);

    // Download all images in body HTML and rewrite URLs to local paths
    let (body_rewritten, downloaded_images) =
        download_and_rewrite_body_images(body, store, "post", post_dt.as_ref());

    // First downloaded image becomes the featured image
    let featured_image = downloaded_images.first().cloned();

    // Generate title: use Tumblr title, or first tag title-cased, or first 60 chars of body
    let title = if !tumblr_title.is_empty() {
        tumblr_title.to_string()
    } else {
        fallback_title_text(tags, body, date)
    };

    let slug = unique_slug(&title, |s| store.post_find_by_slug(s).is_some());

    let published_at = parse_tumblr_date(date);

    // Remove the featured image from body HTML to avoid duplication
    let body_final = if let Some(ref fi) = featured_image {
        remove_first_img_with_src(&body_rewritten, fi)
    } else {
        body_rewritten
    };

    let meta_desc = {
        let plain = strip_html(&body_final);
        if plain.len() > 5 {
            Some(plain.chars().take(160).collect::<String>())
        } else {
            None
        }
    };

    let form = PostForm {
        title: title.clone(),
        slug: slug.clone(),
        content_json: "{}".to_string(),
        content_html: body_final,
        excerpt: None,
        featured_image: featured_image.clone(),
        meta_title: Some(title.clone()),
        meta_description: meta_desc,
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

    // Get photo URLs — try legacy "photos" array first, then NPF content blocks,
    // then fall back to extracting <img> URLs from body HTML
    let mut photo_urls: Vec<String> =
        if let Some(photos) = post.get("photos").and_then(|v| v.as_array()) {
            photos.iter().filter_map(best_photo_url).collect()
        } else if let Some(content) = post.get("content").and_then(|c| c.as_array()) {
            let urls: Vec<String> = content
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
                .collect();
            urls
        } else {
            vec![]
        };

    // Fallback: extract image URLs from body HTML (NPF flattened to legacy)
    if photo_urls.is_empty() {
        let body = post.get("body").and_then(|v| v.as_str()).unwrap_or("");
        photo_urls = extract_all_img_srcs(body)
            .into_iter()
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
            .collect();
    }

    if photo_urls.is_empty() {
        return None;
    }

    let post_dt = parse_tumblr_date_to_naive(date);

    // Download first photo as featured image
    let image_path = download_media(&photo_urls[0], store, "portfolio", post_dt.as_ref()).ok()?;

    // Build description HTML with remaining photos
    let mut desc_html = String::new();
    if !caption.is_empty() {
        desc_html.push_str(caption);
    }
    let title = fallback_title_media(tags, "Photo", date);

    if photo_urls.len() > 1 {
        desc_html.push_str("<div class=\"tumblr-photoset\">");
        for url in &photo_urls[1..] {
            if let Ok(local) = download_media(url, store, "portfolio", post_dt.as_ref()) {
                desc_html.push_str(&format!(
                    "<img src=\"/uploads/{}\" alt=\"{}\" loading=\"lazy\">",
                    local,
                    html_escape(&title)
                ));
            }
        }
        desc_html.push_str("</div>");
    }

    let slug = unique_slug(&title, |s| store.portfolio_find_by_slug(s).is_some());

    let published_at = parse_tumblr_date(date);

    let meta_desc = if !caption.is_empty() {
        Some(strip_html(&caption.chars().take(160).collect::<String>()))
    } else {
        None
    };

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
        meta_title: Some(title.clone()),
        meta_description: meta_desc,
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
) -> Option<TumblrImportedItem> {
    let caption = post.get("caption").and_then(|v| v.as_str()).unwrap_or("");
    let tumblr_url = post
        .get("post_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let post_dt = parse_tumblr_date_to_naive(date);

    // Try to get direct video URL — legacy field first, then NPF content blocks
    let video_url = post
        .get("video_url")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
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

    // Step 1: Try downloading the video file directly
    let mut image_path = None;
    if let Some(ref url) = video_url {
        image_path = download_media(url, store, "portfolio", post_dt.as_ref()).ok();
    }

    // Step 2: If no video file, try thumbnail_url
    if image_path.is_none() {
        image_path = post
            .get("thumbnail_url")
            .and_then(|v| v.as_str())
            .and_then(|url| download_media(url, store, "portfolio", post_dt.as_ref()).ok());
    }

    // Step 3: NPF poster image
    if image_path.is_none() {
        image_path = post
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("video"))
                    .and_then(|b| b.get("poster").and_then(|p| p.as_array()))
                    .and_then(|posters| posters.first())
                    .and_then(|p| p.get("url").and_then(|u| u.as_str()))
                    .and_then(|url| download_media(url, store, "portfolio", post_dt.as_ref()).ok())
            });
    }

    // Get embed HTML from Tumblr's player field (for YouTube/Vimeo embeds)
    let embed_html = post
        .get("player")
        .and_then(|p| {
            // player can be an array of objects with embed_code, or a string
            if let Some(arr) = p.as_array() {
                // Pick the largest embed (last in array)
                arr.iter()
                    .rev()
                    .find_map(|item| item.get("embed_code").and_then(|e| e.as_str()))
                    .map(String::from)
            } else {
                p.as_str().map(String::from)
            }
        })
        .or_else(|| {
            // NPF: look for video block embed_html or embed_url
            post.get("content")
                .and_then(|c| c.as_array())
                .and_then(|blocks| {
                    blocks
                        .iter()
                        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("video"))
                        .and_then(|b| {
                            b.get("embed_html")
                                .or_else(|| b.get("embed_url"))
                                .and_then(|e| e.as_str())
                                .map(String::from)
                        })
                })
        });

    // If we have no image AND no embed, this is truly unimportable
    if image_path.is_none() && embed_html.is_none() {
        return None;
    }

    // Build description: embed HTML + caption
    let mut desc_parts = Vec::new();
    if let Some(ref embed) = embed_html {
        desc_parts.push(embed.clone());
    }
    if !caption.is_empty() {
        desc_parts.push(caption.to_string());
    }
    let desc_html = if desc_parts.is_empty() {
        None
    } else {
        Some(desc_parts.join("\n"))
    };

    // For embedded videos without a downloaded file, import as a Journal post instead
    // since portfolio requires an image_path
    if image_path.is_none() {
        let title = fallback_title_media(tags, "Video", date);
        let slug = unique_slug(&title, |s| store.post_find_by_slug(s).is_some());
        let published_at = parse_tumblr_date(date);

        let meta_desc = if !caption.is_empty() {
            Some(strip_html(&caption.chars().take(160).collect::<String>()))
        } else {
            None
        };

        let form = PostForm {
            title: title.clone(),
            slug: slug.clone(),
            content_json: String::new(),
            content_html: desc_html.unwrap_or_default(),
            excerpt: None,
            featured_image: None,
            meta_title: Some(title.clone()),
            meta_description: meta_desc,
            status: "published".to_string(),
            published_at,
            category_ids: None,
            tag_ids: None,
        };

        let item_id = store.post_create(&form).ok()?;

        return Some(TumblrImportedItem {
            id: item_id,
            item_type: "journal".to_string(),
            title,
            slug,
            image_path: String::new(),
            tumblr_url,
            tags: tags.to_vec(),
        });
    }

    let image_path = image_path.unwrap_or_default();
    let title = fallback_title_media(tags, "Video", date);
    let slug = unique_slug(&title, |s| store.portfolio_find_by_slug(s).is_some());
    let published_at = parse_tumblr_date(date);

    let meta_desc = if !caption.is_empty() {
        Some(strip_html(&caption.chars().take(160).collect::<String>()))
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
        meta_title: Some(title.clone()),
        meta_description: meta_desc,
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
                    let new_title = update.title.clone().unwrap_or_else(|| post.title.clone());
                    let new_slug = if update.title.is_some() {
                        let own_slug = post.slug.clone();
                        unique_slug(&new_title, |s| {
                            s != own_slug && store.post_find_by_slug(s).is_some()
                        })
                    } else {
                        post.slug.clone()
                    };
                    // Rewrite empty alt attrs in content HTML to use the new title
                    let new_content_html = rewrite_empty_img_alts(&post.content_html, &new_title);
                    let form = PostForm {
                        title: new_title,
                        slug: new_slug,
                        content_json: post.content_json,
                        content_html: new_content_html,
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
                    let new_title = update.title.clone().unwrap_or_else(|| item.title.clone());
                    let new_slug = if update.title.is_some() {
                        let own_slug = item.slug.clone();
                        unique_slug(&new_title, |s| {
                            s != own_slug && store.portfolio_find_by_slug(s).is_some()
                        })
                    } else {
                        item.slug.clone()
                    };

                    // Build description_html: prepend AI description, keep existing images
                    let mut new_desc = String::new();
                    if let Some(ref desc) = update.meta_description {
                        if !desc.is_empty() {
                            new_desc.push_str(&format!("<p>{}</p>", html_escape(desc)));
                        }
                    }
                    // Append existing photoset/image HTML (strip any old text-only content)
                    if let Some(ref existing) = item.description_html {
                        // Keep everything from the first <div or <img onward (photoset images)
                        let lower = existing.to_lowercase();
                        let img_start = lower.find("<div").or_else(|| lower.find("<img"));
                        if let Some(pos) = img_start {
                            new_desc.push_str(&existing[pos..]);
                        } else if new_desc.is_empty() {
                            // No images and no AI description — keep original
                            new_desc = existing.clone();
                        }
                    }
                    // Rewrite empty alt attrs to use the new title
                    let new_desc = rewrite_empty_img_alts(&new_desc, &new_title);

                    let form = PortfolioForm {
                        title: new_title,
                        slug: new_slug,
                        description_json: item.description_json,
                        description_html: if new_desc.is_empty() {
                            None
                        } else {
                            Some(new_desc)
                        },
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

/// Generate a unique slug by appending -2, -3, etc. if the base slug already exists.
fn unique_slug<F>(title: &str, exists: F) -> String
where
    F: Fn(&str) -> bool,
{
    let base = slug::slugify(title);
    if !exists(&base) {
        return base;
    }
    for i in 2..=999 {
        let candidate = format!("{}-{}", base, i);
        if !exists(&candidate) {
            return candidate;
        }
    }
    // Extremely unlikely fallback
    format!("{}-{}", base, chrono::Utc::now().timestamp_millis())
}

/// Remove the first `<img>` tag whose `src` contains the given path from HTML.
fn remove_first_img_with_src(html: &str, src_fragment: &str) -> String {
    // Build the /uploads/ version of the path to match in HTML
    let search = format!("/uploads/{}", src_fragment);
    let lower = html.to_lowercase();
    let search_lower = search.to_lowercase();

    let mut pos = 0;
    while let Some(img_start) = lower[pos..].find("<img ") {
        let abs_start = pos + img_start;
        // Find the end of this <img> tag
        if let Some(tag_end_rel) = html[abs_start..].find('>') {
            let tag = &html[abs_start..abs_start + tag_end_rel + 1];
            if tag.to_lowercase().contains(&search_lower) {
                // Remove this entire <img> tag
                let mut result = html[..abs_start].to_string();
                result.push_str(&html[abs_start + tag_end_rel + 1..]);
                return result;
            }
            pos = abs_start + tag_end_rel + 1;
        } else {
            break;
        }
    }
    html.to_string()
}

/// Download all images found in HTML body and rewrite their src URLs to local paths.
/// Returns the rewritten HTML and a list of local paths for downloaded images.
fn download_and_rewrite_body_images(
    html: &str,
    store: &dyn Store,
    prefix: &str,
    post_date: Option<&chrono::NaiveDateTime>,
) -> (String, Vec<String>) {
    let mut result = html.to_string();
    let mut downloaded: Vec<String> = Vec::new();

    // Find all img src URLs in the HTML
    let img_urls = extract_all_img_srcs(html);
    for remote_url in &img_urls {
        // Only download remote URLs (http/https)
        if !remote_url.starts_with("http://") && !remote_url.starts_with("https://") {
            continue;
        }
        if let Ok(local_path) = download_media(remote_url, store, prefix, post_date) {
            // HTML src needs /uploads/ prefix; DB stores without it
            let html_path = format!("/uploads/{}", local_path);
            result = result.replace(remote_url, &html_path);
            downloaded.push(local_path);
        }
    }

    (result, downloaded)
}

/// Extract all `src` attributes from `<img>` tags in HTML.
fn extract_all_img_srcs(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(img_pos) = lower[search_from..].find("<img ") {
        let abs_pos = search_from + img_pos;
        let after_img = &html[abs_pos..];
        if let Some(src_pos) = after_img.to_lowercase().find("src=") {
            let after_src = &after_img[src_pos + 4..];
            if let Some(quote) = after_src.chars().next() {
                if quote == '"' || quote == '\'' {
                    let rest = &after_src[1..];
                    if let Some(end) = rest.find(quote) {
                        let url = &rest[..end];
                        if !url.is_empty() {
                            urls.push(url.to_string());
                        }
                    }
                }
            }
        }
        search_from = abs_pos + 5;
    }
    urls
}

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

    // Check body HTML for embedded media (NPF posts flattened to legacy format)
    if let Some(body) = post.get("body").and_then(|v| v.as_str()) {
        let lower = body.to_lowercase();
        if lower.contains("<video") || lower.contains("<iframe") {
            return "video".to_string();
        }
        if lower.contains("<img ") {
            return "photo".to_string();
        }
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

/// Download a media file from a URL into the uploads directory, respecting
/// the `media_organization` setting. `prefix` is "post" or "portfolio".
/// `post_date` is the original post date for date-based organization.
/// Returns the relative path (e.g. "2026/02/post_abc.jpg") for DB storage.
fn download_media(
    url: &str,
    store: &dyn Store,
    prefix: &str,
    post_date: Option<&chrono::NaiveDateTime>,
) -> Result<String, String> {
    let orig_filename = url.rsplit('/').next().unwrap_or("media").to_string();
    let orig_filename = orig_filename
        .split('?')
        .next()
        .unwrap_or(&orig_filename)
        .to_string();
    let ext = orig_filename
        .rsplit('.')
        .next()
        .unwrap_or("jpg")
        .to_lowercase();

    // Compute subdir from media organization setting
    let subdir = if let Some(dt) = post_date {
        crate::routes::admin::media_subdir_for_date(store, prefix, dt)
    } else {
        crate::routes::admin::media_subdir(store, prefix)
    };

    let uid = uuid::Uuid::new_v4();
    let rel_path = format!("{}{}_{}.{}", subdir, prefix, uid, ext);
    let upload_dir = Path::new("website/site/uploads");
    let full_dir = upload_dir.join(&subdir);
    let _ = std::fs::create_dir_all(&full_dir);
    let dest_path = upload_dir.join(&rel_path);

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

    Ok(rel_path)
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

/// Parse Tumblr date string into NaiveDateTime for media organization.
fn parse_tumblr_date_to_naive(date: &str) -> Option<chrono::NaiveDateTime> {
    if date.is_empty() {
        return None;
    }
    let cleaned = date.replace(" GMT", "").replace(" UTC", "");
    chrono::NaiveDateTime::parse_from_str(&cleaned, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&cleaned, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&cleaned, "%Y-%m-%dT%H:%M"))
        .ok()
}

fn parse_tumblr_date(date: &str) -> Option<String> {
    if date.is_empty() {
        return None;
    }
    // Tumblr format: "2024-01-15 10:30:00 GMT"
    // Must work with both SQLite (%Y-%m-%dT%H:%M) and MongoDB (%Y-%m-%dT%H:%M:%S)
    let cleaned = date.replace(" GMT", "").replace(" UTC", "");
    let result = if cleaned.len() >= 19 {
        // "2024-01-15 10:30:00" → "2024-01-15T10:30:00"
        let d = &cleaned[..10];
        let t = &cleaned[11..19];
        Some(format!("{}T{}", d, t))
    } else if cleaned.len() >= 16 {
        let d = &cleaned[..10];
        let t = &cleaned[11..16];
        Some(format!("{}T{}:00", d, t))
    } else if cleaned.len() >= 10 {
        Some(format!("{}T00:00:00", &cleaned[..10]))
    } else {
        None
    };
    result
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

/// Rewrite `alt=""` attributes on `<img>` tags to use the given alt text.
fn rewrite_empty_img_alts(html: &str, alt_text: &str) -> String {
    let escaped = html_escape(alt_text);
    html.replace("alt=\"\"", &format!("alt=\"{}\"", escaped))
        .replace("alt=''", &format!("alt='{}'", escaped))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
