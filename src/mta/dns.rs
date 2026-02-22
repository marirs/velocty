use std::collections::HashMap;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::Resolver;

use super::deliver::domain_from_url;

/// Result of a DNS health check for a single record type.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DnsCheckResult {
    pub record_type: String,
    pub dns_type: String,      // "TXT", "PTR", etc.
    pub dns_name: String,      // e.g. "@", "velocty._domainkey.example.com"
    pub status: String,        // "ok", "warning", "error", "info", "unchecked"
    pub current_value: String, // what's currently in DNS
    pub recommended: String,   // what we recommend (the value to put in DNS)
    pub message: String,       // human-readable explanation
}

/// Run all DNS checks for the configured domain.
pub fn check_all(settings: &HashMap<String, String>) -> Vec<DnsCheckResult> {
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let domain = match domain_from_url(&site_url) {
        Some(d) => d,
        None => {
            return vec![error_result(
                "Domain",
                "Could not extract domain from site_url",
            )]
        }
    };
    let from_addr = settings
        .get("mta_from_address")
        .cloned()
        .unwrap_or_else(|| format!("noreply@{}", domain));
    let selector = settings
        .get("mta_dkim_selector")
        .cloned()
        .unwrap_or_else(|| "velocty".to_string());
    let dkim_private = settings
        .get("mta_dkim_private_key")
        .cloned()
        .unwrap_or_default();

    let resolver = match Resolver::new(ResolverConfig::default(), ResolverOpts::default()) {
        Ok(r) => r,
        Err(e) => return vec![error_result("DNS", &format!("Resolver init failed: {}", e))],
    };

    let mut results = Vec::new();

    // Detect server IP
    let server_ip = detect_server_ip(&resolver, &domain);

    // DKIM check (first in the table)
    if !dkim_private.is_empty() {
        let pub_key = super::dkim::public_key_from_private_pem(&dkim_private).unwrap_or_default();
        results.push(check_dkim(&resolver, &domain, &selector, &pub_key));
    } else {
        results.push(DnsCheckResult {
            record_type: "DKIM".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: format!("{}._domainkey.{}", selector, domain),
            status: "warning".to_string(),
            current_value: String::new(),
            recommended: "Enable Built-in Email to auto-generate DKIM keys".to_string(),
            message: "No DKIM key generated yet".to_string(),
        });
    }

    // SPF check
    results.push(check_spf(&resolver, &domain, server_ip.as_deref()));

    // DMARC check
    results.push(check_dmarc(&resolver, &domain, &from_addr));

    // PTR check (informational)
    if let Some(ref ip) = server_ip {
        results.push(check_ptr(&resolver, ip, &domain));
    }

    results
}

/// Detect the server's public IP by resolving the domain's A record.
fn detect_server_ip(resolver: &Resolver, domain: &str) -> Option<String> {
    resolver
        .lookup_ip(domain)
        .ok()
        .and_then(|response| response.iter().next().map(|ip| ip.to_string()))
}

/// Check SPF record. Merges with existing if needed.
fn check_spf(resolver: &Resolver, domain: &str, server_ip: Option<&str>) -> DnsCheckResult {
    let existing_spf = lookup_txt_records(resolver, domain)
        .into_iter()
        .find(|r| r.starts_with("v=spf1"));

    let recommended = generate_spf(existing_spf.as_deref(), server_ip);

    match existing_spf {
        None => DnsCheckResult {
            record_type: "SPF".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: "@".to_string(),
            status: "error".to_string(),
            current_value: String::new(),
            recommended,
            message: "No SPF record found. Add a TXT record to your domain.".to_string(),
        },
        Some(ref current) => {
            let has_server = server_ip
                .map(|ip| {
                    current.contains(ip) || current.contains(" a ") || current.contains(" a:")
                })
                .unwrap_or(false);
            if has_server {
                DnsCheckResult {
                    record_type: "SPF".to_string(),
                    dns_type: "TXT".to_string(),
                    dns_name: "@".to_string(),
                    status: "ok".to_string(),
                    current_value: current.clone(),
                    recommended: current.clone(),
                    message: "SPF record includes this server.".to_string(),
                }
            } else {
                DnsCheckResult {
                    record_type: "SPF".to_string(),
                    dns_type: "TXT".to_string(),
                    dns_name: "@".to_string(),
                    status: "warning".to_string(),
                    current_value: current.clone(),
                    recommended,
                    message: "SPF record exists but doesn't include this server. Update with the recommended value.".to_string(),
                }
            }
        }
    }
}

/// Generate an SPF record, merging with existing if present.
pub fn generate_spf(existing: Option<&str>, server_ip: Option<&str>) -> String {
    match existing {
        None => {
            // Fresh SPF
            match server_ip {
                Some(ip) => format!("v=spf1 a mx ip4:{} ~all", ip),
                None => "v=spf1 a mx ~all".to_string(),
            }
        }
        Some(current) => merge_spf(current, server_ip),
    }
}

/// Merge our server into an existing SPF record.
pub fn merge_spf(existing: &str, server_ip: Option<&str>) -> String {
    // Parse existing mechanisms
    let parts: Vec<&str> = existing.split_whitespace().collect();

    // Check if already included
    let has_a = parts.iter().any(|p| *p == "a" || p.starts_with("a:"));
    let has_ip = server_ip
        .map(|ip| {
            let ip4 = format!("ip4:{}", ip);
            let ip6 = format!("ip6:{}", ip);
            parts.iter().any(|p| *p == ip4 || *p == ip6)
        })
        .unwrap_or(false);

    if has_a || has_ip {
        return existing.to_string();
    }

    // Find the position of the "all" qualifier (last mechanism)
    let mut mechanisms: Vec<String> = Vec::new();
    let mut all_qualifier = "~all".to_string();

    for part in &parts {
        if *part == "~all" || *part == "-all" || *part == "?all" || *part == "+all" {
            all_qualifier = part.to_string();
        } else {
            mechanisms.push(part.to_string());
        }
    }

    // Insert our mechanisms before the all qualifier
    mechanisms.push("a".to_string());
    if let Some(ip) = server_ip {
        mechanisms.push(format!("ip4:{}", ip));
    }
    mechanisms.push(all_qualifier);

    mechanisms.join(" ")
}

/// Check DKIM record.
fn check_dkim(
    resolver: &Resolver,
    domain: &str,
    selector: &str,
    expected_pubkey: &str,
) -> DnsCheckResult {
    let dkim_domain = format!("{}._domainkey.{}", selector, domain);
    let txt_records = lookup_txt_records(resolver, &dkim_domain);
    let dkim_record = txt_records.iter().find(|r| r.contains("v=DKIM1"));

    let recommended = format!("v=DKIM1; k=rsa; p={}", expected_pubkey);

    match dkim_record {
        None => DnsCheckResult {
            record_type: "DKIM".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: dkim_domain.clone(),
            status: "error".to_string(),
            current_value: String::new(),
            recommended,
            message: format!(
                "No DKIM record found. Add a TXT record for '{}'.",
                dkim_domain
            ),
        },
        Some(current) => {
            let has_key = current.contains(expected_pubkey);
            if has_key {
                DnsCheckResult {
                    record_type: "DKIM".to_string(),
                    dns_type: "TXT".to_string(),
                    dns_name: dkim_domain,
                    status: "ok".to_string(),
                    current_value: current.clone(),
                    recommended,
                    message: "DKIM record matches the generated key.".to_string(),
                }
            } else {
                DnsCheckResult {
                    record_type: "DKIM".to_string(),
                    dns_type: "TXT".to_string(),
                    dns_name: dkim_domain,
                    status: "warning".to_string(),
                    current_value: current.clone(),
                    recommended,
                    message: "DKIM record exists but the public key doesn't match. Update with the recommended value.".to_string(),
                }
            }
        }
    }
}

/// Check DMARC record.
fn check_dmarc(resolver: &Resolver, domain: &str, from_addr: &str) -> DnsCheckResult {
    let dmarc_domain = format!("_dmarc.{}", domain);
    let txt_records = lookup_txt_records(resolver, &dmarc_domain);
    let dmarc_record = txt_records.iter().find(|r| r.starts_with("v=DMARC1"));

    let recommended = format!("v=DMARC1; p=none; rua=mailto:{}", from_addr);

    match dmarc_record {
        None => DnsCheckResult {
            record_type: "DMARC".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: format!("_dmarc.{}", domain),
            status: "error".to_string(),
            current_value: String::new(),
            recommended,
            message: format!(
                "No DMARC record found. Add a TXT record for '_dmarc.{}'.",
                domain
            ),
        },
        Some(current) => DnsCheckResult {
            record_type: "DMARC".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: format!("_dmarc.{}", domain),
            status: "ok".to_string(),
            current_value: current.clone(),
            recommended,
            message: "DMARC record found.".to_string(),
        },
    }
}

/// Check PTR (reverse DNS) for the server IP.
fn check_ptr(resolver: &Resolver, ip: &str, expected_domain: &str) -> DnsCheckResult {
    let addr: std::net::IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => {
            return DnsCheckResult {
                record_type: "PTR".to_string(),
                dns_type: "PTR".to_string(),
                dns_name: ip.to_string(),
                status: "info".to_string(),
                current_value: String::new(),
                recommended: expected_domain.to_string(),
                message: "Could not check PTR record.".to_string(),
            }
        }
    };

    match resolver.reverse_lookup(addr) {
        Ok(response) => {
            let names: Vec<String> = response.iter().map(|n| n.to_ascii()).collect();
            let matches = names
                .iter()
                .any(|n| n.trim_end_matches('.') == expected_domain);
            if matches {
                DnsCheckResult {
                    record_type: "PTR".to_string(),
                    dns_type: "PTR".to_string(),
                    dns_name: ip.to_string(),
                    status: "ok".to_string(),
                    current_value: names.join(", "),
                    recommended: expected_domain.to_string(),
                    message: "Reverse DNS matches your domain.".to_string(),
                }
            } else {
                DnsCheckResult {
                    record_type: "PTR".to_string(),
                    dns_type: "PTR".to_string(),
                    dns_name: ip.to_string(),
                    status: "warning".to_string(),
                    current_value: if names.is_empty() {
                        String::new()
                    } else {
                        names.join(", ")
                    },
                    recommended: expected_domain.to_string(),
                    message: "PTR record doesn't match your domain. Contact your hosting provider to set reverse DNS.".to_string(),
                }
            }
        }
        Err(_) => DnsCheckResult {
            record_type: "PTR".to_string(),
            dns_type: "PTR".to_string(),
            dns_name: ip.to_string(),
            status: "warning".to_string(),
            current_value: String::new(),
            recommended: expected_domain.to_string(),
            message: "No PTR record found. Contact your hosting provider to set reverse DNS."
                .to_string(),
        },
    }
}

/// Look up TXT records for a domain.
fn lookup_txt_records(resolver: &Resolver, domain: &str) -> Vec<String> {
    match resolver.txt_lookup(domain) {
        Ok(response) => response
            .iter()
            .map(|txt| {
                txt.iter()
                    .map(|data| String::from_utf8_lossy(data).to_string())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn error_result(record_type: &str, message: &str) -> DnsCheckResult {
    DnsCheckResult {
        record_type: record_type.to_string(),
        dns_type: String::new(),
        dns_name: String::new(),
        status: "error".to_string(),
        current_value: String::new(),
        recommended: String::new(),
        message: message.to_string(),
    }
}

/// Generate the required DNS records without checking live DNS.
/// Used for the static "Required Records" display.
pub fn required_records(settings: &HashMap<String, String>) -> Vec<DnsCheckResult> {
    let site_url = settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let domain = match domain_from_url(&site_url) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let from_addr = settings
        .get("mta_from_address")
        .cloned()
        .unwrap_or_else(|| format!("noreply@{}", domain));
    let selector = settings
        .get("mta_dkim_selector")
        .cloned()
        .unwrap_or_else(|| "velocty".to_string());
    let dkim_private = settings
        .get("mta_dkim_private_key")
        .cloned()
        .unwrap_or_default();

    let mut records = Vec::new();

    // DKIM
    if !dkim_private.is_empty() {
        let pub_key = super::dkim::public_key_from_private_pem(&dkim_private).unwrap_or_default();
        records.push(DnsCheckResult {
            record_type: "DKIM".to_string(),
            dns_type: "TXT".to_string(),
            dns_name: format!("{}._domainkey.{}", selector, domain),
            status: "unchecked".to_string(),
            current_value: String::new(),
            recommended: format!("v=DKIM1; k=rsa; p={}", pub_key),
            message: "DKIM signing key for outgoing emails.".to_string(),
        });
    }

    // SPF
    records.push(DnsCheckResult {
        record_type: "SPF".to_string(),
        dns_type: "TXT".to_string(),
        dns_name: "@".to_string(),
        status: "unchecked".to_string(),
        current_value: String::new(),
        recommended: "v=spf1 a mx ~all".to_string(),
        message: "Authorizes this server to send email for your domain.".to_string(),
    });

    // DMARC
    records.push(DnsCheckResult {
        record_type: "DMARC".to_string(),
        dns_type: "TXT".to_string(),
        dns_name: format!("_dmarc.{}", domain),
        status: "unchecked".to_string(),
        current_value: String::new(),
        recommended: format!("v=DMARC1; p=none; rua=mailto:{}", from_addr),
        message: "Tells receiving servers how to handle authentication failures.".to_string(),
    });

    records
}
