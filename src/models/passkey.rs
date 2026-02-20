use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasskey {
    pub id: i64,
    pub user_id: i64,
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: i64,
    pub transports: String,
    pub name: String,
    pub created_at: String,
}

impl UserPasskey {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(UserPasskey {
            id: row.get(0)?,
            user_id: row.get(1)?,
            credential_id: row.get(2)?,
            public_key: row.get(3)?,
            sign_count: row.get(4)?,
            transports: row
                .get::<_, Option<String>>(5)?
                .unwrap_or_else(|| "[]".to_string()),
            name: row
                .get::<_, Option<String>>(6)?
                .unwrap_or_else(|| "Passkey".to_string()),
            created_at: row.get(7)?,
        })
    }

    const SELECT_COLS: &'static str =
        "id, user_id, credential_id, public_key, sign_count, transports, name, created_at";

    /// List all passkeys for a user
    pub fn list_for_user(pool: &DbPool, user_id: i64) -> Vec<UserPasskey> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(&format!(
            "SELECT {} FROM user_passkeys WHERE user_id = ?1 ORDER BY created_at ASC",
            Self::SELECT_COLS
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![user_id], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    /// Get a passkey by credential_id
    pub fn get_by_credential_id(pool: &DbPool, credential_id: &str) -> Option<UserPasskey> {
        let conn = pool.get().ok()?;
        conn.query_row(
            &format!(
                "SELECT {} FROM user_passkeys WHERE credential_id = ?1",
                Self::SELECT_COLS
            ),
            params![credential_id],
            Self::from_row,
        )
        .ok()
    }

    /// Count passkeys for a user
    pub fn count_for_user(pool: &DbPool, user_id: i64) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM user_passkeys WHERE user_id = ?1",
            params![user_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Create a new passkey
    pub fn create(
        pool: &DbPool,
        user_id: i64,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
        transports: &str,
        name: &str,
    ) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO user_passkeys (user_id, credential_id, public_key, sign_count, transports, name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![user_id, credential_id, public_key, sign_count, transports, name],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    /// Update sign_count after successful authentication
    pub fn update_sign_count(
        pool: &DbPool,
        credential_id: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE user_passkeys SET sign_count = ?1 WHERE credential_id = ?2",
            params![sign_count, credential_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a passkey by id (must belong to user)
    pub fn delete(pool: &DbPool, id: i64, user_id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let affected = conn
            .execute(
                "DELETE FROM user_passkeys WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("Passkey not found".into());
        }
        Ok(())
    }

    /// Delete all passkeys for a user
    pub fn delete_all_for_user(pool: &DbPool, user_id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM user_passkeys WHERE user_id = ?1",
            params![user_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
