use rocket::http::{Cookie, CookieJar};

const MFA_PENDING_COOKIE: &str = "velocty_mfa_pending";

/// Generate a new TOTP secret (base32-encoded)
pub fn generate_secret() -> String {
    use totp_rs::Secret;
    let secret = Secret::generate_secret();
    secret.to_encoded().to_string()
}

/// Build a TOTP instance from a base32 secret
fn build_totp(secret_b32: &str, issuer: &str, account: &str) -> Result<totp_rs::TOTP, String> {
    use totp_rs::{Algorithm, Secret, TOTP};
    let secret_bytes = Secret::Encoded(secret_b32.to_string())
        .to_bytes()
        .map_err(|e| format!("Invalid secret: {}", e))?;
    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes, Some(issuer.to_string()), account.to_string())
        .map_err(|e| format!("TOTP error: {}", e))
}

/// Generate a QR code as a data URI (PNG base64) for the TOTP secret
pub fn qr_data_uri(secret_b32: &str, issuer: &str, account: &str) -> Result<String, String> {
    let totp = build_totp(secret_b32, issuer, account)?;
    totp.get_qr_base64().map_err(|e| format!("QR error: {}", e))
        .map(|b64| format!("data:image/png;base64,{}", b64))
}

/// Verify a 6-digit TOTP code against the secret
pub fn verify_code(secret_b32: &str, code: &str) -> bool {
    let totp = match build_totp(secret_b32, "Velocty", "admin") {
        Ok(t) => t,
        Err(_) => return false,
    };
    totp.check_current(code).unwrap_or(false)
}

/// Generate 10 recovery codes (8 chars each, alphanumeric)
pub fn generate_recovery_codes() -> Vec<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    (0..10)
        .map(|_| {
            let code: String = (0..8)
                .map(|_| chars[rng.gen_range(0..chars.len())] as char)
                .collect();
            format!("{}-{}", &code[..4], &code[4..])
        })
        .collect()
}

/// Set the MFA pending cookie (stores session_id of the pending auth)
pub fn set_pending_cookie(cookies: &CookieJar<'_>, pending_token: &str) {
    let mut cookie = Cookie::new(MFA_PENDING_COOKIE, pending_token.to_string());
    cookie.set_http_only(true);
    cookie.set_same_site(rocket::http::SameSite::Strict);
    cookie.set_path("/");
    cookie.set_max_age(rocket::time::Duration::minutes(5));
    cookies.add_private(cookie);
}

/// Get and clear the MFA pending cookie
pub fn take_pending_cookie(cookies: &CookieJar<'_>) -> Option<String> {
    let val = cookies.get_private(MFA_PENDING_COOKIE).map(|c| c.value().to_string());
    cookies.remove_private(Cookie::from(MFA_PENDING_COOKIE));
    val
}

/// Get the MFA pending cookie without clearing it
pub fn get_pending_cookie(cookies: &CookieJar<'_>) -> Option<String> {
    cookies.get_private(MFA_PENDING_COOKIE).map(|c| c.value().to_string())
}
