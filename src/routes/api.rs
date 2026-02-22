use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::models::comment::CommentForm;
use crate::rate_limit::RateLimiter;
use crate::security::auth::ClientIp;
use crate::security::{self, auth};
use crate::store::Store;

// ── Like / Unlike toggle ───────────────────────────────

#[derive(Debug, Serialize)]
pub struct LikeResponse {
    pub liked: bool,
    pub count: i64,
}

#[post("/like/<id>")]
pub fn like_toggle(
    store: &State<Arc<dyn Store>>,
    id: i64,
    client_ip: ClientIp,
) -> Json<LikeResponse> {
    let ip_hash = auth::hash_ip(&client_ip.0);

    if store.like_exists(id, &ip_hash) {
        // Unlike
        let _ = store.like_remove(id, &ip_hash);
        let count = store.portfolio_decrement_likes(id).unwrap_or(0);
        Json(LikeResponse {
            liked: false,
            count,
        })
    } else {
        // Like
        let _ = store.like_add(id, &ip_hash);
        let count = store.portfolio_increment_likes(id).unwrap_or(0);
        Json(LikeResponse { liked: true, count })
    }
}

// ── Check like status ──────────────────────────────────

#[get("/like/<id>/status")]
pub fn like_status(
    store: &State<Arc<dyn Store>>,
    id: i64,
    client_ip: ClientIp,
) -> Json<LikeResponse> {
    let ip_hash = auth::hash_ip(&client_ip.0);
    let liked = store.like_exists(id, &ip_hash);
    let count = store.portfolio_find_by_id(id).map(|p| p.likes).unwrap_or(0);

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
    store: &State<Arc<dyn Store>>,
    limiter: &State<RateLimiter>,
    form: Json<CommentSubmit>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    // Check per-content-type setting
    let ct = form.content_type.as_deref().unwrap_or("post");
    match ct {
        "post" => {
            if s.setting_get_or("comments_on_blog", "false") != "true" {
                return Json(
                    json!({"success": false, "error": "Comments are disabled for blog posts"}),
                );
            }
        }
        "portfolio" => {
            if s.setting_get_or("comments_on_portfolio", "false") != "true" {
                return Json(
                    json!({"success": false, "error": "Comments are disabled for portfolio items"}),
                );
            }
        }
        _ => {}
    }

    // Validate required fields
    if s.setting_get_or("comments_require_name", "false") == "true"
        && form.author_name.trim().is_empty()
    {
        return Json(json!({"success": false, "error": "Name is required"}));
    }
    // Rate limit by author name (email no longer collected for privacy)
    let rate_id = &form.author_name;
    let ip_hash = auth::hash_ip(rate_id);
    let rate_key = format!("comment:{}", ip_hash);
    let max_attempts = s.setting_get_i64("comments_rate_limit").max(1) as u64;
    let window = std::time::Duration::from_secs(15 * 60);

    if !limiter.check_and_record(&rate_key, max_attempts, window) {
        return Json(json!({
            "success": false,
            "error": "Too many comments. Please wait before posting again."
        }));
    }

    // Captcha verification
    if let Some(ref token) = form.captcha_token {
        match security::verify_captcha(s, token, form.ip.as_deref()) {
            Ok(false) => {
                return Json(json!({"success": false, "error": "Captcha verification failed"}))
            }
            Err(e) => log::warn!("Captcha error (allowing): {}", e),
            _ => {}
        }
    } else if security::has_captcha_provider(s) {
        return Json(json!({"success": false, "error": "Captcha token required"}));
    }

    // Spam detection
    let site_url = s.setting_get_or("site_url", "http://localhost:8000");
    let user_ip = form.ip.as_deref().unwrap_or("unknown");
    match security::check_spam(
        s,
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

    match s.comment_create(&comment_form) {
        Ok(id) => {
            let moderation = s.setting_get_or("comments_moderation", "manual");
            if moderation == "auto-approve" {
                let _ = s.comment_update_status(id, "approved");
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
    store: &State<Arc<dyn Store>>,
    category_slug: &str,
    page: Option<i64>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let per_page = s.setting_get_i64("portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let items = s.portfolio_by_category(category_slug, per_page, offset);

    Json(json!({
        "items": items,
        "page": current_page,
    }))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![like_toggle, like_status, comment_submit, portfolio_filter]
}
