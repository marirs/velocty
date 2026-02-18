use serde_json::{json, Value};
use std::collections::HashMap;

/// Check content for spam using OOPSpam API.
/// https://www.oopspam.com/docs/#check-for-spam
pub fn check_spam(
    settings: &HashMap<String, String>,
    user_ip: &str,
    content: &str,
    author_email: Option<&str>,
) -> Result<bool, String> {
    let api_key = settings
        .get("security_oopspam_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("OOPSpam API key not configured".into());
    }

    let mut payload = json!({
        "content": content,
        "senderIP": user_ip,
        "checkForLength": true,
        "blockTempEmail": true
    });

    if let Some(email) = author_email {
        payload["email"] = json!(email);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.oopspam.com/v1/spamdetection")
        .header("X-Api-Key", &api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("OOPSpam request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("OOPSpam returned {}: {}", status, text));
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("OOPSpam JSON parse error: {}", e))?;

    // OOPSpam returns a Score (0-6). Score >= 3 is considered spam.
    let score = json.get("Score").and_then(|v| v.as_i64()).unwrap_or(0);

    if score >= 3 {
        let details = json
            .get("Details")
            .and_then(|v| v.as_object())
            .map(|obj| format!("{:?}", obj))
            .unwrap_or_default();
        log::warn!("OOPSpam flagged as spam (score: {}): {}", score, details);
        return Ok(true);
    }

    Ok(false)
}
