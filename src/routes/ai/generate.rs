use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use std::sync::Arc;

use crate::ai::{self, prompts, AiRequest};
use crate::security::auth::EditorUser;
use crate::store::Store;

use super::parse_json_from_text;

// ── Request Types ─────────────────────────────────────

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

// ── Generate Blog Post ────────────────────────────────

#[post("/ai/generate-post", format = "json", data = "<body>")]
pub fn generate_post(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
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

    match ai::complete(&**store.inner(), &req) {
        Ok(resp) => match parse_json_from_text(&resp.text) {
            Some(parsed) => Json(json!({
                "ok": true,
                "provider": resp.provider,
                "title": parsed.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "content_html": parsed.get("content_html").and_then(|v| v.as_str()).unwrap_or(""),
                "excerpt": parsed.get("excerpt").and_then(|v| v.as_str()).unwrap_or(""),
                "tags": parsed.get("tags").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
            })),
            None => Json(json!({"ok": false, "error": "Failed to parse AI response"})),
        },
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

// ── Inline Assist ─────────────────────────────────────

#[post("/ai/inline-assist", format = "json", data = "<body>")]
pub fn inline_assist(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
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

    match ai::complete(&**store.inner(), &req) {
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
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    body: Json<DescribeImageRequest>,
) -> Json<Value> {
    let req = AiRequest {
        system: prompts::seo_system(),
        prompt: prompts::describe_image(),
        max_tokens: Some(512),
        temperature: Some(0.5),
        image_base64: Some(body.image_base64.clone()),
    };

    match ai::complete(&**store.inner(), &req) {
        Ok(resp) => match parse_json_from_text(&resp.text) {
            Some(parsed) => {
                let description = parsed
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&resp.text)
                    .to_string();
                Json(json!({"ok": true, "provider": resp.provider, "description": description}))
            }
            None => Json(json!({"ok": true, "provider": resp.provider, "description": resp.text})),
        },
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}
