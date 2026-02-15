use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ai::{self, prompts, AiRequest};
use crate::security::auth::EditorUser;
use crate::db::DbPool;
use crate::models::category::Category;
use crate::models::tag::Tag;

use super::parse_json_from_text;

// ── Request Types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SuggestMetaRequest {
    pub title: String,
    pub content_excerpt: Option<String>,
    pub content_type: Option<String>,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestTagsRequest {
    pub title: String,
    pub content_excerpt: Option<String>,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestCategoriesRequest {
    pub title: String,
    pub content_excerpt: Option<String>,
    pub content_type: Option<String>,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestSlugRequest {
    pub title: String,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestAltTextRequest {
    pub context: Option<String>,
    pub filename: String,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestTitleRequest {
    pub description: Option<String>,
    pub image_base64: Option<String>,
}

// ── Suggest Meta Title & Description ──────────────────

#[post("/ai/suggest-meta", format = "json", data = "<body>")]
pub fn suggest_meta(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestMetaRequest>,
) -> Json<Value> {
    let excerpt = body.content_excerpt.as_deref().unwrap_or("");
    let ctype = body.content_type.as_deref().unwrap_or("post");

    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_meta(&body.title, excerpt, ctype),
        max_tokens: Some(256),
        temperature: Some(0.7),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => Json(json!({
                    "ok": true,
                    "provider": resp.provider,
                    "meta_title": parsed.get("meta_title").and_then(|v| v.as_str()).unwrap_or(""),
                    "meta_description": parsed.get("meta_description").and_then(|v| v.as_str()).unwrap_or(""),
                })),
                None => {
                    // Fallback: use raw text as meta description
                    let raw = resp.text.trim();
                    let desc = if raw.len() > 155 { &raw[..155] } else { raw };
                    Json(json!({"ok": true, "provider": resp.provider, "meta_title": "", "meta_description": desc}))
                }
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Tags ──────────────────────────────────────

#[post("/ai/suggest-tags", format = "json", data = "<body>")]
pub fn suggest_tags(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestTagsRequest>,
) -> Json<Value> {
    let excerpt = body.content_excerpt.as_deref().unwrap_or("");

    // Get existing tags for context
    let existing_tags: Vec<String> = Tag::list(pool)
        .iter()
        .map(|t| t.name.clone())
        .collect();

    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_tags(&body.title, excerpt, &existing_tags),
        max_tokens: Some(256),
        temperature: Some(0.7),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let tags = parsed
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    Json(json!({"ok": true, "provider": resp.provider, "tags": tags}))
                }
                None => {
                    // Fallback: split raw text by commas, newlines, or bullets
                    let tags: Vec<String> = resp.text.lines()
                        .flat_map(|l| l.split(','))
                        .map(|t| t.trim().trim_matches('-').trim_matches('*').trim_matches('"').trim().to_string())
                        .filter(|t| !t.is_empty() && t.len() < 50 && !t.to_lowercase().contains("tag"))
                        .take(8)
                        .collect();
                    Json(json!({"ok": true, "provider": resp.provider, "tags": tags}))
                }
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Categories ────────────────────────────────

#[post("/ai/suggest-categories", format = "json", data = "<body>")]
pub fn suggest_categories(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestCategoriesRequest>,
) -> Json<Value> {
    let excerpt = body.content_excerpt.as_deref().unwrap_or("");
    let ctype = body.content_type.as_deref().unwrap_or("post");

    let existing_cats: Vec<String> = Category::list(pool, Some(ctype))
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_categories(&body.title, excerpt, &existing_cats),
        max_tokens: Some(256),
        temperature: Some(0.7),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let categories = parsed
                        .get("categories")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    Json(json!({"ok": true, "provider": resp.provider, "categories": categories}))
                }
                None => Json(json!({"ok": false, "error": "Failed to parse AI response"})),
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Slug ──────────────────────────────────────

#[post("/ai/suggest-slug", format = "json", data = "<body>")]
pub fn suggest_slug(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestSlugRequest>,
) -> Json<Value> {
    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_slug(&body.title),
        max_tokens: Some(128),
        temperature: Some(0.5),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let slug = parsed
                        .get("slug")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Json(json!({"ok": true, "provider": resp.provider, "slug": slug}))
                }
                None => {
                    // Fallback: slugify the raw text
                    let raw = resp.text.trim().to_lowercase();
                    let slug: String = raw.chars()
                        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
                        .collect::<String>()
                        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
                    let slug = if slug.len() > 60 { slug[..60].trim_end_matches('-').to_string() } else { slug };
                    Json(json!({"ok": true, "provider": resp.provider, "slug": slug}))
                }
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Alt Text ──────────────────────────────────

#[post("/ai/suggest-alt-text", format = "json", data = "<body>")]
pub fn suggest_alt_text(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestAltTextRequest>,
) -> Json<Value> {
    let context = body.context.as_deref().unwrap_or("Image in a CMS");

    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_alt_text(context, &body.filename),
        max_tokens: Some(128),
        temperature: Some(0.5),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let alt_text = parsed
                        .get("alt_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Json(json!({"ok": true, "provider": resp.provider, "alt_text": alt_text}))
                }
                None => Json(json!({"ok": false, "error": "Failed to parse AI response"})),
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Title ─────────────────────────────────────

#[post("/ai/suggest-title", format = "json", data = "<body>")]
pub fn suggest_title(
    _admin: EditorUser,
    pool: &State<DbPool>,
    body: Json<SuggestTitleRequest>,
) -> Json<Value> {
    let desc = body.description.as_deref().unwrap_or("");

    // If we have an image but no description, first describe the image
    let effective_desc = if desc.is_empty() && body.image_base64.is_some() {
        let img_req = AiRequest {
            system: prompts::seo_system(),
            prompt: prompts::describe_image(),
            max_tokens: Some(512),
            temperature: Some(0.5),
            image_base64: body.image_base64.clone(),
        };
        match ai::complete(pool, &img_req) {
            Ok(resp) => {
                parse_json_from_text(&resp.text)
                    .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(String::from))
                    .unwrap_or(resp.text)
            }
            Err(e) => return Json(json!({"ok": false, "error": e.to_string()})),
        }
    } else {
        desc.to_string()
    };

    if effective_desc.is_empty() {
        return Json(json!({"ok": false, "error": "Provide a description or upload an image"}));
    }

    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::suggest_title(&effective_desc),
        max_tokens: Some(128),
        temperature: Some(0.7),
        image_base64: None,
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let title = parsed
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Json(json!({"ok": true, "provider": resp.provider, "title": title, "description": effective_desc}))
                }
                None => {
                    // Fallback: use first line of raw text as title
                    let raw = resp.text.trim().trim_matches('"').trim_matches('\'');
                    let title = raw.lines().next().unwrap_or(raw).trim();
                    let title = if title.len() > 100 { &title[..100] } else { title };
                    Json(json!({"ok": true, "provider": resp.provider, "title": title, "description": effective_desc}))
                }
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}
