use serde_json::Value;
use std::collections::HashMap;

/// Verify an hCaptcha token.
/// https://docs.hcaptcha.com/#verify-the-user-response-server-side
pub fn verify(
    settings: &HashMap<String, String>,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    let secret_key = settings
        .get("security_hcaptcha_secret_key")
        .cloned()
        .unwrap_or_default();
    if secret_key.is_empty() {
        return Err("hCaptcha secret key not configured".into());
    }

    let site_key = settings
        .get("security_hcaptcha_site_key")
        .cloned()
        .unwrap_or_default();

    let mut params = vec![("secret", secret_key.as_str()), ("response", token)];
    if !site_key.is_empty() {
        params.push(("sitekey", site_key.as_str()));
    }
    if let Some(ip) = remote_ip {
        params.push(("remoteip", ip));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://api.hcaptcha.com/siteverify")
        .form(&params)
        .send()
        .map_err(|e| format!("hCaptcha request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("hCaptcha returned {}: {}", status, text));
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("hCaptcha JSON parse error: {}", e))?;

    let success = json
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !success {
        let errors = json
            .get("error-codes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        log::warn!("hCaptcha verification failed: {}", errors);
    }

    Ok(success)
}
