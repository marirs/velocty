use std::collections::HashMap;

/// Check content for spam using Akismet API.
/// https://akismet.com/developers/comment-check/
pub fn check_spam(
    settings: &HashMap<String, String>,
    site_url: &str,
    user_ip: &str,
    user_agent: &str,
    content: &str,
    author: Option<&str>,
    author_email: Option<&str>,
    comment_type: Option<&str>,
) -> Result<bool, String> {
    let api_key = settings
        .get("security_akismet_api_key")
        .cloned()
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("Akismet API key not configured".into());
    }

    let url = format!("https://{}.rest.akismet.com/1.1/comment-check", api_key);

    let mut params = vec![
        ("blog", site_url.to_string()),
        ("user_ip", user_ip.to_string()),
        ("user_agent", user_agent.to_string()),
        ("comment_content", content.to_string()),
        (
            "comment_type",
            comment_type.unwrap_or("comment").to_string(),
        ),
    ];
    if let Some(name) = author {
        params.push(("comment_author", name.to_string()));
    }
    if let Some(email) = author_email {
        params.push(("comment_author_email", email.to_string()));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .map_err(|e| format!("Akismet request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("Akismet returned {}: {}", status, text));
    }

    let body = resp.text().unwrap_or_default();

    // Akismet returns "true" if spam, "false" if ham
    match body.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("Akismet unexpected response: {}", other)),
    }
}
