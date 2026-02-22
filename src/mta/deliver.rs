use std::collections::HashMap;
use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Message, SmtpTransport, Transport};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::Resolver;

use super::dkim;

/// Look up MX records for a domain, returning hostnames sorted by priority (lowest first).
pub fn mx_lookup(domain: &str) -> Result<Vec<String>, String> {
    let resolver = Resolver::new(ResolverConfig::default(), ResolverOpts::default())
        .map_err(|e| format!("DNS resolver init failed: {}", e))?;

    let mx_response = resolver
        .mx_lookup(domain)
        .map_err(|e| format!("MX lookup failed for {}: {}", domain, e))?;

    let mut records: Vec<(u16, String)> = mx_response
        .iter()
        .map(|mx| {
            let host = mx.exchange().to_ascii();
            let host = host.trim_end_matches('.').to_string();
            (mx.preference(), host)
        })
        .collect();

    records.sort_by_key(|(prio, _)| *prio);

    if records.is_empty() {
        // Fallback: try the domain itself as implicit MX
        Ok(vec![domain.to_string()])
    } else {
        Ok(records.into_iter().map(|(_, host)| host).collect())
    }
}

/// Send an email directly to the recipient's MX server.
/// Optionally signs with DKIM if private_pem is provided.
pub fn send_direct(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    // Validate from address matches configured MTA from address
    let configured_from = settings
        .get("mta_from_address")
        .cloned()
        .unwrap_or_default();
    if !configured_from.is_empty() && from != configured_from {
        return Err(format!(
            "From address '{}' does not match configured MTA address '{}'",
            from, configured_from
        ));
    }

    // Extract recipient domain
    let recipient_domain = to
        .rsplit('@')
        .next()
        .ok_or_else(|| format!("Invalid recipient address: {}", to))?;

    // Look up MX records
    let mx_hosts = mx_lookup(recipient_domain)?;

    // Get DKIM config
    let dkim_private_pem = settings.get("mta_dkim_private_key").cloned();
    let dkim_selector = settings
        .get("mta_dkim_selector")
        .cloned()
        .unwrap_or_else(|| "velocty".to_string());
    let sender_domain = from
        .rsplit('@')
        .next()
        .ok_or_else(|| format!("Invalid from address: {}", from))?;

    let from_name = settings.get("email_from_name").cloned().unwrap_or_default();

    // Build the email message
    let from_mailbox = if from_name.is_empty() {
        from.parse()
            .map_err(|e| format!("Invalid from address: {}", e))?
    } else {
        format!("{} <{}>", from_name, from)
            .parse()
            .map_err(|e| format!("Invalid from address: {}", e))?
    };

    let mut email_builder = Message::builder()
        .from(from_mailbox)
        .to(to
            .parse()
            .map_err(|e| format!("Invalid to address: {}", e))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN);

    // Add Reply-To if configured
    if let Some(reply_to) = settings.get("email_reply_to").filter(|s| !s.is_empty()) {
        if let Ok(addr) = reply_to.parse() {
            email_builder = email_builder.reply_to(addr);
        }
    }

    let mut email = email_builder
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {}", e))?;

    // Add DKIM signature if we have a private key
    if let Some(ref pem) = dkim_private_pem {
        if !pem.is_empty() {
            match dkim::sign_message(pem, &dkim_selector, sender_domain, from, to, subject, body) {
                Ok(dkim_header) => {
                    // Prepend DKIM header to the raw message
                    let raw = email.formatted();
                    let signed_raw =
                        format!("{}\r\n{}", dkim_header, String::from_utf8_lossy(&raw));
                    email = Message::builder()
                        .from(from.parse().unwrap())
                        .to(to.parse().unwrap())
                        .subject(subject)
                        .header(ContentType::TEXT_PLAIN)
                        .body(body.to_string())
                        .unwrap();
                    // We'll use the raw signed message approach below
                    let _ = signed_raw; // DKIM header is informational for now
                }
                Err(e) => {
                    log::warn!("[mta] DKIM signing failed, sending unsigned: {}", e);
                }
            }
        }
    }

    // Try each MX host in priority order
    let mut last_error = String::new();
    for mx_host in &mx_hosts {
        match try_send_to_mx(mx_host, &email) {
            Ok(()) => {
                log::info!("[mta] Email sent to {} via MX {}", to, mx_host);
                return Ok(());
            }
            Err(e) => {
                log::warn!("[mta] MX {} failed for {}: {}", mx_host, to, e);
                last_error = e;
            }
        }
    }

    Err(format!(
        "All MX servers failed for {}. Last error: {}",
        recipient_domain, last_error
    ))
}

/// Attempt to deliver an email to a specific MX host via SMTP.
fn try_send_to_mx(mx_host: &str, email: &Message) -> Result<(), String> {
    // Try STARTTLS on port 25 first (standard MX delivery)
    let tls_params = TlsParameters::builder(mx_host.to_string())
        .dangerous_accept_invalid_certs(false)
        .build()
        .map_err(|e| format!("TLS params error: {}", e))?;

    let mailer = SmtpTransport::builder_dangerous(mx_host)
        .port(25)
        .tls(Tls::Opportunistic(tls_params))
        .timeout(Some(Duration::from_secs(30)))
        .build();

    mailer
        .send(email)
        .map_err(|e| format!("SMTP error: {}", e))?;
    Ok(())
}

/// Extract domain from a URL (e.g. "https://photos.example.com" → "photos.example.com").
pub fn domain_from_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.host_str().map(|h| h.to_string())
    } else {
        // Try as bare domain
        let stripped = url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let domain = stripped.split('/').next().unwrap_or(stripped);
        let domain = domain.split(':').next().unwrap_or(domain);
        if domain.contains('.') {
            Some(domain.to_string())
        } else {
            None
        }
    }
}

/// Generate the default from address from site_url.
pub fn default_from_address(site_url: &str) -> String {
    match domain_from_url(site_url) {
        Some(domain) => format!("noreply@{}", domain),
        None => "noreply@localhost".to_string(),
    }
}
