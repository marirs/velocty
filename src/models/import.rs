use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Import {
    pub id: i64,
    pub source: String,
    pub filename: Option<String>,
    pub posts_count: i64,
    pub portfolio_count: i64,
    pub comments_count: i64,
    pub skipped_count: i64,
    pub log: Option<String>,
    pub imported_at: NaiveDateTime,
}

impl Import {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Import {
            id: row.get("id")?,
            source: row.get("source")?,
            filename: row.get("filename")?,
            posts_count: row.get("posts_count")?,
            portfolio_count: row.get("portfolio_count")?,
            comments_count: row.get("comments_count")?,
            skipped_count: row.get("skipped_count")?,
            log: row.get("log")?,
            imported_at: row.get("imported_at")?,
        })
    }

    pub fn list(pool: &DbPool) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt =
            match conn.prepare("SELECT * FROM imports ORDER BY imported_at DESC LIMIT 50") {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map([], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn create(
        pool: &DbPool,
        source: &str,
        filename: Option<&str>,
        posts_count: i64,
        portfolio_count: i64,
        comments_count: i64,
        skipped_count: i64,
        log: Option<&str>,
    ) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO imports (source, filename, posts_count, portfolio_count, comments_count, skipped_count, log)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![source, filename, posts_count, portfolio_count, comments_count, skipped_count, log],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}
