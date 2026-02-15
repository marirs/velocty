pub mod gmail;
pub mod smtp;
pub mod resend;
pub mod ses;
pub mod postmark;
pub mod brevo;
pub mod sendpulse;
pub mod mailgun;
pub mod moosend;
pub mod mandrill;
pub mod sparkpost;

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
    // Use explicit from address if configured
    let from_addr = settings.get("email_from_address").cloned().unwrap_or_default();
    if !from_addr.is_empty() {
        return Some(from_addr);
    }

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

/// Send email via the first configured provider in the failover chain.
fn send_via_configured_provider(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let failover_enabled = settings.get("email_failover_enabled").map(|v| v.as_str()) == Some("true");

    let chain_str = settings
        .get("email_failover_chain")
        .cloned()
        .unwrap_or_else(|| "gmail,resend,ses,postmark,brevo,sendpulse,mailgun,moosend,mandrill,sparkpost,smtp".to_string());

    let chain: Vec<&str> = chain_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    let mut last_error = String::new();

    for provider_name in &chain {
        let enabled_key = format!("email_{}_enabled", provider_name);
        if settings.get(&enabled_key).map(|v| v.as_str()) != Some("true") {
            continue;
        }

        let result = match *provider_name {
            "gmail" => gmail::send(settings, from, to, subject, body),
            "smtp" => smtp::send(settings, from, to, subject, body),
            "resend" => resend::send(settings, from, to, subject, body),
            "ses" => ses::send(settings, from, to, subject, body),
            "postmark" => postmark::send(settings, from, to, subject, body),
            "brevo" => brevo::send(settings, from, to, subject, body),
            "sendpulse" => sendpulse::send(settings, from, to, subject, body),
            "mailgun" => mailgun::send(settings, from, to, subject, body),
            "moosend" => moosend::send(settings, from, to, subject, body),
            "mandrill" => mandrill::send(settings, from, to, subject, body),
            "sparkpost" => sparkpost::send(settings, from, to, subject, body),
            _ => {
                log::warn!("Unknown email provider: {}", provider_name);
                continue;
            }
        };

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("Email provider {} failed: {}", provider_name, e);
                last_error = e;
                if !failover_enabled {
                    // No failover — fail immediately
                    return Err(last_error);
                }
            }
        }
    }

    if last_error.is_empty() {
        Err("No email provider configured or enabled".into())
    } else {
        Err(format!("All email providers failed. Last error: {}", last_error))
    }
}

/// Shared SMTP send function used by gmail.rs and smtp.rs
pub fn send_smtp(
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
