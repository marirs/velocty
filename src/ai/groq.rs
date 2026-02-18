use serde_json::{json, Value};
use std::collections::HashMap;

use super::{AiError, AiRequest, AiResponse};

pub fn call(settings: &HashMap<String, String>, req: &AiRequest) -> Result<AiResponse, AiError> {
    let api_key = settings.get("ai_groq_api_key").cloned().unwrap_or_default();
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
