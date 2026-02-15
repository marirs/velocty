use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ai::{self, prompts, AiRequest};
use crate::auth::AdminUser;
use crate::db::DbPool;
use crate::models::category::Category;
use crate::models::tag::Tag;

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
pub struct GeneratePostRequest {
    pub description: String,
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InlineAssistRequest {
    pub action: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct DescribeImageRequest {
    pub image_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct SuggestTitleRequest {
    pub description: Option<String>,
    pub image_base64: Option<String>,
}

// ── Suggest Meta Title & Description ──────────────────

#[post("/ai/suggest-meta", format = "json", data = "<body>")]
pub fn suggest_meta(
    _admin: AdminUser,
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
    _admin: AdminUser,
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
    _admin: AdminUser,
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
    _admin: AdminUser,
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
    _admin: AdminUser,
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

// ── Generate Blog Post ────────────────────────────────

#[post("/ai/generate-post", format = "json", data = "<body>")]
pub fn generate_post(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<GeneratePostRequest>,
) -> Json<Value> {
    let req = AiRequest {
        system: "You are a professional blog writer. Write engaging, well-structured content. \
                 Always respond in valid JSON format as specified. Do not include markdown fences or explanations outside the JSON."
            .to_string(),
        prompt: prompts::generate_post(&body.description),
        max_tokens: Some(4096),
        temperature: Some(0.8),
        image_base64: body.image_base64.clone(),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => Json(json!({
                    "ok": true,
                    "provider": resp.provider,
                    "title": parsed.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                    "content_html": parsed.get("content_html").and_then(|v| v.as_str()).unwrap_or(""),
                    "excerpt": parsed.get("excerpt").and_then(|v| v.as_str()).unwrap_or(""),
                    "tags": parsed.get("tags").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
                })),
                None => Json(json!({"ok": false, "error": "Failed to parse AI response"})),
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Inline Assist ─────────────────────────────────────

#[post("/ai/inline-assist", format = "json", data = "<body>")]
pub fn inline_assist(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<InlineAssistRequest>,
) -> Json<Value> {
    let req = AiRequest {
        system: "You are a writing assistant. Transform text as requested. \
                 Always respond in valid JSON format. Do not include markdown fences."
            .to_string(),
        prompt: prompts::inline_assist(&body.action, &body.text),
        max_tokens: Some(2048),
        temperature: Some(0.7),
        image_base64: None,
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let html = parsed
                        .get("html")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Json(json!({"ok": true, "provider": resp.provider, "html": html}))
                }
                None => {
                    // Fallback: treat the whole response as HTML
                    Json(json!({"ok": true, "provider": resp.provider, "html": resp.text}))
                }
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Describe Image ────────────────────────────────────

#[post("/ai/describe-image", format = "json", data = "<body>")]
pub fn describe_image(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<DescribeImageRequest>,
) -> Json<Value> {
    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::describe_image(),
        max_tokens: Some(512),
        temperature: Some(0.5),
        image_base64: Some(body.image_base64.clone()),
    };

    match ai::complete(pool, &req) {
        Ok(resp) => {
            match parse_json_from_text(&resp.text) {
                Some(parsed) => {
                    let description = parsed
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&resp.text)
                        .to_string();
                    Json(json!({"ok": true, "provider": resp.provider, "description": description}))
                }
                None => Json(json!({"ok": true, "provider": resp.provider, "description": resp.text})),
            }
        }
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Suggest Title ─────────────────────────────────────

#[post("/ai/suggest-title", format = "json", data = "<body>")]
pub fn suggest_title(
    _admin: AdminUser,
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

// ── Status Check ──────────────────────────────────────

#[get("/ai/status")]
pub fn ai_status(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let enabled = ai::is_enabled(pool);
    let flags = ai::suggestion_flags(pool);
    Json(json!({
        "enabled": enabled,
        "features": flags,
    }))
}

// ── Helpers ───────────────────────────────────────────

/// Extract JSON from LLM response text (handles markdown fences, leading text, etc.)
fn parse_json_from_text(text: &str) -> Option<Value> {
    log::debug!("AI raw response: {}", &text[..text.len().min(500)]);

    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }

    // Try to find JSON within markdown code fences
    let stripped = text
        .replace("```json", "")
        .replace("```", "");
    if let Ok(v) = serde_json::from_str::<Value>(stripped.trim()) {
        return Some(v);
    }

    // Try to find first { ... } block (handle nested braces)
    if let Some(start) = text.find('{') {
        let mut depth = 0;
        let mut end_pos = None;
        for (i, ch) in text[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end_pos {
            let candidate = &text[start..=end];
            if let Ok(v) = serde_json::from_str::<Value>(candidate) {
                return Some(v);
            }
            // Try fixing common issues: trailing commas, single quotes
            let fixed = candidate
                .replace(",}", "}")
                .replace(",]", "]")
                .replace("'", "\"");
            if let Ok(v) = serde_json::from_str::<Value>(&fixed) {
                return Some(v);
            }
        }
    }

    // Last resort: try to build JSON from the raw text for known fields
    let text_lower = text.to_lowercase();
    // For slug suggestions
    if text_lower.contains("slug") || text.contains('/') || text.contains('-') {
        let clean = text.trim().trim_matches('"').trim_matches('\'');
        // If it looks like a slug (lowercase, hyphens, no spaces)
        let slug_candidate = clean.split_whitespace().last().unwrap_or(clean);
        if slug_candidate.contains('-') && !slug_candidate.contains(' ') && slug_candidate.len() < 100 {
            return Some(json!({"slug": slug_candidate.trim_matches('"').trim_matches('.').to_lowercase()}));
        }
    }
    // For title suggestions
    if !text.contains('{') && text.len() < 200 {
        let clean = text.trim().trim_matches('"').trim_matches('\'').trim();
        if !clean.is_empty() {
            // Check if it looks like a title (not code, not too long)
            let first_line = clean.lines().next().unwrap_or(clean).trim();
            if !first_line.is_empty() && first_line.len() < 150 {
                return Some(json!({"title": first_line}));
            }
        }
    }
    // For meta suggestions — look for lines with "title" and "description"
    if text_lower.contains("title") && text_lower.contains("description") {
        let mut meta_title = String::new();
        let mut meta_desc = String::new();
        for line in text.lines() {
            let l = line.trim().to_lowercase();
            if l.starts_with("title") || l.starts_with("meta title") || l.starts_with("meta_title") {
                meta_title = line.split(':').skip(1).collect::<Vec<_>>().join(":").trim().trim_matches('"').to_string();
            }
            if l.starts_with("description") || l.starts_with("meta description") || l.starts_with("meta_description") {
                meta_desc = line.split(':').skip(1).collect::<Vec<_>>().join(":").trim().trim_matches('"').to_string();
            }
        }
        if !meta_title.is_empty() || !meta_desc.is_empty() {
            return Some(json!({"meta_title": meta_title, "meta_description": meta_desc}));
        }
    }
    // For tag suggestions — look for comma-separated or bulleted lists
    if text_lower.contains("tag") {
        let mut tags: Vec<String> = Vec::new();
        for line in text.lines() {
            let l = line.trim();
            // Skip header lines
            if l.to_lowercase().starts_with("tag") && l.contains(':') {
                let after_colon = l.split(':').skip(1).collect::<Vec<_>>().join(":");
                for t in after_colon.split(',') {
                    let t = t.trim().trim_matches('"').trim_matches('\'').trim_matches('[').trim_matches(']').trim();
                    if !t.is_empty() && t.len() < 50 { tags.push(t.to_string()); }
                }
                continue;
            }
            // Bulleted items
            let stripped = l.trim_start_matches('-').trim_start_matches('*').trim_start_matches("• ").trim();
            if !stripped.is_empty() && stripped.len() < 50 && !stripped.to_lowercase().starts_with("tag") {
                tags.push(stripped.trim_matches('"').to_string());
            }
        }
        if !tags.is_empty() {
            return Some(json!({"tags": tags}));
        }
    }

    log::warn!("Failed to parse AI response as JSON: {}", &text[..text.len().min(300)]);
    None
}


pub fn routes() -> Vec<rocket::Route> {
    routes![
        suggest_meta,
        suggest_tags,
        suggest_categories,
        suggest_slug,
        suggest_alt_text,
        generate_post,
        inline_assist,
        describe_image,
        suggest_title,
        ai_status,
    ]
}
