use serde_json::{json, Value};
use std::collections::HashMap;

use super::{AiError, AiRequest, AiResponse};

pub fn call(settings: &HashMap<String, String>, req: &AiRequest) -> Result<AiResponse, AiError> {
    let api_key = settings
        .get("ai_openai_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err(AiError("OpenAI API key not configured".into()));
    }

    let mut model = settings
        .get("ai_openai_model")
        .cloned()
        .unwrap_or_else(|| "gpt-4".to_string());

    // Auto-upgrade to a vision-capable model when an image is present
    if req.image_base64.is_some() {
        let m = model.to_lowercase();
        let is_vision = m.contains("4o")
            || m.contains("vision")
            || m.contains("gpt-4-turbo")
            || m.starts_with("o1")
            || m.starts_with("o3")
            || m.starts_with("o4");
        if !is_vision {
            log::info!(
                "[ai] Model '{}' does not support vision; upgrading to gpt-4o for this request",
                model
            );
            model = "gpt-4o".to_string();
        }
    }

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
