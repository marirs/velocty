use std::collections::HashMap;

/// Send email via Mailgun API (https://documentation.mailgun.com/docs/mailgun/api-reference/openapi-final/tag/Messages/)
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let api_key = settings.get("email_mailgun_api_key").cloned().unwrap_or_default();
    let domain = settings.get("email_mailgun_domain").cloned().unwrap_or_default();
    let region = settings.get("email_mailgun_region").cloned().unwrap_or_else(|| "us".to_string());

    if api_key.is_empty() || domain.is_empty() {
        return Err("Mailgun API key or domain not configured".into());
    }

    let base_url = if region == "eu" {
        "https://api.eu.mailgun.net"
    } else {
        "https://api.mailgun.net"
    };

    let url = format!("{}/v3/{}/messages", base_url, domain);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(&url)
        .basic_auth("api", Some(&api_key))
        .form(&[
            ("from", from),
            ("to", to),
            ("subject", subject),
            ("text", body),
        ])
        .send()
        .map_err(|e| format!("Mailgun request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Mailgun returned {}: {}", status, text));
    }

    Ok(())
}
