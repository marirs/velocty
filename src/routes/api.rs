use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::comment::{Comment, CommentForm};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use crate::rate_limit::RateLimiter;
use crate::security::auth::ClientIp;
use crate::security::{self, auth};

// ── Like / Unlike toggle ───────────────────────────────

#[derive(Debug, Serialize)]
pub struct LikeResponse {
    pub liked: bool,
    pub count: i64,
}

#[post("/like/<id>")]
pub fn like_toggle(pool: &State<DbPool>, id: i64, client_ip: ClientIp) -> Json<LikeResponse> {
    let ip_hash = auth::hash_ip(&client_ip.0);

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
        Json(LikeResponse { liked: true, count })
    }
}

// ── Check like status ──────────────────────────────────

#[get("/like/<id>/status")]
pub fn like_status(pool: &State<DbPool>, id: i64, client_ip: ClientIp) -> Json<LikeResponse> {
    let ip_hash = auth::hash_ip(&client_ip.0);
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
    pub captcha_token: Option<String>,
    pub ip: Option<String>,
    pub parent_id: Option<i64>,
}

#[post("/comment", format = "json", data = "<form>")]
pub fn comment_submit(
    pool: &State<DbPool>,
    limiter: &State<RateLimiter>,
    form: Json<CommentSubmit>,
) -> Json<Value> {
    // Check if comments are globally enabled
    if !Setting::get_bool(pool, "comments_enabled") {
        return Json(json!({"success": false, "error": "Comments are disabled"}));
    }

    // Check per-content-type setting
    let ct = form.content_type.as_deref().unwrap_or("post");
    match ct {
        "post" => {
            if !Setting::get_bool(pool, "comments_on_blog") {
                return Json(
                    json!({"success": false, "error": "Comments are disabled for blog posts"}),
                );
            }
        }
        "portfolio" => {
            if !Setting::get_bool(pool, "comments_on_portfolio") {
                return Json(
                    json!({"success": false, "error": "Comments are disabled for portfolio items"}),
                );
            }
        }
        _ => {}
    }

    // Validate required fields
    if Setting::get_bool(pool, "comments_require_name") && form.author_name.trim().is_empty() {
        return Json(json!({"success": false, "error": "Name is required"}));
    }
    // Rate limit by author name (email no longer collected for privacy)
    let rate_id = &form.author_name;
    let ip_hash = auth::hash_ip(rate_id);
    let rate_key = format!("comment:{}", ip_hash);
    let max_attempts = Setting::get_i64(pool, "comments_rate_limit").max(1) as u64;
    let window = std::time::Duration::from_secs(15 * 60);

    if !limiter.check_and_record(&rate_key, max_attempts, window) {
        return Json(json!({
            "success": false,
            "error": "Too many comments. Please wait before posting again."
        }));
    }

    // Captcha verification
    if let Some(ref token) = form.captcha_token {
        match security::verify_captcha(pool, token, form.ip.as_deref()) {
            Ok(false) => {
                return Json(json!({"success": false, "error": "Captcha verification failed"}))
            }
            Err(e) => log::warn!("Captcha error (allowing): {}", e),
            _ => {}
        }
    } else if security::has_captcha_provider(pool) {
        return Json(json!({"success": false, "error": "Captcha token required"}));
    }

    // Spam detection
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let user_ip = form.ip.as_deref().unwrap_or("unknown");
    match security::check_spam(
        pool,
        &site_url,
        user_ip,
        "",
        &form.body,
        Some(&form.author_name),
        form.author_email.as_deref(),
    ) {
        Ok(true) => {
            return Json(json!({"success": false, "error": "Your comment was flagged as spam"}))
        }
        Err(e) => log::warn!("Spam check error (allowing): {}", e),
        _ => {}
    }

    let comment_form = CommentForm {
        post_id: form.post_id,
        content_type: form.content_type.clone(),
        author_name: form.author_name.clone(),
        author_email: form.author_email.clone(),
        body: form.body.clone(),
        honeypot: form.honeypot.clone(),
        parent_id: form.parent_id,
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
