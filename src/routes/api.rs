use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth;
use crate::db::DbPool;
use crate::models::comment::{Comment, CommentForm};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;

// ── Like / Unlike toggle ───────────────────────────────

#[derive(Debug, Serialize)]
pub struct LikeResponse {
    pub liked: bool,
    pub count: i64,
}

#[post("/like/<id>", format = "json", data = "<body>")]
pub fn like_toggle(
    pool: &State<DbPool>,
    id: i64,
    body: Json<Value>,
) -> Json<LikeResponse> {
    let ip = body.get("ip").and_then(|v| v.as_str()).unwrap_or("unknown");
    let ip_hash = auth::hash_ip(ip);

    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return Json(LikeResponse {
                liked: false,
                count: 0,
            })
        }
    };

    // Check if already liked
    let already_liked: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM likes WHERE portfolio_id = ?1 AND ip_hash = ?2",
            rusqlite::params![id, ip_hash],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if already_liked {
        // Unlike
        let _ = conn.execute(
            "DELETE FROM likes WHERE portfolio_id = ?1 AND ip_hash = ?2",
            rusqlite::params![id, ip_hash],
        );
        let count = PortfolioItem::decrement_likes(pool, id).unwrap_or(0);
        Json(LikeResponse {
            liked: false,
            count,
        })
    } else {
        // Like
        let _ = conn.execute(
            "INSERT OR IGNORE INTO likes (portfolio_id, ip_hash) VALUES (?1, ?2)",
            rusqlite::params![id, ip_hash],
        );
        let count = PortfolioItem::increment_likes(pool, id).unwrap_or(0);
        Json(LikeResponse {
            liked: true,
            count,
        })
    }
}

// ── Check like status ──────────────────────────────────

#[get("/like/<id>/status?<ip>")]
pub fn like_status(pool: &State<DbPool>, id: i64, ip: &str) -> Json<LikeResponse> {
    let ip_hash = auth::hash_ip(ip);
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return Json(LikeResponse {
                liked: false,
                count: 0,
            })
        }
    };

    let liked: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM likes WHERE portfolio_id = ?1 AND ip_hash = ?2",
            rusqlite::params![id, ip_hash],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    let count: i64 = conn
        .query_row(
            "SELECT likes FROM portfolio WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Json(LikeResponse { liked, count })
}

// ── Comment submission ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CommentSubmit {
    pub post_id: i64,
    pub content_type: Option<String>,
    pub author_name: String,
    pub author_email: Option<String>,
    pub body: String,
    pub honeypot: Option<String>,
}

#[post("/comment", format = "json", data = "<form>")]
pub fn comment_submit(pool: &State<DbPool>, form: Json<CommentSubmit>) -> Json<Value> {
    let comment_form = CommentForm {
        post_id: form.post_id,
        content_type: form.content_type.clone(),
        author_name: form.author_name.clone(),
        author_email: form.author_email.clone(),
        body: form.body.clone(),
        honeypot: form.honeypot.clone(),
    };

    match Comment::create(pool, &comment_form) {
        Ok(id) => {
            let moderation = Setting::get_or(pool, "comments_moderation", "manual");
            if moderation == "auto-approve" {
                let _ = Comment::update_status(pool, id, "approved");
            }
            Json(json!({
                "success": true,
                "id": id,
                "message": if moderation == "auto-approve" {
                    "Comment posted"
                } else {
                    "Comment submitted for moderation"
                }
            }))
        }
        Err(e) => Json(json!({
            "success": false,
            "error": e,
        })),
    }
}

// ── Portfolio category filter (hybrid AJAX) ────────────

#[get("/portfolio/filter/<category_slug>?<page>")]
pub fn portfolio_filter(
    pool: &State<DbPool>,
    category_slug: &str,
    page: Option<i64>,
) -> Json<Value> {
    let per_page = Setting::get_i64(pool, "portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let items = PortfolioItem::by_category(pool, category_slug, per_page, offset);

    Json(json!({
        "items": items,
        "page": current_page,
    }))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![like_toggle, like_status, comment_submit, portfolio_filter]
}
