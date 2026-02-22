use serde_json::{json, Value};
use std::collections::HashMap;

use super::{AiError, AiRequest, AiResponse};

pub fn call(settings: &HashMap<String, String>, req: &AiRequest) -> Result<AiResponse, AiError> {
    let api_key = settings
        .get("ai_gemini_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err(AiError("Gemini API key not configured".into()));
    }

    let mut model = settings
        .get("ai_gemini_model")
        .cloned()
        .unwrap_or_else(|| "gemini-pro".to_string());

    // Auto-upgrade to a vision-capable model when an image is present
    if req.image_base64.is_some() {
        let m = model.to_lowercase();
        let is_vision = m.contains("flash")
            || m.contains("vision")
            || m.contains("pro-vision")
            || m.contains("1.5")
            || m.contains("2.0")
            || m.contains("2.5");
        if !is_vision {
            log::info!(
                "[ai] Gemini model '{}' does not support vision; upgrading to gemini-2.0-flash for this request",
                model
            );
            model = "gemini-2.0-flash".to_string();
        }
    }

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
