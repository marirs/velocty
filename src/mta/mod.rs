pub mod deliver;
pub mod dkim;
pub mod dns;
pub mod queue;

use std::collections::HashMap;

use crate::store::Store;

/// Send an email via the built-in MTA (direct-to-MX delivery with DKIM signing).
/// This is the entry point called by the email provider chain.
pub fn send(
    settings: &HashMap<String, String>,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    // Rate limit check
    // (The actual rate limiting is done via the Store in the queue task,
    //  but for synchronous sends we check the setting here)
    deliver::send_direct(settings, from, to, subject, body)
}

/// Initialize DKIM keys if not already generated.
/// Called during startup / seed_defaults.
pub fn init_dkim_if_needed(store: &dyn Store) {
    let existing = store
        .setting_get("mta_dkim_private_key")
        .unwrap_or_default();
    if !existing.is_empty() {
        return;
    }

    log::info!("[mta] Generating DKIM keypair...");
    match dkim::generate_keypair() {
        Ok((private_pem, _public_b64)) => {
            let _ = store.setting_set("mta_dkim_private_key", &private_pem);
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let _ = store.setting_set("mta_dkim_generated_at", &now);
            log::info!("[mta] DKIM keypair generated");
        }
        Err(e) => {
            log::error!("[mta] Failed to generate DKIM keypair: {}", e);
        }
    }
}

/// Regenerate DKIM keys (called from admin API).
pub fn regenerate_dkim(store: &dyn Store) -> Result<String, String> {
    let (private_pem, public_b64) = dkim::generate_keypair()?;
    store.setting_set("mta_dkim_private_key", &private_pem)?;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    store.setting_set("mta_dkim_generated_at", &now)?;
    Ok(public_b64)
}

/// Auto-populate the MTA from address from site_url if not already set.
pub fn init_from_address(store: &dyn Store) {
    let existing = store.setting_get("mta_from_address").unwrap_or_default();
    if !existing.is_empty() {
        return;
    }

    let site_url = store
        .setting_get("site_url")
        .unwrap_or_else(|| "http://localhost:8000".to_string());
    let from = deliver::default_from_address(&site_url);
    let _ = store.setting_set("mta_from_address", &from);
    log::info!("[mta] Auto-set from address to {}", from);
}

/// Process the email queue: send pending messages, handle retries.
pub fn process_queue(store: &dyn Store) {
    let settings = store.setting_all();

    // Rate limit: check how many sent in the last hour
    let max_per_hour = settings
        .get("mta_max_emails_per_hour")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(queue::DEFAULT_MAX_EMAILS_PER_HOUR);

    let sent_last_hour = store.mta_queue_sent_last_hour().unwrap_or(0);
    if sent_last_hour >= max_per_hour {
        log::warn!(
            "[mta] Rate limit reached ({}/{}), skipping queue processing",
            sent_last_hour,
            max_per_hour
        );
        return;
    }

    let remaining = max_per_hour - sent_last_hour;
    let pending = store.mta_queue_pending(remaining as i64);

    for msg in pending {
        // Mark as sending
        let _ = store.mta_queue_update_status(msg.id, "sending", None, None);

        match deliver::send_direct(
            &settings,
            &msg.from_addr,
            &msg.to_addr,
            &msg.subject,
            &msg.body_text,
        ) {
            Ok(()) => {
                let _ = store.mta_queue_update_status(msg.id, "sent", None, None);
                log::info!("[mta] Queue: sent email {} to {}", msg.id, msg.to_addr);
            }
            Err(e) => {
                let next_attempt = msg.attempts + 1;
                match queue::next_retry_timestamp(next_attempt) {
                    Some(next_retry) => {
                        let _ = store.mta_queue_update_status(
                            msg.id,
                            "pending",
                            Some(&e),
                            Some(&next_retry),
                        );
                        log::warn!(
                            "[mta] Queue: email {} failed (attempt {}), retry at {}: {}",
                            msg.id,
                            next_attempt,
                            next_retry,
                            e
                        );
                    }
                    None => {
                        let _ = store.mta_queue_update_status(msg.id, "failed", Some(&e), None);
                        log::error!(
                            "[mta] Queue: email {} permanently failed after {} attempts: {}",
                            msg.id,
                            next_attempt,
                            e
                        );
                    }
                }
            }
        }
    }
}
