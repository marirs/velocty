use std::collections::HashMap;

/// Send email via Amazon SES v1 Query API with AWS Signature Version 4.
/// https://docs.aws.amazon.com/ses/latest/APIReference/API_SendEmail.html
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let access_key = settings
        .get("email_ses_access_key")
        .cloned()
        .unwrap_or_default();
    let secret_key = settings
        .get("email_ses_secret_key")
        .cloned()
        .unwrap_or_default();
    let region = settings
        .get("email_ses_region")
        .cloned()
        .unwrap_or_else(|| "us-east-1".to_string());

    if access_key.is_empty() || secret_key.is_empty() {
        return Err("SES access key or secret key not configured".into());
    }

    let host = format!("email.{}.amazonaws.com", region);
    let endpoint = format!("https://{}", host);

    // Build form body (sorted by key for canonical request)
    let mut params: Vec<(&str, &str)> = vec![
        ("Action", "SendEmail"),
        ("Destination.ToAddresses.member.1", to),
        ("Message.Body.Text.Data", body),
        ("Message.Subject.Data", subject),
        ("Source", from),
    ];
    params.sort_by_key(|&(k, _)| k);

    let form_body = params
        .iter()
        .map(|(k, v)| format!("{}={}", aws_urlencode(k), aws_urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = &timestamp[..8];

    let payload_hash = hex_sha256(form_body.as_bytes());

    let canonical_headers = format!(
        "content-type:application/x-www-form-urlencoded\nhost:{}\nx-amz-date:{}\n",
        host, timestamp
    );
    let signed_headers = "content-type;host;x-amz-date";

    let canonical_request = format!(
        "POST\n/\n\n{}\n{}\n{}",
        canonical_headers, signed_headers, payload_hash
    );

    let credential_scope = format!("{}/{}/ses/aws4_request", date_stamp, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        timestamp,
        credential_scope,
        hex_sha256(canonical_request.as_bytes())
    );

    // Derive signing key
    let k_date = hmac_sha256(
        format!("AWS4{}", secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"ses");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key, credential_scope, signed_headers, signature
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(&endpoint)
        .header("Authorization", &authorization)
        .header("x-amz-date", &timestamp)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .map_err(|e| format!("SES request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("SES returned {}: {}", status, text));
    }

    Ok(())
}

/// AWS-style percent encoding (RFC 3986, spaces as %20 not +)
fn aws_urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
