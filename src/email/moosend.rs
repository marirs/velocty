use std::collections::HashMap;
use serde_json::json;

/// Send email via Moosend API (https://moosendapp.docs.apiary.io/)
/// Moosend uses a transactional email endpoint with API key auth.
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings.get("email_moosend_api_key").cloned().unwrap_or_default();
    if api_key.is_empty() {
        return Err("Moosend API key not configured".into());
    }

    let payload = json!({
        "From": from,
        "To": to,
        "Subject": subject,
        "Body": body
    });

    let url = format!(
        "https://api.moosend.com/v3/emails/send.json?apikey={}",
        api_key
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| format!("Moosend request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Moosend returned {}: {}", status, text));
    }

    Ok(())
}
