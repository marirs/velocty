use std::collections::HashMap;
use serde_json::{json, Value};

use super::{AiError, AiRequest, AiResponse};

pub fn call(
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
