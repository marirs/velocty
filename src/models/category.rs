use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub r#type: String,
}

#[derive(Debug, Deserialize)]
pub struct CategoryForm {
    pub name: String,
    pub slug: String,
    pub r#type: String,
}

impl Category {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Category {
            id: row.get("id")?,
            name: row.get("name")?,
            slug: row.get("slug")?,
            r#type: row.get("type")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM categories WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_slug(pool: &DbPool, slug: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM categories WHERE slug = ?1",
            params![slug],
            Self::from_row,
        )
        .ok()
    }

    pub fn list(pool: &DbPool, type_filter: Option<&str>) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match type_filter {
            Some(t) => (
                "SELECT * FROM categories WHERE type = ?1 OR type = 'both' ORDER BY name".to_string(),
                vec![Box::new(t.to_string())],
            ),
            None => (
                "SELECT * FROM categories ORDER BY name".to_string(),
                vec![],
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

    pub fn list_paginated(pool: &DbPool, type_filter: Option<&str>, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match type_filter {
            Some(t) => (
                "SELECT * FROM categories WHERE type = ?1 OR type = 'both' ORDER BY name LIMIT ?2 OFFSET ?3".to_string(),
                vec![Box::new(t.to_string()), Box::new(limit), Box::new(offset)],
            ),
            None => (
                "SELECT * FROM categories ORDER BY name LIMIT ?1 OFFSET ?2".to_string(),
                vec![Box::new(limit), Box::new(offset)],
            ),
        };
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        stmt.query_map(params_refs.as_slice(), Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count(pool: &DbPool, type_filter: Option<&str>) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        match type_filter {
            Some(t) => conn.query_row("SELECT COUNT(*) FROM categories WHERE type = ?1 OR type = 'both'", params![t], |row| row.get(0)).unwrap_or(0),
            None => conn.query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0)).unwrap_or(0),
        }
    }

    pub fn for_content(pool: &DbPool, content_id: i64, content_type: &str) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT c.* FROM categories c
             JOIN content_categories cc ON cc.category_id = c.id
             WHERE cc.content_id = ?1 AND cc.content_type = ?2
             ORDER BY c.name",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![content_id, content_type], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count_items(pool: &DbPool, category_id: i64) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM content_categories WHERE category_id = ?1",
            params![category_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    pub fn create(pool: &DbPool, form: &CategoryForm) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO categories (name, slug, type) VALUES (?1, ?2, ?3)",
            params![form.name, form.slug, form.r#type],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update(pool: &DbPool, id: i64, form: &CategoryForm) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE categories SET name = ?1, slug = ?2, type = ?3 WHERE id = ?4",
            params![form.name, form.slug, form.r#type, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM content_categories WHERE category_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM categories WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_for_content(
        pool: &DbPool,
        content_id: i64,
        content_type: &str,
        category_ids: &[i64],
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM content_categories WHERE content_id = ?1 AND content_type = ?2",
            params![content_id, content_type],
        )
        .map_err(|e| e.to_string())?;

        for cat_id in category_ids {
            conn.execute(
                "INSERT INTO content_categories (content_id, content_type, category_id) VALUES (?1, ?2, ?3)",
                params![content_id, content_type, cat_id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
