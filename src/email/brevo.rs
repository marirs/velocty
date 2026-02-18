use serde_json::json;
use std::collections::HashMap;

/// Send email via Brevo (formerly Sendinblue) API
/// https://developers.brevo.com/reference/sendtransacemail
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings
        .get("email_brevo_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("Brevo API key not configured".into());
    }

    let payload = json!({
        "sender": {"email": from},
        "to": [{"email": to}],
        "subject": subject,
        "textContent": body
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.brevo.com/v3/smtp/email")
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .header("api-key", &api_key)
        .json(&payload)
        .send()
        .map_err(|e| format!("Brevo request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Brevo returned {}: {}", status, text));
    }

    Ok(())
}
