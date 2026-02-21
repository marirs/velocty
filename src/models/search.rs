use rusqlite::params;
use serde::Serialize;

use crate::db::DbPool;

#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub item_type: String, // "post" or "portfolio"
    pub item_id: i64,
    pub title: String,
    pub slug: String,
    pub snippet: String,
    pub image: Option<String>,
    pub date: Option<String>,
    pub rank: f64,
}

/// Strip HTML tags from a string (simple regex-free approach).
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => out.push(ch),
            _ => {}
        }
    }
    // Collapse whitespace
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate text to approximately `max_words` words.
fn truncate_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        format!("{}…", words[..max_words].join(" "))
    }
}

/// Create the FTS5 virtual table if it doesn't exist.
pub fn create_fts_table(pool: &DbPool) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
            item_type,
            item_id UNINDEXED,
            title,
            body,
            slug UNINDEXED,
            image UNINDEXED,
            date UNINDEXED,
            tokenize='porter unicode61'
        );",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Rebuild the entire search index from published posts and portfolio items.
pub fn rebuild_index(pool: &DbPool) -> Result<usize, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;

    // Clear existing index
    conn.execute("DELETE FROM search_index", [])
        .map_err(|e| e.to_string())?;

    let mut count = 0usize;

    // Index published posts
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content_html, slug, featured_image, published_at FROM posts WHERE status = 'published'",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            let (id, title, html, slug, image, date) = row.map_err(|e| e.to_string())?;
            let body = strip_html(&html);
            conn.execute(
                "INSERT INTO search_index (item_type, item_id, title, body, slug, image, date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params!["post", id, title, body, slug, image, date],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    // Index published portfolio items
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description_html, slug, image_path, published_at FROM portfolio WHERE status = 'published'",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            let (id, title, html, slug, image, date) = row.map_err(|e| e.to_string())?;
            let body = strip_html(&html.unwrap_or_default());
            conn.execute(
                "INSERT INTO search_index (item_type, item_id, title, body, slug, image, date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params!["portfolio", id, title, body, slug, image, date],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}

/// Update or insert a single item in the search index.
/// Call this after creating or updating a post/portfolio item.
pub fn upsert_item(
    pool: &DbPool,
    item_type: &str,
    item_id: i64,
    title: &str,
    html_body: &str,
    slug: &str,
    image: Option<&str>,
    date: Option<&str>,
    is_published: bool,
) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Always remove old entry first
    let _ = conn.execute(
        "DELETE FROM search_index WHERE item_type = ?1 AND item_id = ?2",
        params![item_type, item_id],
    );

    // Only insert if published
    if is_published {
        let body = strip_html(html_body);
        let _ = conn.execute(
            "INSERT INTO search_index (item_type, item_id, title, body, slug, image, date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![item_type, item_id, title, body, slug, image, date],
        );
    }
}

/// Remove an item from the search index.
/// Call this after deleting a post/portfolio item.
pub fn remove_item(pool: &DbPool, item_type: &str, item_id: i64) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = conn.execute(
        "DELETE FROM search_index WHERE item_type = ?1 AND item_id = ?2",
        params![item_type, item_id],
    );
}

/// Search the FTS index. Returns results ranked by relevance.
pub fn search(pool: &DbPool, query: &str, limit: i64) -> Vec<SearchResult> {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let query = query.trim();
    if query.is_empty() {
        return vec![];
    }

    // Escape FTS5 special characters and append * for prefix matching
    let fts_query = query
        .split_whitespace()
        .map(|w| {
            // Remove FTS5 operators from user input
            let clean: String = w
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{}\"*", clean)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if fts_query.is_empty() {
        return vec![];
    }

    let mut stmt = match conn.prepare(
        "SELECT item_type, item_id, title, body, slug, image, date, rank
         FROM search_index
         WHERE search_index MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(params![fts_query, limit], |row| {
        let body: String = row.get(3)?;
        Ok(SearchResult {
            item_type: row.get(0)?,
            item_id: row.get(1)?,
            title: row.get(2)?,
            slug: row.get(4)?,
            snippet: truncate_words(&body, 40),
            image: row.get(5)?,
            date: row.get(6)?,
            rank: row.get(7)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}
