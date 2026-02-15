use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::settings::Setting;

// ── Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub system: String,
    pub prompt: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub image_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug)]
pub struct AiError(pub String);

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Provider Enum ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Provider {
    Ollama,
    OpenAi,
    Gemini,
    Cloudflare,
    Groq,
}

impl Provider {
    fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "ollama" => Some(Self::Ollama),
            "openai" => Some(Self::OpenAi),
            "gemini" => Some(Self::Gemini),
            "cloudflare" => Some(Self::Cloudflare),
            "groq" => Some(Self::Groq),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::Cloudflare => "cloudflare",
            Self::Groq => "groq",
        }
    }

    fn supports_vision(&self) -> bool {
        matches!(self, Self::OpenAi | Self::Gemini | Self::Ollama | Self::Groq)
    }
}

// ── Public API ────────────────────────────────────────

/// Send a request through the failover chain. Returns the first successful response.
pub fn complete(pool: &DbPool, req: &AiRequest) -> Result<AiResponse, AiError> {
    let settings: HashMap<String, String> = Setting::all(pool);
    let chain_str = settings
        .get("ai_failover_chain")
        .cloned()
        .unwrap_or_else(|| "ollama,openai,gemini,groq,cloudflare".to_string());

    let chain: Vec<Provider> = chain_str
        .split(',')
        .filter_map(|s| Provider::from_str(s))
        .collect();

    if chain.is_empty() {
        return Err(AiError("No AI providers configured in failover chain".into()));
    }

    let mut last_error = String::new();

    for provider in &chain {
        // Skip disabled providers
        let enabled_key = format!("ai_{}_enabled", provider.name());
        if settings.get(&enabled_key).map(|v| v.as_str()) != Some("true") {
            continue;
        }

        // If request has an image, skip providers that don't support vision
        if req.image_base64.is_some() && !provider.supports_vision() {
            continue;
        }

        match call_provider(provider, &settings, req) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                log::warn!("AI provider {} failed: {}", provider.name(), e.0);
                last_error = e.0;
            }
        }
    }

    Err(AiError(format!(
        "All AI providers failed. Last error: {}",
        last_error
    )))
}

/// Check if any AI provider is enabled
pub fn is_enabled(pool: &DbPool) -> bool {
    let settings: HashMap<String, String> = Setting::all(pool);
    ["ollama", "openai", "gemini", "cloudflare", "groq"]
        .iter()
        .any(|p| settings.get(&format!("ai_{}_enabled", p)).map(|v| v.as_str()) == Some("true"))
}

/// Check if any vision-capable provider is enabled (Ollama, OpenAI, Gemini)
pub fn has_vision_provider(pool: &DbPool) -> bool {
    let settings: HashMap<String, String> = Setting::all(pool);
    ["ollama", "openai", "gemini", "groq"]
        .iter()
        .any(|p| settings.get(&format!("ai_{}_enabled", p)).map(|v| v.as_str()) == Some("true"))
}

/// Check which suggestion features are enabled
pub fn suggestion_flags(pool: &DbPool) -> HashMap<String, bool> {
    let settings: HashMap<String, String> = Setting::all(pool);
    let mut flags = HashMap::new();
    for key in &[
        "ai_suggest_meta",
        "ai_suggest_tags",
        "ai_suggest_categories",
        "ai_suggest_alt_text",
        "ai_suggest_slug",
        "ai_theme_generation",
        "ai_post_generation",
    ] {
        flags.insert(
            key.to_string(),
            settings.get(*key).map(|v| v.as_str()) == Some("true"),
        );
    }
    flags
}

// ── Provider Implementations ──────────────────────────

fn call_provider(
    provider: &Provider,
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    match provider {
        Provider::Ollama => call_ollama(settings, req),
        Provider::OpenAi => call_openai(settings, req),
        Provider::Gemini => call_gemini(settings, req),
        Provider::Cloudflare => call_cloudflare(settings, req),
        Provider::Groq => call_groq(settings, req),
    }
}

// ── Ollama ────────────────────────────────────────────

fn call_ollama(
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    let base_url = settings
        .get("ai_ollama_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = settings
        .get("ai_ollama_model")
        .cloned()
        .unwrap_or_default();

    if model.is_empty() {
        return Err(AiError("Ollama model not configured".into()));
    }

    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));

    let user_content = if let Some(ref img) = req.image_base64 {
        json!({
            "role": "user",
            "content": req.prompt,
            "images": [img]
        })
    } else {
        json!({"role": "user", "content": req.prompt})
    };

    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": req.system},
            user_content
        ],
        "stream": false,
        "options": {
            "temperature": req.temperature.unwrap_or(0.7),
            "num_predict": req.max_tokens.unwrap_or(1024)
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AiError(format!("HTTP client error: {}", e)))?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| AiError(format!("Ollama request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(AiError(format!("Ollama returned {}: {}", status, text)));
    }

    let json: Value = resp
        .json()
        .map_err(|e| AiError(format!("Ollama JSON parse error: {}", e)))?;

    let text = json
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AiResponse {
        text,
        provider: "ollama".into(),
        model,
    })
}

// ── OpenAI ────────────────────────────────────────────

fn call_openai(
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    let api_key = settings
        .get("ai_openai_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err(AiError("OpenAI API key not configured".into()));
    }

    let model = settings
        .get("ai_openai_model")
        .cloned()
        .unwrap_or_else(|| "gpt-4".to_string());

    let base_url = settings
        .get("ai_openai_base_url")
        .cloned()
        .unwrap_or_default();
    let base_url = if base_url.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        base_url.trim_end_matches('/').to_string()
    };

    let url = format!("{}/chat/completions", base_url);

    let user_message = if let Some(ref img) = req.image_base64 {
        json!({
            "role": "user",
            "content": [
                {"type": "text", "text": req.prompt},
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", img)}}
            ]
        })
    } else {
        json!({"role": "user", "content": req.prompt})
    };

    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": req.system},
            user_message
        ],
        "max_tokens": req.max_tokens.unwrap_or(1024),
        "temperature": req.temperature.unwrap_or(0.7)
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AiError(format!("HTTP client error: {}", e)))?;

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| AiError(format!("OpenAI request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(AiError(format!("OpenAI returned {}: {}", status, text)));
    }

    let json: Value = resp
        .json()
        .map_err(|e| AiError(format!("OpenAI JSON parse error: {}", e)))?;

    let text = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AiResponse {
        text,
        provider: "openai".into(),
        model,
    })
}

// ── Gemini ────────────────────────────────────────────

fn call_gemini(
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    let api_key = settings
        .get("ai_gemini_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err(AiError("Gemini API key not configured".into()));
    }

    let model = settings
        .get("ai_gemini_model")
        .cloned()
        .unwrap_or_else(|| "gemini-pro".to_string());

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let mut parts = vec![json!({"text": format!("{}\n\n{}", req.system, req.prompt)})];
    if let Some(ref img) = req.image_base64 {
        parts.push(json!({
            "inline_data": {
                "mime_type": "image/jpeg",
                "data": img
            }
        }));
    }

    let body = json!({
        "contents": [{"parts": parts}],
        "generationConfig": {
            "maxOutputTokens": req.max_tokens.unwrap_or(1024),
            "temperature": req.temperature.unwrap_or(0.7)
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AiError(format!("HTTP client error: {}", e)))?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| AiError(format!("Gemini request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(AiError(format!("Gemini returned {}: {}", status, text)));
    }

    let json: Value = resp
        .json()
        .map_err(|e| AiError(format!("Gemini JSON parse error: {}", e)))?;

    let text = json
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AiResponse {
        text,
        provider: "gemini".into(),
        model,
    })
}

// ── Cloudflare Workers AI ─────────────────────────────

fn call_cloudflare(
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    let account_id = settings
        .get("ai_cloudflare_account_id")
        .cloned()
        .unwrap_or_default();
    let api_token = settings
        .get("ai_cloudflare_api_token")
        .cloned()
        .unwrap_or_default();

    if account_id.is_empty() || api_token.is_empty() {
        return Err(AiError("Cloudflare account ID or API token not configured".into()));
    }

    let model = settings
        .get("ai_cloudflare_model")
        .cloned()
        .unwrap_or_else(|| "@cf/meta/llama-3-8b-instruct".to_string());

    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
        account_id, model
    );

    let body = json!({
        "messages": [
            {"role": "system", "content": req.system},
            {"role": "user", "content": req.prompt}
        ],
        "max_tokens": req.max_tokens.unwrap_or(1024),
        "temperature": req.temperature.unwrap_or(0.7)
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AiError(format!("HTTP client error: {}", e)))?;

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| AiError(format!("Cloudflare AI request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(AiError(format!("Cloudflare AI returned {}: {}", status, text)));
    }

    let json: Value = resp
        .json()
        .map_err(|e| AiError(format!("Cloudflare AI JSON parse error: {}", e)))?;

    let text = json
        .get("result")
        .and_then(|r| r.get("response"))
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AiResponse {
        text,
        provider: "cloudflare".into(),
        model,
    })
}

// ── Groq ──────────────────────────────────────────────

fn call_groq(
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    let api_key = settings
        .get("ai_groq_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err(AiError("Groq API key not configured".into()));
    }

    let model = settings
        .get("ai_groq_model")
        .cloned()
        .unwrap_or_else(|| "llama-3.3-70b-versatile".to_string());

    let url = "https://api.groq.com/openai/v1/chat/completions";

    let user_message = if let Some(ref img) = req.image_base64 {
        json!({
            "role": "user",
            "content": [
                {"type": "text", "text": req.prompt},
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", img)}}
            ]
        })
    } else {
        json!({"role": "user", "content": req.prompt})
    };

    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": req.system},
            user_message
        ],
        "max_tokens": req.max_tokens.unwrap_or(1024),
        "temperature": req.temperature.unwrap_or(0.7)
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AiError(format!("HTTP client error: {}", e)))?;

    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| AiError(format!("Groq request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(AiError(format!("Groq returned {}: {}", status, text)));
    }

    let json: Value = resp
        .json()
        .map_err(|e| AiError(format!("Groq JSON parse error: {}", e)))?;

    let text = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AiResponse {
        text,
        provider: "groq".into(),
        model,
    })
}

// ── Prompt Builders ───────────────────────────────────

pub mod prompts {
    /// System prompt for SEO suggestions
    pub fn seo_system() -> String {
        "You are an SEO expert assistant for a CMS. You provide concise, actionable suggestions. \
         Always respond in valid JSON format as specified. Do not include markdown fences or explanations outside the JSON."
            .to_string()
    }

    /// Suggest meta title and description for content
    pub fn suggest_meta(title: &str, content_excerpt: &str, content_type: &str) -> String {
        format!(
            "Given this {} titled \"{}\" with content excerpt:\n\n{}\n\n\
             Generate an SEO-optimized meta title (≤60 chars) and meta description (120-155 chars).\n\
             Respond as JSON: {{\"meta_title\": \"...\", \"meta_description\": \"...\"}}",
            content_type, title, content_excerpt
        )
    }

    /// Suggest tags for content
    pub fn suggest_tags(title: &str, content_excerpt: &str, existing_tags: &[String]) -> String {
        let existing = if existing_tags.is_empty() {
            "none".to_string()
        } else {
            existing_tags.join(", ")
        };
        format!(
            "Given content titled \"{}\" with excerpt:\n\n{}\n\n\
             Existing tags in the system: {}\n\n\
             Suggest 3-6 relevant tags. Prefer reusing existing tags when appropriate. \
             Respond as JSON: {{\"tags\": [\"tag1\", \"tag2\", ...]}}",
            title, content_excerpt, existing
        )
    }

    /// Suggest categories for content
    pub fn suggest_categories(
        title: &str,
        content_excerpt: &str,
        existing_categories: &[String],
    ) -> String {
        let existing = if existing_categories.is_empty() {
            "none".to_string()
        } else {
            existing_categories.join(", ")
        };
        format!(
            "Given content titled \"{}\" with excerpt:\n\n{}\n\n\
             Existing categories: {}\n\n\
             Suggest 1-3 relevant categories. Prefer reusing existing categories when appropriate. \
             Respond as JSON: {{\"categories\": [\"cat1\", \"cat2\"]}}",
            title, content_excerpt, existing
        )
    }

    /// Suggest a URL slug
    pub fn suggest_slug(title: &str) -> String {
        format!(
            "Generate an SEO-friendly URL slug for the title: \"{}\"\n\
             Rules: lowercase, hyphens only, no stop words, 3-6 words max.\n\
             Respond as JSON: {{\"slug\": \"...\"}}",
            title
        )
    }

    /// Suggest alt text for an image
    pub fn suggest_alt_text(context: &str, image_filename: &str) -> String {
        format!(
            "Generate descriptive alt text for an image.\n\
             Context: {}\nFilename: {}\n\n\
             The alt text should be concise (≤125 chars), descriptive, and accessible.\n\
             Respond as JSON: {{\"alt_text\": \"...\"}}",
            context, image_filename
        )
    }

    /// Describe an image for content generation
    pub fn describe_image() -> String {
        "Describe this image in detail. What is shown? What is the subject, setting, mood, colors, and style? \
         Be specific and descriptive in 2-3 sentences.\n\
         Respond as JSON: {\"description\": \"...\"}"
            .to_string()
    }

    /// Suggest a title from a description or image description
    pub fn suggest_title(description: &str) -> String {
        format!(
            "Given this description:\n\n{}\n\n\
             Generate a compelling, SEO-friendly title (≤70 chars).\n\
             Respond as JSON: {{\"title\": \"...\"}}",
            description
        )
    }

    /// Generate a blog post from a description
    pub fn generate_post(description: &str) -> String {
        format!(
            "Write a blog post based on this description:\n\n{}\n\n\
             Requirements:\n\
             - Use HTML formatting (h2, h3, p, ul/li, strong, em)\n\
             - Include an engaging introduction\n\
             - 3-5 sections with subheadings (h2)\n\
             - 600-1200 words\n\
             - Professional but approachable tone\n\
             - Do NOT include an h1 tag (the title is separate)\n\n\
             Also suggest a title, excerpt (1-2 sentences), and 3-5 tags.\n\n\
             Respond as JSON:\n\
             {{\"title\": \"...\", \"content_html\": \"...\", \"excerpt\": \"...\", \"tags\": [\"...\"]}}",
            description
        )
    }

    /// Inline assist: transform selected text
    pub fn inline_assist(action: &str, selected_text: &str) -> String {
        let instruction = match action {
            "expand" => "Expand this text with more detail, examples, and depth. Keep the same tone and style. Return HTML.",
            "rewrite" => "Rewrite this text to improve clarity, flow, and readability. Keep the same meaning and length. Return HTML.",
            "summarise" => "Summarise this text into a concise version, keeping only the key points. Return HTML.",
            "continue" => "Continue writing from where this text ends, maintaining the same tone, style, and topic. Write 2-3 additional paragraphs. Return HTML.",
            "formal" => "Rewrite this text in a more formal, professional tone. Return HTML.",
            "casual" => "Rewrite this text in a more casual, conversational tone. Return HTML.",
            _ => "Improve this text. Return HTML.",
        };
        format!(
            "{}\n\nText:\n{}\n\nRespond as JSON: {{\"html\": \"...\"}}",
            instruction, selected_text
        )
    }
}
