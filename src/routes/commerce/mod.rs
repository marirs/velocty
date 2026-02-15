pub mod paypal;
pub mod stripe;
pub mod razorpay;
pub mod mollie;
pub mod square;
pub mod twocheckout;
pub mod payoneer;

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, License, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

// ── Shared Helpers ──────────────────────────────────────

/// Helper: get base URL for webhooks/redirects
pub fn site_url(settings: &HashMap<String, String>) -> String {
    settings.get("site_url").cloned().unwrap_or_else(|| "http://localhost:8000".to_string())
}

/// Helper: get currency
pub fn currency(settings: &HashMap<String, String>) -> String {
    settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string())
}

/// Generate a secure random token for downloads
pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let rand_part: u64 = (ts as u64) ^ (ts.wrapping_mul(6364136223846793005) as u64);
    format!("{:016x}{:016x}", ts as u64, rand_part)
}

/// Generate a license key in format: XXXX-XXXX-XXXX-XXXX
pub fn generate_license_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut seed = ts as u64;
    let mut parts = Vec::new();
    for _ in 0..4 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let chunk = format!("{:04X}", (seed >> 32) & 0xFFFF);
        parts.push(chunk);
    }
    parts.join("-")
}

/// After a payment provider confirms, this creates the order + download token + license + sends email.
/// Returns JSON with download_token, license_key, etc.
pub fn finalize_order(
    pool: &DbPool,
    order_id: i64,
    provider_order_id: &str,
    buyer_email: &str,
    buyer_name: &str,
) -> Result<Value, String> {
    let order = Order::find_by_id(pool, order_id).ok_or("Order not found")?;
    if order.status != "pending" {
        return Err("Order already completed".to_string());
    }

    let _ = Order::update_provider_order_id(pool, order.id, provider_order_id);
    let _ = Order::update_status(pool, order.id, "completed");

    // Update buyer info
    if let Ok(conn) = pool.get() {
        let _ = conn.execute(
            "UPDATE orders SET buyer_email = ?1, buyer_name = ?2 WHERE id = ?3",
            rusqlite::params![buyer_email, buyer_name, order.id],
        );
    }

    let settings: HashMap<String, String> = Setting::all(pool);
    let max_downloads: i64 = settings.get("downloads_max_per_purchase").and_then(|v| v.parse().ok()).unwrap_or(3);
    let expiry_hours: i64 = settings.get("downloads_expiry_hours").and_then(|v| v.parse().ok()).unwrap_or(48);

    let token = generate_token();
    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::hours(expiry_hours);
    DownloadToken::create(pool, order.id, &token, max_downloads, expires_at)?;

    let license_key = generate_license_key();
    License::create(pool, order.id, &license_key)?;

    // Send email in background
    let item = PortfolioItem::find_by_id(pool, order.portfolio_id);
    let base = site_url(&settings);
    let download_url = format!("{}/download/{}", base, token);
    let cur = currency(&settings);
    std::thread::spawn({
        let pool = pool.clone();
        let email = buyer_email.to_string();
        let title = item.as_ref().map(|i| i.title.clone()).unwrap_or_default();
        let note = item.as_ref().map(|i| i.purchase_note.clone()).unwrap_or_default();
        let lk = license_key.clone();
        let amt = order.amount;
        let cur = cur.clone();
        let dl = download_url.clone();
        move || {
            crate::email::send_purchase_email(&pool, &email, &title, &note, &dl, Some(&lk), amt, &cur);
        }
    });

    Ok(json!({
        "ok": true,
        "download_token": token,
        "license_key": license_key,
        "max_downloads": max_downloads,
        "expiry_hours": expiry_hours,
    }))
}

/// Create a pending order for a given provider + item, returns (order_id, price, currency).
pub fn create_pending_order(
    pool: &DbPool,
    portfolio_id: i64,
    provider: &str,
    buyer_email: &str,
) -> Result<(i64, f64, String), String> {
    let item = PortfolioItem::find_by_id(pool, portfolio_id)
        .filter(|i| i.sell_enabled)
        .ok_or("Item not available for purchase")?;
    let price = item.price.filter(|&p| p > 0.0).ok_or("Item has no price set")?;
    let settings: HashMap<String, String> = Setting::all(pool);
    let cur = currency(&settings);
    let order_id = Order::create(pool, item.id, buyer_email, "", price, &cur, provider, "", "pending")?;
    Ok((order_id, price, cur))
}

/// Simple URL encoding for query parameters
pub fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

// ── Download page: Validate token and show download UI ─

#[get("/download/<token>")]
pub fn download_page(
    pool: &State<DbPool>,
    token: &str,
) -> rocket_dyn_templates::Template {
    let settings: HashMap<String, String> = Setting::all(pool);

    let dl_token = match DownloadToken::find_by_token(pool, token) {
        Some(t) => t,
        None => {
            return rocket_dyn_templates::Template::render("download", json!({
                "error": "Invalid download link",
                "settings": &settings,
            }));
        }
    };

    if !dl_token.is_valid() {
        let reason = if dl_token.downloads_used >= dl_token.max_downloads {
            "Download limit reached"
        } else {
            "Download link has expired"
        };
        return rocket_dyn_templates::Template::render("download", json!({
            "error": reason,
            "expired": true,
            "settings": &settings,
        }));
    }

    let order = match Order::find_by_id(pool, dl_token.order_id) {
        Some(o) if o.status == "completed" => o,
        _ => {
            return rocket_dyn_templates::Template::render("download", json!({
                "error": "Order not found or not completed",
                "settings": &settings,
            }));
        }
    };

    let item = match PortfolioItem::find_by_id(pool, order.portfolio_id) {
        Some(i) => i,
        None => {
            return rocket_dyn_templates::Template::render("download", json!({
                "error": "Item not found",
                "settings": &settings,
            }));
        }
    };

    let license = License::find_by_order(pool, order.id);

    rocket_dyn_templates::Template::render("download", json!({
        "settings": &settings,
        "item_title": item.title,
        "item_slug": item.slug,
        "image_path": item.image_path,
        "purchase_note": item.purchase_note,
        "license_key": license.map(|l| l.license_key),
        "downloads_used": dl_token.downloads_used,
        "max_downloads": dl_token.max_downloads,
        "downloads_remaining": dl_token.max_downloads - dl_token.downloads_used,
        "token": token,
        "buyer_email": order.buyer_email,
    }))
}

// ── Actual file download (increments count) ────────────

#[get("/download/<token>/file")]
pub fn download_file(
    pool: &State<DbPool>,
    token: &str,
) -> Result<rocket::response::Redirect, Json<Value>> {
    let dl_token = match DownloadToken::find_by_token(pool, token) {
        Some(t) => t,
        None => return Err(Json(json!({ "ok": false, "error": "Invalid download link" }))),
    };

    if !dl_token.is_valid() {
        return Err(Json(json!({ "ok": false, "error": "Download expired or limit reached" })));
    }

    let order = match Order::find_by_id(pool, dl_token.order_id) {
        Some(o) if o.status == "completed" => o,
        _ => return Err(Json(json!({ "ok": false, "error": "Order not completed" }))),
    };

    let item = match PortfolioItem::find_by_id(pool, order.portfolio_id) {
        Some(i) => i,
        None => return Err(Json(json!({ "ok": false, "error": "Item not found" }))),
    };

    // Increment download count
    let _ = DownloadToken::increment_download(pool, dl_token.id);

    // Redirect to the actual file
    Ok(rocket::response::Redirect::to(format!("/uploads/{}", item.image_path)))
}

// ── Check purchase status (for public page) ────────────

#[derive(Deserialize)]
pub struct CheckPurchaseRequest {
    pub portfolio_id: i64,
    pub email: String,
}

#[post("/api/checkout/check", format = "json", data = "<body>")]
pub fn check_purchase(
    pool: &State<DbPool>,
    body: Json<CheckPurchaseRequest>,
) -> Json<Value> {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return Json(json!({ "ok": false, "purchased": false })),
    };

    // Find a completed order for this email + portfolio item
    let result: Option<i64> = conn
        .query_row(
            "SELECT id FROM orders WHERE portfolio_id = ?1 AND buyer_email = ?2 AND status = 'completed' ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![body.portfolio_id, body.email],
            |row| row.get(0),
        )
        .ok();

    match result {
        Some(order_id) => {
            let dl_token = DownloadToken::find_by_order(pool, order_id);
            let license = License::find_by_order(pool, order_id);

            let token_valid = dl_token.as_ref().map(|t| t.is_valid()).unwrap_or(false);

            Json(json!({
                "ok": true,
                "purchased": true,
                "download_token": dl_token.map(|t| t.token),
                "token_valid": token_valid,
                "license_key": license.map(|l| l.license_key),
            }))
        }
        None => Json(json!({ "ok": true, "purchased": false })),
    }
}

// ── Generic: Capture (fallback for any provider) ────────

#[derive(Deserialize)]
pub struct GenericCaptureRequest {
    pub order_id: i64,
    pub provider_order_id: String,
    pub buyer_email: String,
    pub buyer_name: Option<String>,
}

#[post("/api/checkout/capture", format = "json", data = "<body>")]
pub fn generic_capture_order(
    pool: &State<DbPool>,
    body: Json<GenericCaptureRequest>,
) -> Json<Value> {
    match finalize_order(pool, body.order_id, &body.provider_order_id, &body.buyer_email, body.buyer_name.as_deref().unwrap_or("")) {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

// ── Route Registration ──────────────────────────────────

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        paypal::paypal_create_order,
        paypal::paypal_capture_order,
        stripe::stripe_create_session,
        stripe::stripe_success,
        razorpay::razorpay_create_order,
        razorpay::razorpay_verify,
        mollie::mollie_create_payment,
        mollie::mollie_webhook,
        mollie::mollie_return,
        square::square_create_payment,
        square::square_return,
        twocheckout::twocheckout_create,
        twocheckout::twocheckout_return,
        payoneer::payoneer_create,
        payoneer::payoneer_return,
        payoneer::payoneer_webhook,
        generic_capture_order,
        download_page,
        download_file,
        check_purchase,
    ]
}
