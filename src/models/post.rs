use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Post {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub content_json: String,
    pub content_html: String,
    pub excerpt: Option<String>,
    pub featured_image: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub published_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct PostForm {
    pub title: String,
    pub slug: String,
    pub content_json: String,
    pub content_html: String,
    pub excerpt: Option<String>,
    pub featured_image: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub published_at: Option<String>,
    pub category_ids: Option<Vec<i64>>,
    pub tag_ids: Option<Vec<i64>>,
}

impl Post {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Post {
            id: row.get("id")?,
            title: row.get("title")?,
            slug: row.get("slug")?,
            content_json: row.get("content_json")?,
            content_html: row.get("content_html")?,
            excerpt: row.get("excerpt")?,
            featured_image: row.get("featured_image")?,
            meta_title: row.get("meta_title")?,
            meta_description: row.get("meta_description")?,
            status: row.get("status")?,
            published_at: row.get("published_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row("SELECT * FROM posts WHERE id = ?1", params![id], Self::from_row)
            .ok()
    }

    pub fn find_by_slug(pool: &DbPool, slug: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM posts WHERE slug = ?1",
            params![slug],
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
                "SELECT * FROM posts WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
                    .to_string(),
                vec![
                    Box::new(s.to_string()),
                    Box::new(limit),
                    Box::new(offset),
                ],
            ),
            None => (
                "SELECT * FROM posts ORDER BY created_at DESC LIMIT ?1 OFFSET ?2".to_string(),
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

    pub fn count(pool: &DbPool, status: Option<&str>) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        match status {
            Some(s) => conn
                .query_row(
                    "SELECT COUNT(*) FROM posts WHERE status = ?1",
                    params![s],
                    |row| row.get(0),
                )
                .unwrap_or(0),
            None => conn
                .query_row("SELECT COUNT(*) FROM posts", [], |row| row.get(0))
                .unwrap_or(0),
        }
    }

    pub fn published(pool: &DbPool, limit: i64, offset: i64) -> Vec<Self> {
        Self::list(pool, Some("published"), limit, offset)
    }

    pub fn create(pool: &DbPool, form: &PostForm) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let published_at: Option<NaiveDateTime> = form
            .published_at
            .as_ref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok());

        conn.execute(
            "INSERT INTO posts (title, slug, content_json, content_html, excerpt, featured_image, meta_title, meta_description, status, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                form.title,
                form.slug,
                form.content_json,
                form.content_html,
                form.excerpt,
                form.featured_image,
                form.meta_title,
                form.meta_description,
                form.status,
                published_at,
            ],
        )
        .map_err(|e| e.to_string())?;

        let id = conn.last_insert_rowid();
        Ok(id)
    }

    pub fn update(pool: &DbPool, id: i64, form: &PostForm) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let published_at: Option<NaiveDateTime> = form
            .published_at
            .as_ref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok());

        conn.execute(
            "UPDATE posts SET title=?1, slug=?2, content_json=?3, content_html=?4, excerpt=?5,
             featured_image=?6, meta_title=?7, meta_description=?8, status=?9, published_at=?10,
             updated_at=CURRENT_TIMESTAMP WHERE id=?11",
            params![
                form.title,
                form.slug,
                form.content_json,
                form.content_html,
                form.excerpt,
                form.featured_image,
                form.meta_title,
                form.meta_description,
                form.status,
                published_at,
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM content_categories WHERE content_id = ?1 AND content_type = 'post'", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM content_tags WHERE content_id = ?1 AND content_type = 'post'", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM comments WHERE post_id = ?1 AND content_type = 'post'", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM posts WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get the previous published post (older, by published_at)
    pub fn prev_published(pool: &DbPool, published_at: &NaiveDateTime) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM posts WHERE status = 'published' AND published_at < ?1 ORDER BY published_at DESC LIMIT 1",
            params![published_at],
            Self::from_row,
        ).ok()
    }

    /// Get the next published post (newer, by published_at)
    pub fn next_published(pool: &DbPool, published_at: &NaiveDateTime) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM posts WHERE status = 'published' AND published_at > ?1 ORDER BY published_at ASC LIMIT 1",
            params![published_at],
            Self::from_row,
        ).ok()
    }

    pub fn update_status(pool: &DbPool, id: i64, status: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE posts SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
