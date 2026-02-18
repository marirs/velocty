use serde_json::json;
use std::collections::HashMap;

/// Send email via SparkPost API (https://developers.sparkpost.com/api/transmissions/)
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings
        .get("email_sparkpost_api_key")
        .cloned()
        .unwrap_or_default();
    let region = settings
        .get("email_sparkpost_region")
        .cloned()
        .unwrap_or_else(|| "us".to_string());

    if api_key.is_empty() {
        return Err("SparkPost API key not configured".into());
    }

    let base_url = if region == "eu" {
        "https://api.eu.sparkpost.com"
    } else {
        "https://api.sparkpost.com"
    };

    let url = format!("{}/api/v1/transmissions", base_url);

    let payload = json!({
        "recipients": [{"address": {"email": to}}],
        "content": {
            "from": {"email": from},
            "subject": subject,
            "text": body
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(&url)
        .header("Authorization", &api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("SparkPost request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("SparkPost returned {}: {}", status, text));
    }

    Ok(())
}
