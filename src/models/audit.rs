use chrono::NaiveDateTime;
use rusqlite::params;
use serde::Serialize;

use crate::db::DbPool;

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub user_id: Option<i64>,
    pub user_name: Option<String>,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub entity_title: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: NaiveDateTime,
}

impl AuditEntry {
    pub fn log(
        pool: &DbPool,
        user_id: Option<i64>,
        user_name: Option<&str>,
        action: &str,
        entity_type: Option<&str>,
        entity_id: Option<i64>,
        entity_title: Option<&str>,
        details: Option<&str>,
        ip_address: Option<&str>,
    ) {
        if let Ok(conn) = pool.get() {
            let _ = conn.execute(
                "INSERT INTO audit_log (user_id, user_name, action, entity_type, entity_id, entity_title, details, ip_address)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![user_id, user_name, action, entity_type, entity_id, entity_title, details, ip_address],
            );
        }
    }

    pub fn list(
        pool: &DbPool,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut sql = "SELECT * FROM audit_log WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(a) = action_filter {
            sql.push_str(&format!(" AND action = ?{}", idx));
            params_vec.push(Box::new(a.to_string()));
            idx += 1;
        }
        if let Some(e) = entity_filter {
            sql.push_str(&format!(" AND entity_type = ?{}", idx));
            params_vec.push(Box::new(e.to_string()));
            idx += 1;
        }
        if let Some(u) = user_filter {
            sql.push_str(&format!(" AND user_id = ?{}", idx));
            params_vec.push(Box::new(u));
            idx += 1;
        }

        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{} OFFSET ?{}", idx, idx + 1));
        params_vec.push(Box::new(limit));
        params_vec.push(Box::new(offset));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        conn.prepare(&sql)
            .and_then(|mut stmt| {
                stmt.query_map(param_refs.as_slice(), |row| {
                    Ok(AuditEntry {
                        id: row.get("id")?,
                        user_id: row.get("user_id")?,
                        user_name: row.get("user_name")?,
                        action: row.get("action")?,
                        entity_type: row.get("entity_type")?,
                        entity_id: row.get("entity_id")?,
                        entity_title: row.get("entity_title")?,
                        details: row.get("details")?,
                        ip_address: row.get("ip_address")?,
                        created_at: row.get("created_at")?,
                    })
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default()
    }

    pub fn count(
        pool: &DbPool,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
    ) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let mut sql = "SELECT COUNT(*) FROM audit_log WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(a) = action_filter {
            sql.push_str(&format!(" AND action = ?{}", idx));
            params_vec.push(Box::new(a.to_string()));
            idx += 1;
        }
        if let Some(e) = entity_filter {
            sql.push_str(&format!(" AND entity_type = ?{}", idx));
            params_vec.push(Box::new(e.to_string()));
            idx += 1;
        }
        if let Some(u) = user_filter {
            sql.push_str(&format!(" AND user_id = ?{}", idx));
            params_vec.push(Box::new(u));
            let _ = idx + 1;
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn distinct_actions(pool: &DbPool) -> Vec<String> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        conn.prepare("SELECT DISTINCT action FROM audit_log ORDER BY action")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default()
    }

    pub fn distinct_entity_types(pool: &DbPool) -> Vec<String> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        conn.prepare("SELECT DISTINCT entity_type FROM audit_log WHERE entity_type IS NOT NULL ORDER BY entity_type")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default()
    }

    pub fn cleanup(pool: &DbPool, max_age_days: i64) -> Result<usize, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let deleted = conn
            .execute(
                "DELETE FROM audit_log WHERE created_at < datetime('now', ?1)",
                params![format!("-{} days", max_age_days)],
            )
            .map_err(|e| e.to_string())?;
        Ok(deleted)
    }
}
