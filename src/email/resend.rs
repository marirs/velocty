use serde_json::json;
use std::collections::HashMap;

/// Send email via Resend API (https://resend.com/docs/api-reference/emails/send-email)
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings
        .get("email_resend_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("Resend API key not configured".into());
    }

    let payload = json!({
        "from": from,
        "to": [to],
        "subject": subject,
        "text": body
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("Resend request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Resend returned {}: {}", status, text));
    }

    Ok(())
}
