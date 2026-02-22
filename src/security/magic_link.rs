use crate::store::Store;

/// Create a magic link token for the given email. Stores in DB, returns the token.
pub fn create_token(store: &dyn Store, email: &str) -> Result<String, String> {
    let token = uuid::Uuid::new_v4().to_string();
    store.magic_link_create(&token, email, 15)?;
    Ok(token)
}

/// Verify a magic link token. Returns the associated email if valid.
/// Marks the token as used so it cannot be reused.
pub fn verify_token(store: &dyn Store, token: &str) -> Result<String, String> {
    store.magic_link_verify(token)
}

/// Send a magic link email to the admin.
pub fn send_magic_link_email(store: &dyn Store, email: &str, token: &str) -> Result<(), String> {
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
        "{}/{}/magic-link/verify?token={}",
        site_url.trim_end_matches('/'),
        admin_slug,
        token
    );

    let subject = format!("Sign in to {} — Magic Link", site_name);
    let body = format!(
        "Hello,\n\n\
         Click the link below to sign in to {}:\n\n\
         {}\n\n\
         This link expires in 15 minutes and can only be used once.\n\n\
         If you didn't request this, you can safely ignore this email.\n\n\
         — {}\n",
        site_name, link, site_name
    );

    // Use the email module's configured provider to send
    let from_email = get_from_email(&settings)
        .ok_or("No email provider configured. Magic link requires an email provider.")?;

    crate::email::send_via_provider(&settings, &from_email, email, &subject, &body)
}

/// Determine the "from" email address (mirrors email module logic)
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

/// Clean up expired magic link tokens
pub fn cleanup_expired(store: &dyn Store) -> Result<(), String> {
    // Magic link cleanup is handled by the store's raw_execute or a dedicated method.
    // For now, use raw_execute for SQLite; MongoDB handles TTL via indexes.
    let _ = store.raw_execute("DELETE FROM magic_links WHERE expires_at < datetime('now')");
    Ok(())
}
