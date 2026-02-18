use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: String,   // admin, editor, author, subscriber
    pub status: String, // active, suspended, locked
    pub avatar: String,
    pub mfa_enabled: bool,
    pub mfa_secret: String,
    pub mfa_recovery_codes: String,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let mfa_int: i32 = row.get(7)?;
        Ok(User {
            id: row.get(0)?,
            email: row.get(1)?,
            password_hash: row.get(2)?,
            display_name: row.get(3)?,
            role: row.get(4)?,
            status: row.get(5)?,
            avatar: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            mfa_enabled: mfa_int != 0,
            mfa_secret: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
            mfa_recovery_codes: row
                .get::<_, Option<String>>(9)?
                .unwrap_or_else(|| "[]".to_string()),
            last_login_at: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }

    const SELECT_COLS: &'static str =
        "id, email, password_hash, display_name, role, status, avatar, mfa_enabled, mfa_secret, mfa_recovery_codes, last_login_at, created_at, updated_at";

    // ── Lookups ──

    pub fn get_by_id(pool: &DbPool, id: i64) -> Option<User> {
        let conn = pool.get().ok()?;
        conn.query_row(
            &format!("SELECT {} FROM users WHERE id = ?1", Self::SELECT_COLS),
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn get_by_email(pool: &DbPool, email: &str) -> Option<User> {
        let conn = pool.get().ok()?;
        conn.query_row(
            &format!("SELECT {} FROM users WHERE email = ?1", Self::SELECT_COLS),
            params![email],
            Self::from_row,
        )
        .ok()
    }

    pub fn list_all(pool: &DbPool) -> Vec<User> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(&format!(
            "SELECT {} FROM users ORDER BY id ASC",
            Self::SELECT_COLS
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn list_paginated(pool: &DbPool, role: Option<&str>, limit: i64, offset: i64) -> Vec<User> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match role {
            Some(r) => (
                format!(
                    "SELECT {} FROM users WHERE role = ?1 ORDER BY id ASC LIMIT ?2 OFFSET ?3",
                    Self::SELECT_COLS
                ),
                vec![Box::new(r.to_string()), Box::new(limit), Box::new(offset)],
            ),
            None => (
                format!(
                    "SELECT {} FROM users ORDER BY id ASC LIMIT ?1 OFFSET ?2",
                    Self::SELECT_COLS
                ),
                vec![Box::new(limit), Box::new(offset)],
            ),
        };
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        stmt.query_map(params_refs.as_slice(), Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count_filtered(pool: &DbPool, role: Option<&str>) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        match role {
            Some(r) => conn
                .query_row(
                    "SELECT COUNT(*) FROM users WHERE role = ?1",
                    params![r],
                    |row| row.get(0),
                )
                .unwrap_or(0),
            None => conn
                .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
                .unwrap_or(0),
        }
    }

    pub fn count(pool: &DbPool) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn count_by_role(pool: &DbPool, role: &str) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM users WHERE role = ?1",
            params![role],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    // ── Create ──

    pub fn create(
        pool: &DbPool,
        email: &str,
        password_hash: &str,
        display_name: &str,
        role: &str,
    ) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO users (email, password_hash, display_name, role, status)
             VALUES (?1, ?2, ?3, ?4, 'active')",
            params![email, password_hash, display_name, role],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    // ── Update ──

    pub fn update_profile(
        pool: &DbPool,
        id: i64,
        display_name: &str,
        email: &str,
        avatar: &str,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET display_name = ?1, email = ?2, avatar = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?4",
            params![display_name, email, avatar, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_role(pool: &DbPool, id: i64, role: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET role = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![role, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_password(pool: &DbPool, id: i64, password_hash: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET password_hash = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![password_hash, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_avatar(pool: &DbPool, id: i64, avatar: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET avatar = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![avatar, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn touch_last_login(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET last_login_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── MFA ──

    pub fn update_mfa(
        pool: &DbPool,
        id: i64,
        enabled: bool,
        secret: &str,
        recovery_codes: &str,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let mfa_int: i32 = if enabled { 1 } else { 0 };
        conn.execute(
            "UPDATE users SET mfa_enabled = ?1, mfa_secret = ?2, mfa_recovery_codes = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?4",
            params![mfa_int, secret, recovery_codes, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Status management ──

    pub fn lock(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET status = 'locked', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn unlock(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET status = 'active', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Delete ──

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        // Invalidate sessions
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        // Nullify content ownership (don't delete their posts)
        conn.execute(
            "UPDATE posts SET user_id = NULL WHERE user_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE portfolio SET user_id = NULL WHERE user_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        // Delete user
        conn.execute("DELETE FROM users WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Helpers ──

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn is_editor_or_above(&self) -> bool {
        self.role == "admin" || self.role == "editor"
    }

    pub fn is_author_or_above(&self) -> bool {
        self.role == "admin" || self.role == "editor" || self.role == "author"
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Return a safe version without password_hash for template contexts
    pub fn safe_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "email": self.email,
            "display_name": self.display_name,
            "role": self.role,
            "status": self.status,
            "avatar": self.avatar,
            "mfa_enabled": self.mfa_enabled,
            "last_login_at": self.last_login_at,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}
