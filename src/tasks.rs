use rocket::fairing::{Fairing, Info, Kind};
use rocket::tokio;
use rocket::{Orbit, Rocket};
use std::sync::Arc;
use std::time::Duration;

use crate::store::Store;

pub struct BackgroundTasks;

#[rocket::async_trait]
impl Fairing for BackgroundTasks {
    fn info(&self) -> Info {
        Info {
            name: "Background Tasks",
            kind: Kind::Liftoff,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let store = rocket
            .state::<Arc<dyn Store>>()
            .expect("Store not found in managed state")
            .clone();

        // Session cleanup task
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_session_cleanup_interval", 30);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&*s, "task_session_max_age_days", 30);
                match s.task_cleanup_sessions(max_age) {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Cleaned up {} expired sessions", count);
                        }
                    }
                    Err(e) => log::error!("[task] Session cleanup failed: {}", e),
                }
            }
        });

        // Magic link cleanup task
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_magic_link_cleanup_interval", 60);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                match s.magic_link_cleanup() {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Cleaned up {} expired magic link tokens", count);
                        }
                    }
                    Err(e) => log::error!("[task] Magic link cleanup failed: {}", e),
                }
            }
        });

        // Scheduled publish task
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_scheduled_publish_interval", 1);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                match s.task_publish_scheduled() {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Published {} scheduled items", count);
                        }
                    }
                    Err(e) => log::error!("[task] Scheduled publish failed: {}", e),
                }
            }
        });

        // Analytics cleanup task
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_analytics_cleanup_interval", 1440);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&*s, "task_analytics_max_age_days", 365);
                match s.task_cleanup_analytics(max_age) {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Cleaned up {} old analytics records", count);
                        }
                    }
                    Err(e) => log::error!("[task] Analytics cleanup failed: {}", e),
                }
            }
        });

        // Audit log cleanup task
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_audit_log_cleanup_interval", 1440);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&*s, "task_audit_log_max_age_days", 90);
                match s.audit_cleanup(max_age) {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Cleaned up {} old audit log entries", count);
                        }
                    }
                    Err(e) => log::error!("[task] Audit log cleanup failed: {}", e),
                }
            }
        });

        // Initialize built-in MTA (DKIM keys + from address)
        crate::mta::init_dkim_if_needed(&*store);
        crate::mta::init_from_address(&*store);

        // Email queue processing task (built-in MTA)
        let s = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&*s, "task_email_queue_interval", 1);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                crate::mta::process_queue(&*s);
                // Cleanup old queue entries (keep 30 days)
                let _ = s.mta_queue_cleanup(30);
            }
        });

        log::info!("[task] Background tasks started");
    }
}

fn get_interval(store: &dyn Store, key: &str, default: u64) -> u64 {
    store
        .setting_get_or(key, &default.to_string())
        .parse::<u64>()
        .unwrap_or(default)
        .max(1)
}

fn get_setting_i64(store: &dyn Store, key: &str, default: i64) -> i64 {
    store
        .setting_get_or(key, &default.to_string())
        .parse::<i64>()
        .unwrap_or(default)
}
