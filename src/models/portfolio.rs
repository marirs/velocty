use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PortfolioItem {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub description_json: Option<String>,
    pub description_html: Option<String>,
    pub image_path: String,
    pub thumbnail_path: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub sell_enabled: bool,
    pub price: Option<f64>,
    pub purchase_note: String,
    pub payment_provider: String,
    pub download_file_path: String,
    pub likes: i64,
    pub status: String,
    pub published_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub seo_score: i32,
    pub seo_issues: String,
}

#[derive(Debug, Deserialize)]
pub struct PortfolioForm {
    pub title: String,
    pub slug: String,
    pub description_json: Option<String>,
    pub description_html: Option<String>,
    pub image_path: String,
    pub thumbnail_path: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub sell_enabled: Option<bool>,
    pub price: Option<f64>,
    pub purchase_note: Option<String>,
    pub payment_provider: Option<String>,
    pub download_file_path: Option<String>,
    pub status: String,
    pub published_at: Option<String>,
    pub category_ids: Option<Vec<i64>>,
    pub tag_ids: Option<Vec<i64>>,
}

impl PortfolioItem {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let sell_raw: i64 = row.get("sell_enabled")?;
        Ok(PortfolioItem {
            id: row.get("id")?,
            title: row.get("title")?,
            slug: row.get("slug")?,
            description_json: row.get("description_json")?,
            description_html: row.get("description_html")?,
            image_path: row.get("image_path")?,
            thumbnail_path: row.get("thumbnail_path")?,
            meta_title: row.get("meta_title")?,
            meta_description: row.get("meta_description")?,
            sell_enabled: sell_raw != 0,
            price: row.get("price")?,
            purchase_note: row
                .get::<_, Option<String>>("purchase_note")?
                .unwrap_or_default(),
            payment_provider: row
                .get::<_, Option<String>>("payment_provider")?
                .unwrap_or_default(),
            download_file_path: row
                .get::<_, Option<String>>("download_file_path")?
                .unwrap_or_default(),
            likes: row.get("likes")?,
            status: row.get("status")?,
            published_at: row.get("published_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            seo_score: row.get("seo_score").unwrap_or(-1),
            seo_issues: row.get("seo_issues").unwrap_or_else(|_| "[]".to_string()),
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM portfolio WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_slug(pool: &DbPool, slug: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM portfolio WHERE slug = ?1",
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
                "SELECT * FROM portfolio WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
                    .to_string(),
                vec![Box::new(s.to_string()), Box::new(limit), Box::new(offset)],
            ),
            None => (
                "SELECT * FROM portfolio ORDER BY created_at DESC LIMIT ?1 OFFSET ?2".to_string(),
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
                    "SELECT COUNT(*) FROM portfolio WHERE status = ?1",
                    params![s],
                    |row| row.get(0),
                )
                .unwrap_or(0),
            None => conn
                .query_row("SELECT COUNT(*) FROM portfolio", [], |row| row.get(0))
                .unwrap_or(0),
        }
    }

    pub fn published(pool: &DbPool, limit: i64, offset: i64) -> Vec<Self> {
        Self::list(pool, Some("published"), limit, offset)
    }

    pub fn by_category(pool: &DbPool, category_slug: &str, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = conn
            .prepare(
                "SELECT p.* FROM portfolio p
                 JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'portfolio'
                 JOIN categories c ON c.id = cc.category_id
                 WHERE c.slug = ?1 AND p.status = 'published'
                 ORDER BY p.created_at DESC LIMIT ?2 OFFSET ?3",
            )
            .ok();

        match &mut stmt {
            Some(s) => s
                .query_map(params![category_slug, limit, offset], Self::from_row)
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            None => vec![],
        }
    }

    pub fn create(pool: &DbPool, form: &PortfolioForm) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let published_at: Option<NaiveDateTime> = form
            .published_at
            .as_ref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok());

        conn.execute(
            "INSERT INTO portfolio (title, slug, description_json, description_html, image_path, thumbnail_path,
             meta_title, meta_description, sell_enabled, price, purchase_note, payment_provider, download_file_path, status, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                form.title,
                form.slug,
                form.description_json,
                form.description_html,
                form.image_path,
                form.thumbnail_path,
                form.meta_title,
                form.meta_description,
                form.sell_enabled.unwrap_or(false) as i64,
                form.price,
                form.purchase_note.as_deref().unwrap_or(""),
                form.payment_provider.as_deref().unwrap_or(""),
                form.download_file_path.as_deref().unwrap_or(""),
                form.status,
                published_at,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }

    pub fn update(pool: &DbPool, id: i64, form: &PortfolioForm) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let published_at: Option<NaiveDateTime> = form
            .published_at
            .as_ref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok());

        conn.execute(
            "UPDATE portfolio SET title=?1, slug=?2, description_json=?3, description_html=?4,
             image_path=?5, thumbnail_path=?6, meta_title=?7, meta_description=?8,
             sell_enabled=?9, price=?10, purchase_note=?11, payment_provider=?12, download_file_path=?13, status=?14, published_at=?15,
             updated_at=CURRENT_TIMESTAMP WHERE id=?16",
            params![
                form.title,
                form.slug,
                form.description_json,
                form.description_html,
                form.image_path,
                form.thumbnail_path,
                form.meta_title,
                form.meta_description,
                form.sell_enabled.unwrap_or(false) as i64,
                form.price,
                form.purchase_note.as_deref().unwrap_or(""),
                form.payment_provider.as_deref().unwrap_or(""),
                form.download_file_path.as_deref().unwrap_or(""),
                form.status,
                published_at,
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn update_status(pool: &DbPool, id: i64, status: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE portfolio SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM content_categories WHERE content_id = ?1 AND content_type = 'portfolio'",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM content_tags WHERE content_id = ?1 AND content_type = 'portfolio'",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM likes WHERE portfolio_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM portfolio WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn increment_likes(pool: &DbPool, id: i64) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE portfolio SET likes = likes + 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row(
                "SELECT likes FROM portfolio WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn decrement_likes(pool: &DbPool, id: i64) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE portfolio SET likes = MAX(0, likes - 1) WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row(
                "SELECT likes FROM portfolio WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn update_seo_score(
        pool: &DbPool,
        id: i64,
        score: i32,
        issues_json: &str,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE portfolio SET seo_score = ?1, seo_issues = ?2 WHERE id = ?3",
            params![score, issues_json, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
