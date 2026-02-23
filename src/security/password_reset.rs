use crate::store::Store;

/// Create a password reset token for the given email. Stores in magic_links table with purpose.
pub fn create_token(store: &dyn Store, email: &str) -> Result<String, String> {
    let token = uuid::Uuid::new_v4().to_string();
    store.magic_link_create(&token, email, 30)?;
    Ok(token)
}

/// Verify a password reset token. Returns the associated email if valid.
/// Marks the token as used so it cannot be reused.
pub fn verify_token(store: &dyn Store, token: &str) -> Result<String, String> {
    store.magic_link_verify(token)
}

/// Send a password reset email.
pub fn send_reset_email(store: &dyn Store, email: &str, token: &str) -> Result<(), String> {
    let settings = store.setting_all();
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let admin_slug = settings
        .get("admin_slug")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());

    let link = format!(
        "{}/{}/reset-password?token={}",
        site_url.trim_end_matches('/'),
        admin_slug,
        token
    );

    let subject = format!("Reset your password — {}", site_name);
    let body = format!(
        "Hello,\n\n\
         A password reset was requested for your account on {}.\n\n\
         Click the link below to set a new password:\n\n\
         {}\n\n\
         This link expires in 30 minutes and can only be used once.\n\n\
         If you didn't request this, you can safely ignore this email.\n\n\
         — {}\n",
        site_name, link, site_name
    );

    let from_email = get_from_email(&settings)
        .ok_or("No email provider configured. Password reset requires an email provider.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Send an admin-initiated password reset email with a temporary password.
pub fn send_admin_reset_email(
    store: &dyn Store,
    email: &str,
    temp_password: &str,
) -> Result<(), String> {
    let settings = store.setting_all();
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let admin_slug = settings
        .get("admin_slug")
        .cloned()
        .unwrap_or_else(|| "admin".to_string());

    let login_url = format!("{}/{}/login", site_url.trim_end_matches('/'), admin_slug);

    let subject = format!("Your password has been reset — {}", site_name);
    let body = format!(
        "Hello,\n\n\
         An administrator has reset your password on {}.\n\n\
         Your temporary password is:\n\n\
         {}\n\n\
         Please log in and change your password immediately:\n\
         {}\n\n\
         — {}\n",
        site_name, temp_password, login_url, site_name
    );

    let from_email = get_from_email(&settings).ok_or("No email provider configured.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Generate a random temporary password (12 chars, alphanumeric + symbols).
pub fn generate_temp_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789!@#$%";
    let mut password = String::with_capacity(12);
    for _ in 0..12 {
        let idx = rng.gen_range(0..chars.len());
        password.push(chars[idx] as char);
    }
    password
}

fn get_from_email(settings: &std::collections::HashMap<String, String>) -> Option<String> {
    let from_addr = settings
        .get("email_from_address")
        .cloned()
        .unwrap_or_default();
    if !from_addr.is_empty() {
        return Some(from_addr);
    }
    if settings.get("email_gmail_enabled").map(|v| v.as_str()) == Some("true") {
        return settings
            .get("email_gmail_address")
            .cloned()
            .filter(|s| !s.is_empty());
    }
    if settings.get("email_smtp_enabled").map(|v| v.as_str()) == Some("true") {
        return settings
            .get("email_smtp_username")
            .cloned()
            .filter(|s| !s.is_empty());
    }
    let api_providers = [
        "email_resend_enabled",
        "email_ses_enabled",
        "email_postmark_enabled",
        "email_brevo_enabled",
        "email_sendpulse_enabled",
        "email_mailgun_enabled",
        "email_moosend_enabled",
        "email_mandrill_enabled",
        "email_sparkpost_enabled",
    ];
    for provider in &api_providers {
        if settings.get(*provider).map(|v| v.as_str()) == Some("true") {
            return settings
                .get("admin_email")
                .cloned()
                .filter(|s| !s.is_empty());
        }
    }
    None
}
