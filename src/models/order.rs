use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Order {
    pub id: i64,
    pub uuid: String,
    pub portfolio_id: i64,
    pub buyer_email: String,
    pub buyer_name: String,
    pub amount: f64,
    pub currency: String,
    pub provider: String,
    pub provider_order_id: String,
    pub status: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadToken {
    pub id: i64,
    pub order_id: i64,
    pub token: String,
    pub downloads_used: i64,
    pub max_downloads: i64,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct License {
    pub id: i64,
    pub order_id: i64,
    pub license_key: String,
    pub created_at: NaiveDateTime,
}

impl Order {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Order {
            id: row.get("id")?,
            uuid: row.get::<_, Option<String>>("uuid")?.unwrap_or_default(),
            portfolio_id: row.get("portfolio_id")?,
            buyer_email: row.get("buyer_email")?,
            buyer_name: row
                .get::<_, Option<String>>("buyer_name")?
                .unwrap_or_default(),
            amount: row.get("amount")?,
            currency: row.get("currency")?,
            provider: row.get("provider")?,
            provider_order_id: row
                .get::<_, Option<String>>("provider_order_id")?
                .unwrap_or_default(),
            status: row.get("status")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn find_by_id(pool: &DbPool, id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM orders WHERE id = ?1",
            params![id],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_uuid(pool: &DbPool, uuid: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM orders WHERE uuid = ?1",
            params![uuid],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_provider_order_id(pool: &DbPool, provider_order_id: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM orders WHERE provider_order_id = ?1",
            params![provider_order_id],
            Self::from_row,
        )
        .ok()
    }

    pub fn list(pool: &DbPool, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn
            .prepare("SELECT * FROM orders ORDER BY created_at DESC LIMIT ?1 OFFSET ?2")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn list_by_status(pool: &DbPool, status: &str, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM orders WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![status, limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn list_by_email(pool: &DbPool, email: &str, limit: i64, offset: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM orders WHERE buyer_email = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![email, limit, offset], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn list_by_portfolio(pool: &DbPool, portfolio_id: i64) -> Vec<Self> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn
            .prepare("SELECT * FROM orders WHERE portfolio_id = ?1 ORDER BY created_at DESC")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![portfolio_id], Self::from_row)
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn count(pool: &DbPool) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM orders", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn count_by_status(pool: &DbPool, status: &str) -> i64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM orders WHERE status = ?1",
            params![status],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    pub fn total_revenue(pool: &DbPool) -> f64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0.0,
        };
        conn.query_row(
            "SELECT COALESCE(SUM(amount), 0.0) FROM orders WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0.0)
    }

    pub fn revenue_by_period(pool: &DbPool, days: i64) -> f64 {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return 0.0,
        };
        conn.query_row(
            "SELECT COALESCE(SUM(amount), 0.0) FROM orders WHERE status = 'completed' AND created_at >= datetime('now', ?1)",
            params![format!("-{} days", days)],
            |row| row.get(0),
        )
        .unwrap_or(0.0)
    }

    pub fn create(
        pool: &DbPool,
        portfolio_id: i64,
        buyer_email: &str,
        buyer_name: &str,
        amount: f64,
        currency: &str,
        provider: &str,
        provider_order_id: &str,
        status: &str,
    ) -> Result<(i64, String), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let order_uuid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO orders (portfolio_id, buyer_email, buyer_name, amount, currency, provider, provider_order_id, status, uuid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![portfolio_id, buyer_email, buyer_name, amount, currency, provider, provider_order_id, status, order_uuid],
        )
        .map_err(|e| e.to_string())?;
        Ok((conn.last_insert_rowid(), order_uuid))
    }

    pub fn update_status(pool: &DbPool, id: i64, status: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE orders SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_provider_order_id(
        pool: &DbPool,
        id: i64,
        provider_order_id: &str,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE orders SET provider_order_id = ?1 WHERE id = ?2",
            params![provider_order_id, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl DownloadToken {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(DownloadToken {
            id: row.get("id")?,
            order_id: row.get("order_id")?,
            token: row.get("token")?,
            downloads_used: row.get("downloads_used")?,
            max_downloads: row.get("max_downloads")?,
            expires_at: row.get("expires_at")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn find_by_token(pool: &DbPool, token: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM download_tokens WHERE token = ?1",
            params![token],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_order(pool: &DbPool, order_id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM download_tokens WHERE order_id = ?1",
            params![order_id],
            Self::from_row,
        )
        .ok()
    }

    pub fn is_valid(&self) -> bool {
        let now = chrono::Utc::now().naive_utc();
        self.downloads_used < self.max_downloads && self.expires_at > now
    }

    pub fn increment_download(pool: &DbPool, id: i64) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE download_tokens SET downloads_used = downloads_used + 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create(
        pool: &DbPool,
        order_id: i64,
        token: &str,
        max_downloads: i64,
        expires_at: NaiveDateTime,
    ) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO download_tokens (order_id, token, max_downloads, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![order_id, token, max_downloads, expires_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}

impl License {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(License {
            id: row.get("id")?,
            order_id: row.get("order_id")?,
            license_key: row.get("license_key")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn find_by_order(pool: &DbPool, order_id: i64) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM licenses WHERE order_id = ?1",
            params![order_id],
            Self::from_row,
        )
        .ok()
    }

    pub fn find_by_key(pool: &DbPool, key: &str) -> Option<Self> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT * FROM licenses WHERE license_key = ?1",
            params![key],
            Self::from_row,
        )
        .ok()
    }

    pub fn create(pool: &DbPool, order_id: i64, license_key: &str) -> Result<i64, String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO licenses (order_id, license_key) VALUES (?1, ?2)",
            params![order_id, license_key],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}
