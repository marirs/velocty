use serde_json::{json, Value};
use std::collections::HashMap;

use super::{AiError, AiRequest, AiResponse};

pub fn call(settings: &HashMap<String, String>, req: &AiRequest) -> Result<AiResponse, AiError> {
    let account_id = settings
        .get("ai_cloudflare_account_id")
        .cloned()
        .unwrap_or_default();
    let api_token = settings
        .get("ai_cloudflare_api_token")
        .cloned()
        .unwrap_or_default();

    if account_id.is_empty() || api_token.is_empty() {
        return Err(AiError(
            "Cloudflare account ID or API token not configured".into(),
        ));
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
        return Err(AiError(format!(
            "Cloudflare AI returned {}: {}",
            status, text
        )));
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
