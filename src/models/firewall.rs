use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FwBan {
    pub id: i64,
    pub ip: String,
    pub reason: String,
    pub detail: Option<String>,
    pub banned_at: String,
    pub expires_at: Option<String>,
    pub country: Option<String>,
    pub user_agent: Option<String>,
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FwEvent {
    pub id: i64,
    pub ip: String,
    pub event_type: String,
    pub detail: Option<String>,
    pub country: Option<String>,
    pub user_agent: Option<String>,
    pub request_path: Option<String>,
    pub created_at: String,
}

impl FwBan {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(FwBan {
            id: row.get("id")?,
            ip: row.get("ip")?,
            reason: row.get("reason")?,
            detail: row.get("detail")?,
            banned_at: row.get("banned_at")?,
            expires_at: row.get("expires_at")?,
            country: row.get("country")?,
            user_agent: row.get("user_agent")?,
            active: row.get::<_, i64>("active")? == 1,
        })
    }

    /// Check if an IP is currently banned (active and not expired)
    pub fn is_banned(pool: &DbPool, ip: &str) -> bool {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM fw_bans WHERE ip = ?1 AND active = 1
             AND (expires_at IS NULL OR expires_at > datetime('now'))",
            params![ip],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    /// Create a new ban entry
    pub fn create(
        pool: &DbPool,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        expires_at: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;

        // Deactivate any existing active bans for this IP first
        let _ = conn.execute(
            "UPDATE fw_bans SET active = 0 WHERE ip = ?1 AND active = 1",
            params![ip],
        );

        conn.execute(
            "INSERT INTO fw_bans (ip, reason, detail, expires_at, country, user_agent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![ip, reason, detail, expires_at, country, user_agent],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }

    /// Create a ban with a duration string like "1h", "24h", "7d", "30d", "permanent"
    pub fn create_with_duration(
        pool: &DbPool,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        duration: &str,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        let expires = duration_to_expiry(duration);
        Self::create(pool, ip, reason, detail, expires.as_deref(), country, user_agent)
    }

    /// Unban an IP (deactivate all active bans)
    pub fn unban(pool: &DbPool, ip: &str) -> Result<usize, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE fw_bans SET active = 0 WHERE ip = ?1 AND active = 1",
            params![ip],
        )
        .map_err(|e| e.to_string())
    }

    /// Unban by ID
    pub fn unban_by_id(pool: &DbPool, id: i64) -> Result<usize, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE fw_bans SET active = 0 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())
    }

    /// List active bans
    pub fn active_bans(pool: &DbPool, limit: i64, offset: i64) -> Vec<FwBan> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM fw_bans WHERE active = 1
             AND (expires_at IS NULL OR expires_at > datetime('now'))
             ORDER BY banned_at DESC LIMIT ?1 OFFSET ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    /// Count active bans
    pub fn active_count(pool: &DbPool) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM fw_bans WHERE active = 1
             AND (expires_at IS NULL OR expires_at > datetime('now'))",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// All bans (including expired/inactive) for history
    pub fn all_bans(pool: &DbPool, limit: i64, offset: i64) -> Vec<FwBan> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM fw_bans ORDER BY banned_at DESC LIMIT ?1 OFFSET ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    /// Expire stale bans (mark inactive if past expiry)
    pub fn expire_stale(pool: &DbPool) {
        if let Ok(conn) = pool.get() {
            let _ = conn.execute(
                "UPDATE fw_bans SET active = 0 WHERE active = 1 AND expires_at IS NOT NULL AND expires_at <= datetime('now')",
                [],
            );
        }
    }
}

impl FwEvent {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(FwEvent {
            id: row.get("id")?,
            ip: row.get("ip")?,
            event_type: row.get("event_type")?,
            detail: row.get("detail")?,
            country: row.get("country")?,
            user_agent: row.get("user_agent")?,
            request_path: row.get("request_path")?,
            created_at: row.get("created_at")?,
        })
    }

    /// Log a firewall event
    pub fn log(
        pool: &DbPool,
        ip: &str,
        event_type: &str,
        detail: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
        request_path: Option<&str>,
    ) {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return,
        };
        let _ = conn.execute(
            "INSERT INTO fw_events (ip, event_type, detail, country, user_agent, request_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![ip, event_type, detail, country, user_agent, request_path],
        );

        // Auto-prune: keep only the most recent 10,000 events
        let _ = conn.execute(
            "DELETE FROM fw_events WHERE id NOT IN (SELECT id FROM fw_events ORDER BY id DESC LIMIT 10000)",
            [],
        );
    }

    /// Recent events with optional type filter
    pub fn recent(pool: &DbPool, event_type: Option<&str>, limit: i64, offset: i64) -> Vec<FwEvent> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        if let Some(et) = event_type {
            let mut stmt = match conn.prepare(
                "SELECT * FROM fw_events WHERE event_type = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
            ) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            stmt.query_map(params![et, limit, offset], Self::from_row)
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        } else {
            let mut stmt = match conn.prepare(
                "SELECT * FROM fw_events ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            ) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            stmt.query_map(params![limit, offset], Self::from_row)
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        }
    }

    /// Count events in the last N hours
    pub fn count_since_hours(pool: &DbPool, hours: i64) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM fw_events WHERE created_at > datetime('now', '-{} hours')",
                hours
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Count events for a specific IP in the last N minutes (for threshold checks)
    pub fn count_for_ip_since(pool: &DbPool, ip: &str, event_type: &str, minutes: i64) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM fw_events WHERE ip = ?1 AND event_type = ?2
                 AND created_at > datetime('now', '-{} minutes')",
                minutes
            ),
            params![ip, event_type],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Top offending IPs in the last 24 hours
    pub fn top_ips(pool: &DbPool, limit: i64) -> Vec<(String, i64)> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT ip, COUNT(*) as cnt FROM fw_events
             WHERE created_at > datetime('now', '-24 hours')
             GROUP BY ip ORDER BY cnt DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Event counts grouped by type in the last 24 hours
    pub fn counts_by_type(pool: &DbPool) -> Vec<(String, i64)> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT event_type, COUNT(*) as cnt FROM fw_events
             WHERE created_at > datetime('now', '-24 hours')
             GROUP BY event_type ORDER BY cnt DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

/// Convert a duration string like "1h", "6h", "24h", "7d", "30d", "permanent" to an expiry datetime string
fn duration_to_expiry(duration: &str) -> Option<String> {
    if duration == "permanent" || duration.is_empty() {
        return None;
    }
    let (num, unit) = if duration.ends_with('d') {
        let n: i64 = duration.trim_end_matches('d').parse().unwrap_or(1);
        (n, "days")
    } else if duration.ends_with('h') {
        let n: i64 = duration.trim_end_matches('h').parse().unwrap_or(1);
        (n, "hours")
    } else {
        // Default: treat as hours
        let n: i64 = duration.parse().unwrap_or(24);
        (n, "hours")
    };

    let now = chrono::Utc::now().naive_utc();
    let expiry = match unit {
        "days" => now + chrono::Duration::days(num),
        _ => now + chrono::Duration::hours(num),
    };
    Some(expiry.format("%Y-%m-%d %H:%M:%S").to_string())
}
