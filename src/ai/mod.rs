pub mod cloudflare;
pub mod gemini;
pub mod groq;
pub mod ollama;
pub mod openai;
pub mod prompts;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        matches!(
            self,
            Self::OpenAi | Self::Gemini | Self::Ollama | Self::Groq
        )
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
        .filter_map(Provider::from_str)
        .collect();

    if chain.is_empty() {
        return Err(AiError(
            "No AI providers configured in failover chain".into(),
        ));
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
        .any(|p| {
            settings
                .get(&format!("ai_{}_enabled", p))
                .map(|v| v.as_str())
                == Some("true")
        })
}

/// Check if any vision-capable provider is enabled (Ollama, OpenAI, Gemini, Groq)
pub fn has_vision_provider(pool: &DbPool) -> bool {
    let settings: HashMap<String, String> = Setting::all(pool);
    ["ollama", "openai", "gemini", "groq"].iter().any(|p| {
        settings
            .get(&format!("ai_{}_enabled", p))
            .map(|v| v.as_str())
            == Some("true")
    })
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

// ── Provider Dispatch ─────────────────────────────────

fn call_provider(
    provider: &Provider,
    settings: &HashMap<String, String>,
    req: &AiRequest,
) -> Result<AiResponse, AiError> {
    match provider {
        Provider::Ollama => ollama::call(settings, req),
        Provider::OpenAi => openai::call(settings, req),
        Provider::Gemini => gemini::call(settings, req),
        Provider::Cloudflare => cloudflare::call(settings, req),
        Provider::Groq => groq::call(settings, req),
    }
}
