use std::collections::HashMap;
use serde_json::json;

/// Send email via SendPulse SMTP API
/// https://sendpulse.com/integrations/api/smtp
/// SendPulse requires OAuth2 token exchange before sending.
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let client_id = settings.get("email_sendpulse_client_id").cloned().unwrap_or_default();
    let client_secret = settings.get("email_sendpulse_client_secret").cloned().unwrap_or_default();

    if client_id.is_empty() || client_secret.is_empty() {
        return Err("SendPulse client ID or client secret not configured".into());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Step 1: Get OAuth2 access token
    let token_resp = client
        .post("https://api.sendpulse.com/oauth/access_token")
        .json(&json!({
            "grant_type": "client_credentials",
            "client_id": client_id,
            "client_secret": client_secret
        }))
        .send()
        .map_err(|e| format!("SendPulse token request failed: {}", e))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let text = token_resp.text().unwrap_or_default();
        return Err(format!("SendPulse token returned {}: {}", status, text));
    }

    let token_json: serde_json::Value = token_resp
        .json()
        .map_err(|e| format!("SendPulse token parse error: {}", e))?;

    let access_token = token_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("SendPulse: no access_token in response")?;

    // Step 2: Send email via SMTP endpoint
    let payload = json!({
        "email": {
            "from": {"name": "", "email": from},
            "to": [{"name": "", "email": to}],
            "subject": subject,
            "text": body
        }
    });

    let resp = client
        .post("https://api.sendpulse.com/smtp/emails")
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&payload)
        .send()
        .map_err(|e| format!("SendPulse send request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("SendPulse returned {}: {}", status, text));
    }

    Ok(())
}
