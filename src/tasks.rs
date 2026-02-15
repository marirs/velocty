use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Orbit, Rocket};
use rocket::tokio;
use std::sync::Arc;
use std::time::Duration;

use crate::db::DbPool;
use crate::models::settings::Setting;

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
        let pool = rocket
            .state::<DbPool>()
            .expect("DbPool not found in managed state")
            .clone();
        let pool = Arc::new(pool);

        // Session cleanup task
        let p = Arc::clone(&pool);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&p, "task_session_cleanup_interval", 30);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&p, "task_session_max_age_days", 30);
                match cleanup_sessions(&p, max_age) {
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
        let p = Arc::clone(&pool);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&p, "task_magic_link_cleanup_interval", 60);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                match cleanup_magic_links(&p) {
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
        let p = Arc::clone(&pool);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&p, "task_scheduled_publish_interval", 1);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                match publish_scheduled(&p) {
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
        let p = Arc::clone(&pool);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&p, "task_analytics_cleanup_interval", 1440);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&p, "task_analytics_max_age_days", 365);
                match cleanup_analytics(&p, max_age) {
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
        let p = Arc::clone(&pool);
        tokio::spawn(async move {
            loop {
                let interval = get_interval(&p, "task_audit_log_cleanup_interval", 1440);
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;
                let max_age = get_setting_i64(&p, "task_audit_log_max_age_days", 90);
                match crate::models::audit::AuditEntry::cleanup(&p, max_age) {
                    Ok(count) => {
                        if count > 0 {
                            log::info!("[task] Cleaned up {} old audit log entries", count);
                        }
                    }
                    Err(e) => log::error!("[task] Audit log cleanup failed: {}", e),
                }
            }
        });

        log::info!("[task] Background tasks started");
    }
}

fn get_interval(pool: &DbPool, key: &str, default: u64) -> u64 {
    Setting::get_or(pool, key, &default.to_string())
        .parse::<u64>()
        .unwrap_or(default)
        .max(1)
}

fn get_setting_i64(pool: &DbPool, key: &str, default: i64) -> i64 {
    Setting::get_or(pool, key, &default.to_string())
        .parse::<i64>()
        .unwrap_or(default)
}

fn cleanup_sessions(pool: &DbPool, max_age_days: i64) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let deleted = conn
        .execute(
            "DELETE FROM sessions WHERE expires_at < datetime('now') OR created_at < datetime('now', ?1)",
            rusqlite::params![format!("-{} days", max_age_days)],
        )
        .map_err(|e| e.to_string())?;
    Ok(deleted)
}

fn cleanup_magic_links(pool: &DbPool) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let deleted = conn
        .execute(
            "DELETE FROM magic_links WHERE expires_at < datetime('now') OR used = 1",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(deleted)
}

fn publish_scheduled(pool: &DbPool) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let posts = conn
        .execute(
            "UPDATE posts SET status = 'published', updated_at = CURRENT_TIMESTAMP WHERE status = 'scheduled' AND published_at <= datetime('now')",
            [],
        )
        .map_err(|e| e.to_string())?;
    let portfolio = conn
        .execute(
            "UPDATE portfolio SET status = 'published', updated_at = CURRENT_TIMESTAMP WHERE status = 'scheduled' AND published_at <= datetime('now')",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(posts + portfolio)
}

fn cleanup_analytics(pool: &DbPool, max_age_days: i64) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let deleted = conn
        .execute(
            "DELETE FROM page_views WHERE created_at < datetime('now', ?1)",
            rusqlite::params![format!("-{} days", max_age_days)],
        )
        .map_err(|e| e.to_string())?;
    Ok(deleted)
}
