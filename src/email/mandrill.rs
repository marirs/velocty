use std::collections::HashMap;
use serde_json::json;

/// Send email via Mandrill (Mailchimp Transactional) API
/// https://mailchimp.com/developer/transactional/api/messages/send-new-message/
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings.get("email_mandrill_api_key").cloned().unwrap_or_default();
    if api_key.is_empty() {
        return Err("Mandrill API key not configured".into());
    }

    let payload = json!({
        "key": api_key,
        "message": {
            "from_email": from,
            "to": [{"email": to, "type": "to"}],
            "subject": subject,
            "text": body
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://mandrillapp.com/api/1.0/messages/send")
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("Mandrill request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Mandrill returned {}: {}", status, text));
    }

    // Mandrill returns 200 even for rejected emails; check the response body
    let resp_json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Mandrill response parse error: {}", e))?;

    if let Some(arr) = resp_json.as_array() {
        if let Some(first) = arr.first() {
            let status = first.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status == "rejected" || status == "invalid" {
                let reason = first.get("reject_reason").and_then(|v| v.as_str()).unwrap_or("unknown");
                return Err(format!("Mandrill rejected email: {}", reason));
            }
        }
    }

    Ok(())
}
