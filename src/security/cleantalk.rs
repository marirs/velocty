use std::collections::HashMap;
use serde_json::{json, Value};

/// Check content for spam using CleanTalk API.
/// https://cleantalk.org/help/api-check-message
pub fn check_spam(
    settings: &HashMap<String, String>,
    user_ip: &str,
    user_agent: &str,
    content: &str,
    author: Option<&str>,
    author_email: Option<&str>,
) -> Result<bool, String> {
    let api_key = settings.get("security_cleantalk_api_key").cloned().unwrap_or_default();
    if api_key.is_empty() {
        return Err("CleanTalk API key not configured".into());
    }

    let payload = json!({
        "method_name": "check_message",
        "auth_key": api_key,
        "sender_ip": user_ip,
        "agent": user_agent,
        "message": content,
        "sender_nickname": author.unwrap_or(""),
        "sender_email": author_email.unwrap_or(""),
        "js_on": 1,
        "submit_time": 5
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://moderate.cleantalk.org/api2.0")
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("CleanTalk request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("CleanTalk returned {}: {}", status, text));
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("CleanTalk JSON parse error: {}", e))?;

    // CleanTalk returns allow: 0 for spam, allow: 1 for clean
    let allow = json.get("allow").and_then(|v| v.as_i64()).unwrap_or(1);

    if allow == 0 {
        let comment = json.get("comment")
            .and_then(|v| v.as_str())
            .unwrap_or("spam detected");
        log::warn!("CleanTalk flagged as spam: {}", comment);
        return Ok(true);
    }

    Ok(false)
}
