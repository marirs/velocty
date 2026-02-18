use serde_json::Value;
use std::collections::HashMap;

/// Verify a Google reCAPTCHA v2 or v3 token.
/// https://developers.google.com/recaptcha/docs/verify
pub fn verify(
    settings: &HashMap<String, String>,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, String> {
    let secret_key = settings
        .get("security_recaptcha_secret_key")
        .cloned()
        .unwrap_or_default();
    if secret_key.is_empty() {
        return Err("reCAPTCHA secret key not configured".into());
    }

    let version = settings
        .get("security_recaptcha_version")
        .cloned()
        .unwrap_or_else(|| "v3".to_string());

    let mut params = vec![("secret", secret_key.as_str()), ("response", token)];
    if let Some(ip) = remote_ip {
        params.push(("remoteip", ip));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post("https://www.google.com/recaptcha/api/siteverify")
        .form(&params)
        .send()
        .map_err(|e| format!("reCAPTCHA request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("reCAPTCHA returned {}: {}", status, text));
    }

    let json: Value = resp
        .json()
        .map_err(|e| format!("reCAPTCHA JSON parse error: {}", e))?;

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
        log::warn!("reCAPTCHA verification failed: {}", errors);
        return Ok(false);
    }

    // For v3, check the score (0.0 = bot, 1.0 = human)
    if version == "v3" {
        let score = json.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let threshold = 0.5;
        if score < threshold {
            log::warn!(
                "reCAPTCHA v3 score too low: {} (threshold: {})",
                score,
                threshold
            );
            return Ok(false);
        }
    }

    Ok(true)
}
