use std::collections::HashMap;

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

use crate::db::DbPool;
use crate::models::settings::Setting;

/// Send a purchase confirmation email to the buyer with download link and license key.
pub fn send_purchase_email(
    pool: &DbPool,
    buyer_email: &str,
    item_title: &str,
    purchase_note: &str,
    download_url: &str,
    license_key: Option<&str>,
    amount: f64,
    currency: &str,
) {
    let settings = Setting::all(pool);
    let site_name = settings.get("site_name").cloned().unwrap_or_else(|| "Velocty".to_string());
    let from_email = get_from_email(&settings);

    if from_email.is_none() {
        eprintln!("[email] No email provider configured, skipping purchase email to {}", buyer_email);
        return;
    }
    let from = from_email.unwrap();

    let mut body = format!(
        "Thank you for your purchase!\n\n\
         Item: {}\n\
         Amount: {} {:.2}\n",
        item_title, currency, amount,
    );

    if !purchase_note.is_empty() {
        body.push_str(&format!("Includes: {}\n", purchase_note));
    }

    body.push_str(&format!("\nDownload your file:\n{}\n", download_url));

    if let Some(key) = license_key {
        body.push_str(&format!("\nLicense Key: {}\n", key));
    }

    body.push_str(&format!(
        "\nPlease save this email for your records.\n\n— {}\n",
        site_name
    ));

    let subject = format!("Your purchase: {} — {}", item_title, site_name);

    if let Err(e) = send_via_configured_provider(&settings, &from, buyer_email, &subject, &body) {
        eprintln!("[email] Failed to send purchase email to {}: {}", buyer_email, e);
    } else {
        eprintln!("[email] Purchase email sent to {}", buyer_email);
    }
}

/// Determine the "from" email address from configured providers.
fn get_from_email(settings: &HashMap<String, String>) -> Option<String> {
    // Check providers in priority order
    if settings.get("email_gmail_enabled").map(|v| v.as_str()) == Some("true") {
        return settings.get("email_gmail_address").cloned().filter(|s| !s.is_empty());
    }
    if settings.get("email_smtp_enabled").map(|v| v.as_str()) == Some("true") {
        return settings.get("email_smtp_username").cloned().filter(|s| !s.is_empty());
    }
    // For API-based providers, use admin_email as from
    let api_providers = [
        "email_resend_enabled", "email_ses_enabled", "email_postmark_enabled",
        "email_brevo_enabled", "email_sendpulse_enabled", "email_mailgun_enabled",
        "email_moosend_enabled", "email_mandrill_enabled", "email_sparkpost_enabled",
    ];
    for provider in &api_providers {
        if settings.get(*provider).map(|v| v.as_str()) == Some("true") {
            return settings.get("admin_email").cloned().filter(|s| !s.is_empty());
        }
    }
    None
}

/// Send email via the first configured SMTP-compatible provider.
/// For Phase 2, we support Gmail SMTP and custom SMTP.
/// API-based providers (Resend, SES, etc.) would need HTTP clients — stubbed for now.
fn send_via_configured_provider(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    // Gmail SMTP
    if settings.get("email_gmail_enabled").map(|v| v.as_str()) == Some("true") {
        let address = settings.get("email_gmail_address").cloned().unwrap_or_default();
        let app_password = settings.get("email_gmail_app_password").cloned().unwrap_or_default();
        if !address.is_empty() && !app_password.is_empty() {
            return send_smtp(
                "smtp.gmail.com",
                587,
                &address,
                &app_password,
                from,
                to,
                subject,
                body,
            );
        }
    }

    // Custom SMTP
    if settings.get("email_smtp_enabled").map(|v| v.as_str()) == Some("true") {
        let host = settings.get("email_smtp_host").cloned().unwrap_or_default();
        let port: u16 = settings.get("email_smtp_port").and_then(|v| v.parse().ok()).unwrap_or(587);
        let username = settings.get("email_smtp_username").cloned().unwrap_or_default();
        let password = settings.get("email_smtp_password").cloned().unwrap_or_default();
        if !host.is_empty() && !username.is_empty() {
            return send_smtp(&host, port, &username, &password, from, to, subject, body);
        }
    }

    Err("No SMTP provider configured. API-based providers require HTTP integration.".to_string())
}

fn send_smtp(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let email = Message::builder()
        .from(from.parse().map_err(|e| format!("Invalid from address: {}", e))?)
        .to(to.parse().map_err(|e| format!("Invalid to address: {}", e))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {}", e))?;

    let creds = Credentials::new(username.to_string(), password.to_string());

    let mailer = SmtpTransport::starttls_relay(host)
        .map_err(|e| format!("SMTP relay error: {}", e))?
        .port(port)
        .credentials(creds)
        .build();

    mailer.send(&email).map_err(|e| format!("SMTP send error: {}", e))?;
    Ok(())
}
