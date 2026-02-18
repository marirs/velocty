use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct TagForm {
    pub name: String,
    pub slug: String,
}

impl Tag {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Tag {
            id: row.get("id")?,
            name: row.get("name")?,
            slug: row.get("slug")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM tags WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_slug(pool: &DbPool, slug: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM tags WHERE slug = ?1",
            params![slug],
            Self::from_row,
        )
        .ok()
    }

    pub fn list(pool: &DbPool) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare("SELECT * FROM tags ORDER BY name") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn list_paginated(pool: &DbPool, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare("SELECT * FROM tags ORDER BY name LIMIT ?1 OFFSET ?2") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count(pool: &DbPool) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn for_content(pool: &DbPool, content_id: i64, content_type: &str) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT t.* FROM tags t
             JOIN content_tags ct ON ct.tag_id = t.id
             WHERE ct.content_id = ?1 AND ct.content_type = ?2
             ORDER BY t.name",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![content_id, content_type], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count_items(pool: &DbPool, tag_id: i64) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM content_tags WHERE tag_id = ?1",
            params![tag_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    pub fn create(pool: &DbPool, form: &TagForm) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO tags (name, slug) VALUES (?1, ?2)",
            params![form.name, form.slug],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update(pool: &DbPool, id: i64, form: &TagForm) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE tags SET name = ?1, slug = ?2 WHERE id = ?3",
            params![form.name, form.slug, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM content_tags WHERE tag_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_for_content(
        pool: &DbPool,
        content_id: i64,
        content_type: &str,
        tag_ids: &[i64],
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM content_tags WHERE content_id = ?1 AND content_type = ?2",
            params![content_id, content_type],
        )
        .map_err(|e| e.to_string())?;

        for tag_id in tag_ids {
            conn.execute(
                "INSERT OR IGNORE INTO content_tags (content_id, content_type, tag_id) VALUES (?1, ?2, ?3)",
                params![content_id, content_type, tag_id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn find_or_create(pool: &DbPool, name: &str) -> Result<i64, String> {
        let slug_str = slug::slugify(name);
        if let Some(existing) = Self::find_by_slug(pool, &slug_str) {
            return Ok(existing.id);
        }
        Self::create(
            pool,
            &TagForm {
                name: name.to_string(),
                slug: slug_str,
            },
        )
    }
}
