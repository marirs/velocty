use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, License, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

/// Generate a secure random token for downloads
fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let rand_part: u64 = (ts as u64) ^ (ts.wrapping_mul(6364136223846793005) as u64);
    format!("{:016x}{:016x}", ts as u64, rand_part)
}

/// Generate a license key in format: XXXX-XXXX-XXXX-XXXX
fn generate_license_key() -> String {
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

// ── Create Order (called after provider confirms payment) ──

/// After a payment provider confirms, this creates the order + download token + license.
/// Returns the download token for the buyer.
pub fn complete_purchase(
    pool: &DbPool,
    portfolio_id: i64,
    buyer_email: &str,
    buyer_name: &str,
    amount: f64,
    currency: &str,
    provider: &str,
    provider_order_id: &str,
) -> Result<(Order, DownloadToken, License), String> {
    // Create the order
    let order_id = Order::create(
        pool,
        portfolio_id,
        buyer_email,
        buyer_name,
        amount,
        currency,
        provider,
        provider_order_id,
        "completed",
    )?;

    // Get download settings
    let settings: HashMap<String, String> = Setting::all(pool);
    let max_downloads: i64 = settings
        .get("downloads_max_per_purchase")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let expiry_hours: i64 = settings
        .get("downloads_expiry_hours")
        .and_then(|v| v.parse().ok())
        .unwrap_or(48);

    // Create download token
    let token = generate_token();
    let expires_at = chrono::Utc::now().naive_utc()
        + chrono::Duration::hours(expiry_hours);
    let _token_id = DownloadToken::create(pool, order_id, &token, max_downloads, expires_at)?;

    // Create license
    let license_key = generate_license_key();
    let _license_id = License::create(pool, order_id, &license_key)?;

    // Fetch the created records
    let order = Order::find_by_id(pool, order_id).ok_or("Order not found after creation")?;
    let dl_token =
        DownloadToken::find_by_order(pool, order_id).ok_or("Token not found after creation")?;
    let license =
        License::find_by_order(pool, order_id).ok_or("License not found after creation")?;

    Ok((order, dl_token, license))
}

// ── PayPal: Create Order ───────────────────────────────

#[derive(Deserialize)]
pub struct PaypalCreateRequest {
    pub portfolio_id: i64,
}

#[post("/api/checkout/paypal/create", format = "json", data = "<body>")]
pub fn paypal_create_order(
    pool: &State<DbPool>,
    body: Json<PaypalCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);

    if settings.get("commerce_paypal_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "PayPal is not enabled" }));
    }

    let item = match PortfolioItem::find_by_id(pool, body.portfolio_id) {
        Some(i) if i.sell_enabled => i,
        _ => return Json(json!({ "ok": false, "error": "Item not available for purchase" })),
    };

    let price = match item.price {
        Some(p) if p > 0.0 => p,
        _ => return Json(json!({ "ok": false, "error": "Item has no price set" })),
    };

    let currency = settings
        .get("commerce_currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());

    // Create a pending order in our DB
    let order_id = match Order::create(
        pool,
        item.id,
        "",
        "",
        price,
        &currency,
        "paypal",
        "",
        "pending",
    ) {
        Ok(id) => id,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Return the order info for the PayPal JS SDK to create the PayPal order client-side
    Json(json!({
        "ok": true,
        "order_id": order_id,
        "amount": format!("{:.2}", price),
        "currency": currency,
        "item_title": item.title,
    }))
}

// ── PayPal: Capture Order (after buyer approves) ───────

#[derive(Deserialize)]
pub struct PaypalCaptureRequest {
    pub order_id: i64,
    pub paypal_order_id: String,
    pub buyer_email: String,
    pub buyer_name: Option<String>,
}

#[post("/api/checkout/paypal/capture", format = "json", data = "<body>")]
pub fn paypal_capture_order(
    pool: &State<DbPool>,
    body: Json<PaypalCaptureRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);

    if settings.get("commerce_paypal_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "PayPal is not enabled" }));
    }

    // Find the pending order
    let order = match Order::find_by_id(pool, body.order_id) {
        Some(o) if o.status == "pending" && o.provider == "paypal" => o,
        _ => return Json(json!({ "ok": false, "error": "Order not found or already completed" })),
    };

    // Update order with PayPal details
    let _ = Order::update_provider_order_id(pool, order.id, &body.paypal_order_id);
    let _ = Order::update_status(pool, order.id, "completed");

    // Update buyer info
    {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
        };
        let _ = conn.execute(
            "UPDATE orders SET buyer_email = ?1, buyer_name = ?2 WHERE id = ?3",
            rusqlite::params![
                body.buyer_email,
                body.buyer_name.as_deref().unwrap_or(""),
                order.id
            ],
        );
    }

    // Create download token + license
    let max_downloads: i64 = settings
        .get("downloads_max_per_purchase")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let expiry_hours: i64 = settings
        .get("downloads_expiry_hours")
        .and_then(|v| v.parse().ok())
        .unwrap_or(48);

    let token = generate_token();
    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::hours(expiry_hours);
    if let Err(e) = DownloadToken::create(pool, order.id, &token, max_downloads, expires_at) {
        return Json(json!({ "ok": false, "error": e }));
    }

    let license_key = generate_license_key();
    if let Err(e) = License::create(pool, order.id, &license_key) {
        return Json(json!({ "ok": false, "error": e }));
    }

    // Send purchase confirmation email
    let item = PortfolioItem::find_by_id(pool, order.portfolio_id);
    let site_url = settings.get("site_url").cloned().unwrap_or_else(|| "http://localhost:8000".to_string());
    let download_url = format!("{}/download/{}", site_url, token);
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());
    std::thread::spawn({
        let pool = pool.inner().clone();
        let buyer_email = body.buyer_email.clone();
        let item_title = item.as_ref().map(|i| i.title.clone()).unwrap_or_default();
        let purchase_note = item.as_ref().map(|i| i.purchase_note.clone()).unwrap_or_default();
        let license_key_clone = license_key.clone();
        let amount = order.amount;
        let currency = currency.clone();
        let download_url = download_url.clone();
        move || {
            crate::email::send_purchase_email(
                &pool, &buyer_email, &item_title, &purchase_note,
                &download_url, Some(&license_key_clone), amount, &currency,
            );
        }
    });

    Json(json!({
        "ok": true,
        "download_token": token,
        "license_key": license_key,
        "max_downloads": max_downloads,
        "expiry_hours": expiry_hours,
    }))
}

// ── Stripe: Create Checkout Session ────────────────────

#[derive(Deserialize)]
pub struct StripeCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/stripe/create", format = "json", data = "<body>")]
pub fn stripe_create_session(
    pool: &State<DbPool>,
    body: Json<StripeCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);

    if settings.get("commerce_stripe_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Stripe is not enabled" }));
    }

    let item = match PortfolioItem::find_by_id(pool, body.portfolio_id) {
        Some(i) if i.sell_enabled => i,
        _ => return Json(json!({ "ok": false, "error": "Item not available for purchase" })),
    };

    let price = match item.price {
        Some(p) if p > 0.0 => p,
        _ => return Json(json!({ "ok": false, "error": "Item has no price set" })),
    };

    let currency = settings
        .get("commerce_currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());

    // Create a pending order
    let order_id = match Order::create(
        pool,
        item.id,
        body.buyer_email.as_deref().unwrap_or(""),
        "",
        price,
        &currency,
        "stripe",
        "",
        "pending",
    ) {
        Ok(id) => id,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Return info for the frontend to create a Stripe Checkout session
    // The actual Stripe session creation happens server-side via the Stripe API
    let stripe_key = settings
        .get("stripe_publishable_key")
        .cloned()
        .unwrap_or_default();

    Json(json!({
        "ok": true,
        "order_id": order_id,
        "amount": format!("{:.2}", price),
        "amount_cents": (price * 100.0) as i64,
        "currency": currency.to_lowercase(),
        "item_title": item.title,
        "stripe_key": stripe_key,
    }))
}

// ── Generic: Capture (for Stripe webhook, Razorpay, etc.) ──

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
    let order = match Order::find_by_id(pool, body.order_id) {
        Some(o) if o.status == "pending" => o,
        _ => return Json(json!({ "ok": false, "error": "Order not found or already completed" })),
    };

    let _ = Order::update_provider_order_id(pool, order.id, &body.provider_order_id);
    let _ = Order::update_status(pool, order.id, "completed");

    {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
        };
        let _ = conn.execute(
            "UPDATE orders SET buyer_email = ?1, buyer_name = ?2 WHERE id = ?3",
            rusqlite::params![
                body.buyer_email,
                body.buyer_name.as_deref().unwrap_or(""),
                order.id
            ],
        );
    }

    let settings: HashMap<String, String> = Setting::all(pool);
    let max_downloads: i64 = settings
        .get("downloads_max_per_purchase")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let expiry_hours: i64 = settings
        .get("downloads_expiry_hours")
        .and_then(|v| v.parse().ok())
        .unwrap_or(48);

    let token = generate_token();
    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::hours(expiry_hours);
    if let Err(e) = DownloadToken::create(pool, order.id, &token, max_downloads, expires_at) {
        return Json(json!({ "ok": false, "error": e }));
    }

    let license_key = generate_license_key();
    if let Err(e) = License::create(pool, order.id, &license_key) {
        return Json(json!({ "ok": false, "error": e }));
    }

    // Send purchase confirmation email
    let item = PortfolioItem::find_by_id(pool, order.portfolio_id);
    let site_url = settings.get("site_url").cloned().unwrap_or_else(|| "http://localhost:8000".to_string());
    let download_url = format!("{}/download/{}", site_url, token);
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());
    std::thread::spawn({
        let pool = pool.inner().clone();
        let buyer_email = body.buyer_email.clone();
        let item_title = item.as_ref().map(|i| i.title.clone()).unwrap_or_default();
        let purchase_note = item.as_ref().map(|i| i.purchase_note.clone()).unwrap_or_default();
        let license_key_clone = license_key.clone();
        let amount = order.amount;
        let currency = currency.clone();
        let download_url = download_url.clone();
        move || {
            crate::email::send_purchase_email(
                &pool, &buyer_email, &item_title, &purchase_note,
                &download_url, Some(&license_key_clone), amount, &currency,
            );
        }
    });

    Json(json!({
        "ok": true,
        "download_token": token,
        "license_key": license_key,
        "max_downloads": max_downloads,
        "expiry_hours": expiry_hours,
    }))
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

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        paypal_create_order,
        paypal_capture_order,
        stripe_create_session,
        generic_capture_order,
        download_page,
        download_file,
        check_purchase,
    ]
}
