use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Design {
    pub id: i64,
    pub name: String,
    pub layout_html: String,
    pub style_css: String,
    pub thumbnail_path: Option<String>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DesignTemplate {
    pub id: i64,
    pub design_id: i64,
    pub template_type: String,
    pub layout_html: String,
    pub style_css: String,
    pub updated_at: NaiveDateTime,
}

impl Design {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let active_raw: i64 = row.get("is_active")?;
        Ok(Design {
            id: row.get("id")?,
            name: row.get("name")?,
            layout_html: row.get("layout_html")?,
            style_css: row.get("style_css")?,
            thumbnail_path: row.get("thumbnail_path")?,
            is_active: active_raw != 0,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM designs WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn active(pool: &DbPool) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM designs WHERE is_active = 1 LIMIT 1",
            [],
            Self::from_row,
        )
        .ok()
    }

    pub fn list(pool: &DbPool) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare("SELECT * FROM designs ORDER BY created_at DESC") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn activate(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute("UPDATE designs SET is_active = 0", [])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE designs SET is_active = 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create(pool: &DbPool, name: &str) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO designs (name, layout_html, style_css) VALUES (?1, '', '')",
            params![name],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn duplicate(pool: &DbPool, id: i64, new_name: &str) -> Result<i64, String> {
        let original = Self::find_by_id(pool, id).ok_or("Design not found")?;
        let conn = pool.get().map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO designs (name, layout_html, style_css) VALUES (?1, ?2, ?3)",
            params![new_name, original.layout_html, original.style_css],
        )
        .map_err(|e| e.to_string())?;

        let new_id = conn.last_insert_rowid();

        // Duplicate all templates
        let templates = DesignTemplate::for_design(pool, id);
        for tmpl in templates {
            conn.execute(
                "INSERT INTO design_templates (design_id, template_type, layout_html, style_css)
                 VALUES (?1, ?2, ?3, ?4)",
                params![new_id, tmpl.template_type, tmpl.layout_html, tmpl.style_css],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(new_id)
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM design_templates WHERE design_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM designs WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl DesignTemplate {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(DesignTemplate {
            id: row.get("id")?,
            design_id: row.get("design_id")?,
            template_type: row.get("template_type")?,
            layout_html: row.get("layout_html")?,
            style_css: row.get("style_css")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn for_design(pool: &DbPool, design_id: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM design_templates WHERE design_id = ?1 ORDER BY template_type",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![design_id], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get(pool: &DbPool, design_id: i64, template_type: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM design_templates WHERE design_id = ?1 AND template_type = ?2",
            params![design_id, template_type],
            Self::from_row,
        )
        .ok()
    }

    pub fn upsert(
        pool: &DbPool,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO design_templates (design_id, template_type, layout_html, style_css)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(design_id, template_type)
             DO UPDATE SET layout_html = ?3, style_css = ?4, updated_at = CURRENT_TIMESTAMP",
            params![design_id, template_type, layout_html, style_css],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
