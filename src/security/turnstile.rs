use std::collections::HashMap;
use serde_json::Value;

/// Verify a Cloudflare Turnstile token.
/// https://developers.cloudflare.com/turnstile/get-started/server-side-validation/
pub fn verify(
    settings: &HashMap<String, String>,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    let secret_key = settings.get("security_turnstile_secret_key").cloned().unwrap_or_default();
    if secret_key.is_empty() {
        return Err("Turnstile secret key not configured".into());
    }

    let mut params = vec![
        ("secret", secret_key.as_str()),
        ("response", token),
    ];
    if let Some(ip) = remote_ip {
        params.push(("remoteip", ip));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .form(&params)
        .send()
        .map_err(|e| format!("Turnstile request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Turnstile returned {}: {}", status, text));
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("Turnstile JSON parse error: {}", e))?;

    let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    if !success {
        let errors = json.get("error-codes")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        log::warn!("Turnstile verification failed: {}", errors);
    }

    Ok(success)
}
