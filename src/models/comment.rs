use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Comment {
    pub id: i64,
    pub post_id: i64,
    pub content_type: String,
    pub author_name: String,
    pub author_email: Option<String>,
    pub body: String,
    pub status: String,
    pub parent_id: Option<i64>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CommentForm {
    pub post_id: i64,
    pub content_type: Option<String>,
    pub author_name: String,
    pub author_email: Option<String>,
    pub body: String,
    pub honeypot: Option<String>,
    pub parent_id: Option<i64>,
}

impl Comment {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Comment {
            id: row.get("id")?,
            post_id: row.get("post_id")?,
            content_type: row.get("content_type")?,
            author_name: row.get("author_name")?,
            author_email: row.get("author_email")?,
            body: row.get("body")?,
            status: row.get("status")?,
            parent_id: row.get("parent_id").ok(),
            created_at: row.get("created_at")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM comments WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn list(pool: &DbPool, status: Option<&str>, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT * FROM comments WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
                    .to_string(),
                vec![Box::new(s.to_string()), Box::new(limit), Box::new(offset)],
            ),
            None => (
                "SELECT * FROM comments ORDER BY created_at DESC LIMIT ?1 OFFSET ?2".to_string(),
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

    pub fn for_post(pool: &DbPool, post_id: i64, content_type: &str) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM comments WHERE post_id = ?1 AND content_type = ?2 AND status = 'approved' ORDER BY created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![post_id, content_type], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count(pool: &DbPool, status: Option<&str>) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        match status {
            Some(s) => conn
                .query_row(
                    "SELECT COUNT(*) FROM comments WHERE status = ?1",
                    params![s],
                    |row| row.get(0),
                )
                .unwrap_or(0),
            None => conn
                .query_row("SELECT COUNT(*) FROM comments", [], |row| row.get(0))
                .unwrap_or(0),
        }
    }

    pub fn create(pool: &DbPool, form: &CommentForm) -> Result<i64, String> {
        // Honeypot check — if filled, it's a bot
        if let Some(ref hp) = form.honeypot {
            if !hp.is_empty() {
                return Err("Spam detected".to_string());
            }
        }

        let conn = pool.get().map_err(|e| e.to_string())?;
        let ct = form.content_type.as_deref().unwrap_or("post");

        conn.execute(
            "INSERT INTO comments (post_id, content_type, author_name, author_email, body, status, parent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
            params![form.post_id, ct, form.author_name, form.author_email, form.body, form.parent_id],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }

    pub fn update_status(pool: &DbPool, id: i64, status: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE comments SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM comments WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
