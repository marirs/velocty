use serde_json::json;
use std::collections::HashMap;

/// Send email via Postmark API (https://postmarkapp.com/developer/api/email-api)
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let server_token = settings
        .get("email_postmark_server_token")
        .cloned()
        .unwrap_or_default();
    if server_token.is_empty() {
        return Err("Postmark server token not configured".into());
    }

    let payload = json!({
        "From": from,
        "To": to,
        "Subject": subject,
        "TextBody": body
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.postmarkapp.com/email")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("X-Postmark-Server-Token", &server_token)
        .json(&payload)
        .send()
        .map_err(|e| format!("Postmark request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Postmark returned {}: {}", status, text));
    }

    Ok(())
}
