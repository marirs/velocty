use std::collections::HashMap;

use super::send_smtp;

pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let host = settings.get("email_smtp_host").cloned().unwrap_or_default();
    let port: u16 = settings.get("email_smtp_port").and_then(|v| v.parse().ok()).unwrap_or(587);
    let username = settings.get("email_smtp_username").cloned().unwrap_or_default();
    let password = settings.get("email_smtp_password").cloned().unwrap_or_default();

    if host.is_empty() || username.is_empty() {
        return Err("SMTP host or username not configured".into());
    }

    send_smtp(&host, port, &username, &password, from, to, subject, body)
}
