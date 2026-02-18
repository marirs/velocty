use std::collections::HashMap;

use super::send_smtp;

pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let address = settings
        .get("email_gmail_address")
        .cloned()
        .unwrap_or_default();
    let app_password = settings
        .get("email_gmail_app_password")
        .cloned()
        .unwrap_or_default();

    if address.is_empty() || app_password.is_empty() {
        return Err("Gmail address or app password not configured".into());
    }

    send_smtp(
        "smtp.gmail.com",
        587,
        &address,
        &app_password,
        from,
        to,
        subject,
        body,
    )
}
