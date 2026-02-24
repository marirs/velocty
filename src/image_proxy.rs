use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Encode an upload path into an opaque proxy token.
/// Format: `base64url(path)-hmac_hex[0..16]`
pub fn encode_token(secret: &str, path: &str) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(path.as_bytes());
    let sig = hmac_signature(secret, path);
    format!("{}-{}", encoded, sig)
}

/// Decode a proxy token back into the original upload path.
/// Returns `None` if the token is malformed or the HMAC doesn't match.
pub fn decode_token(secret: &str, token: &str) -> Option<String> {
    let dash = token.rfind('-')?;
    let encoded = &token[..dash];
    let sig = &token[dash + 1..];
    let path_bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let path = String::from_utf8(path_bytes).ok()?;
    let expected = hmac_signature(secret, &path);
    if sig.len() == expected.len() && constant_time_eq(sig.as_bytes(), expected.as_bytes()) {
        Some(path)
    } else {
        None
    }
}

/// Decode with dual-key support for zero-downtime key rotation.
/// Tries the current secret first; if that fails and an old secret is provided
/// (and hasn't expired), tries the old secret.
pub fn decode_token_with_fallback(
    secret: &str,
    old_secret: &str,
    old_expires: &str,
    token: &str,
) -> Option<String> {
    // Try current key first
    if let Some(path) = decode_token(secret, token) {
        return Some(path);
    }
    // Fall back to old key if it exists and hasn't expired
    if !old_secret.is_empty() && !old_expires.is_empty() {
        if let Ok(expires) = chrono::NaiveDateTime::parse_from_str(old_expires, "%Y-%m-%d %H:%M:%S")
        {
            let now = chrono::Utc::now().naive_utc();
            if now < expires {
                return decode_token(old_secret, token);
            }
        }
    }
    None
}

/// Rewrite all `/uploads/...` URLs in rendered HTML to `/img/<token>` proxy URLs.
/// Matches paths in src="...", href="...", url(...), and srcset="..." attributes.
pub fn rewrite_upload_urls(html: &str, secret: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut remaining = html;

    while let Some(pos) = remaining.find("/uploads/") {
        // Push everything before this match
        result.push_str(&remaining[..pos]);

        // Extract the full path: /uploads/... up to a delimiter
        let path_start = pos;
        let after = &remaining[pos..];
        let path_end = after
            .find(['"', '\'', ')', ' ', '>', '?'])
            .unwrap_or(after.len());
        let path = &after[..path_end];

        // Only rewrite if it looks like a real file path (has an extension)
        if path.contains('.') && !path.contains("..") {
            let token = encode_token(secret, path);
            result.push_str("/img/");
            result.push_str(&token);
        } else {
            result.push_str(path);
        }

        remaining = &remaining[path_start + path_end..];
    }

    result.push_str(remaining);
    result
}

fn hmac_signature(secret: &str, path: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(path.as_bytes());
    let result = mac.finalize().into_bytes();
    // Use first 8 bytes (16 hex chars) for a compact but secure signature
    hex::encode(&result[..8])
}

/// Constant-time comparison — delegates to the consolidated implementation.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    crate::security::constant_time_eq(a, b)
}

/// Detect MIME type from file extension
pub fn mime_from_extension(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tiff" | "tif" => "image/tiff",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heic",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "pdf" => "application/pdf",
        "css" => "text/css",
        "js" => "application/javascript",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}
