use chrono::NaiveDateTime;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Serialize, Deserialize)]
pub struct PageView {
    pub id: i64,
    pub path: String,
    pub ip_hash: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct OverviewStats {
    pub total_views: i64,
    pub unique_visitors: i64,
    pub posts_count: i64,
    pub portfolio_count: i64,
    pub comments_pending: i64,
    pub total_likes: i64,
}

#[derive(Debug, Serialize)]
pub struct FlowNode {
    pub source: String,
    pub target: String,
    pub value: i64,
}

#[derive(Debug, Serialize)]
pub struct CountEntry {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct DailyCount {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct StreamEntry {
    pub date: String,
    pub content_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct TagRelation {
    pub source: String,
    pub target: String,
    pub weight: i64,
}

impl PageView {
    pub fn record(
        pool: &DbPool,
        path: &str,
        ip_hash: &str,
        country: Option<&str>,
        city: Option<&str>,
        referrer: Option<&str>,
        user_agent: Option<&str>,
        device_type: Option<&str>,
        browser: Option<&str>,
    ) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO page_views (path, ip_hash, country, city, referrer, user_agent, device_type, browser)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![path, ip_hash, country, city, referrer, user_agent, device_type, browser],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn overview(pool: &DbPool, from: &str, to: &str) -> OverviewStats {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => {
                return OverviewStats {
                    total_views: 0,
                    unique_visitors: 0,
                    posts_count: 0,
                    portfolio_count: 0,
                    comments_pending: 0,
                    total_likes: 0,
                }
            }
        };

        let total_views: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM page_views WHERE created_at BETWEEN ?1 AND ?2",
                params![from, to],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let unique_visitors: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT ip_hash) FROM page_views WHERE created_at BETWEEN ?1 AND ?2",
                params![from, to],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let posts_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM posts", [], |row| row.get(0))
            .unwrap_or(0);

        let portfolio_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM portfolio", [], |row| row.get(0))
            .unwrap_or(0);

        let comments_pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM comments WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_likes: i64 = conn
            .query_row("SELECT COALESCE(SUM(likes), 0) FROM portfolio", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        OverviewStats {
            total_views,
            unique_visitors,
            posts_count,
            portfolio_count,
            comments_pending,
            total_likes,
        }
    }

    pub fn flow_data(pool: &DbPool, from: &str, to: &str) -> Vec<FlowNode> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        // Referrer -> Content type
        let mut stmt = match conn.prepare(
            "SELECT
                COALESCE(referrer, 'Direct') as source,
                CASE
                    WHEN path LIKE '/blog%' OR path LIKE '/journal%' THEN 'Blog'
                    WHEN path LIKE '/portfolio%' THEN 'Portfolio'
                    ELSE 'Pages'
                END as target,
                COUNT(*) as value
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             GROUP BY source, target
             ORDER BY value DESC
             LIMIT 50",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let mut results: Vec<FlowNode> = stmt
            .query_map(params![from, to], |row| {
                Ok(FlowNode {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    value: row.get(2)?,
                })
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        // Content type -> Top items
        let mut stmt2 = match conn.prepare(
            "SELECT
                CASE
                    WHEN path LIKE '/blog%' OR path LIKE '/journal%' THEN 'Blog'
                    WHEN path LIKE '/portfolio%' THEN 'Portfolio'
                    ELSE 'Pages'
                END as source,
                path as target,
                COUNT(*) as value
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             AND path != '/'
             GROUP BY source, target
             ORDER BY value DESC
             LIMIT 30",
        ) {
            Ok(s) => s,
            Err(_) => return results,
        };

        let items: Vec<FlowNode> = stmt2
            .query_map(params![from, to], |row| {
                Ok(FlowNode {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    value: row.get(2)?,
                })
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        results.extend(items);
        results
    }

    pub fn geo_data(pool: &DbPool, from: &str, to: &str) -> Vec<CountEntry> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT COALESCE(country, 'Unknown') as label, COUNT(*) as count
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             GROUP BY country
             ORDER BY count DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![from, to], |row| {
            Ok(CountEntry {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn stream_data(pool: &DbPool, from: &str, to: &str) -> Vec<StreamEntry> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT
                DATE(created_at) as date,
                CASE
                    WHEN path LIKE '/blog%' OR path LIKE '/journal%' THEN 'blog'
                    WHEN path LIKE '/portfolio%' THEN 'portfolio'
                    ELSE 'pages'
                END as content_type,
                COUNT(*) as count
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             GROUP BY date, content_type
             ORDER BY date",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![from, to], |row| {
            Ok(StreamEntry {
                date: row.get(0)?,
                content_type: row.get(1)?,
                count: row.get(2)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn calendar_data(pool: &DbPool, from: &str, to: &str) -> Vec<DailyCount> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT DATE(created_at) as date, COUNT(*) as count
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             GROUP BY date
             ORDER BY date",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![from, to], |row| {
            Ok(DailyCount {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn top_portfolio(pool: &DbPool, from: &str, to: &str, limit: i64) -> Vec<CountEntry> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT path as label, COUNT(*) as count
             FROM page_views
             WHERE path LIKE '/portfolio/%' AND created_at BETWEEN ?1 AND ?2
             GROUP BY path
             ORDER BY count DESC
             LIMIT ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![from, to, limit], |row| {
            Ok(CountEntry {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn top_referrers(pool: &DbPool, from: &str, to: &str, limit: i64) -> Vec<CountEntry> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT COALESCE(referrer, 'Direct') as label, COUNT(*) as count
             FROM page_views
             WHERE created_at BETWEEN ?1 AND ?2
             GROUP BY referrer
             ORDER BY count DESC
             LIMIT ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map(params![from, to, limit], |row| {
            Ok(CountEntry {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn tag_relations(pool: &DbPool) -> Vec<TagRelation> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut stmt = match conn.prepare(
            "SELECT t1.name as source, t2.name as target, COUNT(*) as weight
             FROM content_tags ct1
             JOIN content_tags ct2 ON ct1.content_id = ct2.content_id
                AND ct1.content_type = ct2.content_type
                AND ct1.tag_id < ct2.tag_id
             JOIN tags t1 ON t1.id = ct1.tag_id
             JOIN tags t2 ON t2.id = ct2.tag_id
             GROUP BY t1.name, t2.name
             ORDER BY weight DESC
             LIMIT 100",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map([], |row| {
            Ok(TagRelation {
                source: row.get(0)?,
                target: row.get(1)?,
                weight: row.get(2)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}
