use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, License, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

/// Helper: get base URL for webhooks/redirects
fn site_url(settings: &HashMap<String, String>) -> String {
    settings.get("site_url").cloned().unwrap_or_else(|| "http://localhost:8000".to_string())
}

/// Helper: get currency
fn currency(settings: &HashMap<String, String>) -> String {
    settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string())
}

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

/// After a payment provider confirms, this creates the order + download token + license + sends email.
/// Returns JSON with download_token, license_key, etc.
fn finalize_order(
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
fn create_pending_order(
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
    match finalize_order(pool, body.order_id, &body.paypal_order_id, &body.buyer_email, body.buyer_name.as_deref().unwrap_or("")) {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
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
    let secret_key = settings.get("stripe_secret_key").cloned().unwrap_or_default();
    if secret_key.is_empty() {
        return Json(json!({ "ok": false, "error": "Stripe secret key not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "stripe", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    // Call Stripe API to create a Checkout Session
    let client = reqwest::blocking::Client::new();
    let resp = client.post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&secret_key, None::<&str>)
        .form(&[
            ("mode", "payment"),
            ("success_url", &format!("{}/api/stripe/success?session_id={{CHECKOUT_SESSION_ID}}&order_id={}", base, order_id)),
            ("cancel_url", &format!("{}/portfolio/{}", base, item.slug)),
            ("line_items[0][price_data][currency]", &cur.to_lowercase()),
            ("line_items[0][price_data][unit_amount]", &format!("{}", (price * 100.0) as i64)),
            ("line_items[0][price_data][product_data][name]", &item.title),
            ("line_items[0][quantity]", "1"),
            ("metadata[order_id]", &order_id.to_string()),
        ])
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
                let session_id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let _ = Order::update_provider_order_id(pool, order_id, session_id);
                Json(json!({ "ok": true, "order_id": order_id, "checkout_url": url, "session_id": session_id }))
            } else {
                let err = body.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("Stripe API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Stripe request failed: {}", e) })),
    }
}

// ── Stripe: Success redirect (captures after checkout) ──

#[get("/api/stripe/success?<session_id>&<order_id>")]
pub fn stripe_success(
    pool: &State<DbPool>,
    session_id: &str,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let secret_key = settings.get("stripe_secret_key").cloned().unwrap_or_default();
    let base = site_url(&settings);

    // Verify session is paid
    let client = reqwest::blocking::Client::new();
    let verified = client.get(&format!("https://api.stripe.com/v1/checkout/sessions/{}", session_id))
        .basic_auth(&secret_key, None::<&str>)
        .send()
        .ok()
        .and_then(|r| r.json::<Value>().ok())
        .and_then(|v| v.get("payment_status").and_then(|s| s.as_str()).map(|s| s == "paid"))
        .unwrap_or(false);

    if verified {
        // Get buyer email from Stripe session
        let session_data = client.get(&format!("https://api.stripe.com/v1/checkout/sessions/{}", session_id))
            .basic_auth(&secret_key, None::<&str>)
            .send().ok().and_then(|r| r.json::<Value>().ok());
        let buyer_email = session_data.as_ref()
            .and_then(|v| v.get("customer_details"))
            .and_then(|c| c.get("email"))
            .and_then(|e| e.as_str())
            .unwrap_or("");

        if let Ok(result) = finalize_order(pool, order_id, session_id, buyer_email, "") {
            if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                return rocket::response::Redirect::to(format!("/download/{}", token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}

// ── Razorpay: Create Order ──────────────────────────────

#[derive(Deserialize)]
pub struct RazorpayCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/razorpay/create", format = "json", data = "<body>")]
pub fn razorpay_create_order(
    pool: &State<DbPool>,
    body: Json<RazorpayCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_razorpay_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Razorpay is not enabled" }));
    }
    let key_id = settings.get("razorpay_key_id").cloned().unwrap_or_default();
    let key_secret = settings.get("razorpay_key_secret").cloned().unwrap_or_default();
    if key_id.is_empty() || key_secret.is_empty() {
        return Json(json!({ "ok": false, "error": "Razorpay credentials not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "razorpay", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Razorpay amounts are in smallest currency unit (paise for INR, cents for USD)
    let amount_minor = (price * 100.0) as i64;

    let client = reqwest::blocking::Client::new();
    let resp = client.post("https://api.razorpay.com/v1/orders")
        .basic_auth(&key_id, Some(&key_secret))
        .json(&json!({
            "amount": amount_minor,
            "currency": cur,
            "receipt": format!("order_{}", order_id),
            "notes": { "velocty_order_id": order_id.to_string() }
        }))
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            if let Some(rp_order_id) = body.get("id").and_then(|v| v.as_str()) {
                let _ = Order::update_provider_order_id(pool, order_id, rp_order_id);
                Json(json!({
                    "ok": true,
                    "order_id": order_id,
                    "razorpay_order_id": rp_order_id,
                    "razorpay_key_id": key_id,
                    "amount": amount_minor,
                    "currency": cur,
                }))
            } else {
                let err = body.get("error").and_then(|e| e.get("description")).and_then(|m| m.as_str()).unwrap_or("Razorpay API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Razorpay request failed: {}", e) })),
    }
}

// ── Razorpay: Verify Payment ────────────────────────────

#[derive(Deserialize)]
pub struct RazorpayVerifyRequest {
    pub order_id: i64,
    pub razorpay_order_id: String,
    pub razorpay_payment_id: String,
    pub razorpay_signature: String,
    pub buyer_email: String,
    pub buyer_name: Option<String>,
}

#[post("/api/checkout/razorpay/verify", format = "json", data = "<body>")]
pub fn razorpay_verify(
    pool: &State<DbPool>,
    body: Json<RazorpayVerifyRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    let key_secret = settings.get("razorpay_key_secret").cloned().unwrap_or_default();

    // Verify HMAC-SHA256 signature: sha256(razorpay_order_id + "|" + razorpay_payment_id)
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let msg = format!("{}|{}", body.razorpay_order_id, body.razorpay_payment_id);
    let mut mac = match HmacSha256::new_from_slice(key_secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return Json(json!({ "ok": false, "error": "HMAC init failed" })),
    };
    mac.update(msg.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    if expected != body.razorpay_signature {
        return Json(json!({ "ok": false, "error": "Invalid payment signature" }));
    }

    match finalize_order(pool, body.order_id, &body.razorpay_payment_id, &body.buyer_email, body.buyer_name.as_deref().unwrap_or("")) {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

// ── Mollie: Create Payment ──────────────────────────────

#[derive(Deserialize)]
pub struct MollieCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/mollie/create", format = "json", data = "<body>")]
pub fn mollie_create_payment(
    pool: &State<DbPool>,
    body: Json<MollieCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_mollie_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Mollie is not enabled" }));
    }
    let api_key = settings.get("mollie_api_key").cloned().unwrap_or_default();
    if api_key.is_empty() {
        return Json(json!({ "ok": false, "error": "Mollie API key not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "mollie", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    let client = reqwest::blocking::Client::new();
    let resp = client.post("https://api.mollie.com/v2/payments")
        .bearer_auth(&api_key)
        .json(&json!({
            "amount": { "currency": cur, "value": format!("{:.2}", price) },
            "description": item.title,
            "redirectUrl": format!("{}/api/mollie/return?order_id={}", base, order_id),
            "webhookUrl": format!("{}/api/mollie/webhook", base),
            "metadata": { "order_id": order_id.to_string() }
        }))
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            let checkout_url = body.get("_links").and_then(|l| l.get("checkout")).and_then(|c| c.get("href")).and_then(|h| h.as_str());
            let mollie_id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(url) = checkout_url {
                let _ = Order::update_provider_order_id(pool, order_id, mollie_id);
                Json(json!({ "ok": true, "order_id": order_id, "checkout_url": url }))
            } else {
                let err = body.get("detail").and_then(|m| m.as_str()).unwrap_or("Mollie API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Mollie request failed: {}", e) })),
    }
}

// ── Mollie: Webhook (payment status update) ─────────────

#[post("/api/mollie/webhook", format = "application/x-www-form-urlencoded", data = "<body>")]
pub fn mollie_webhook(
    pool: &State<DbPool>,
    body: String,
) -> &'static str {
    // Body is: id=tr_xxxxx
    let payment_id = body.strip_prefix("id=").unwrap_or(&body);
    let settings: HashMap<String, String> = Setting::all(pool);
    let api_key = settings.get("mollie_api_key").cloned().unwrap_or_default();

    // Fetch payment details from Mollie
    let client = reqwest::blocking::Client::new();
    let resp = client.get(&format!("https://api.mollie.com/v2/payments/{}", payment_id))
        .bearer_auth(&api_key)
        .send();

    if let Ok(r) = resp {
        if let Ok(data) = r.json::<Value>() {
            let status = data.get("status").and_then(|s| s.as_str()).unwrap_or("");
            let order_id_str = data.get("metadata").and_then(|m| m.get("order_id")).and_then(|o| o.as_str()).unwrap_or("");
            if status == "paid" {
                if let Ok(oid) = order_id_str.parse::<i64>() {
                    let email = data.get("details").and_then(|d| d.get("consumerName")).and_then(|n| n.as_str()).unwrap_or("");
                    let _ = finalize_order(pool, oid, payment_id, email, "");
                }
            }
        }
    }
    "OK"
}

// ── Mollie: Return redirect ─────────────────────────────

#[get("/api/mollie/return?<order_id>")]
pub fn mollie_return(
    pool: &State<DbPool>,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let base = site_url(&settings);
    // Check if order was completed by webhook
    if let Some(order) = Order::find_by_id(pool, order_id) {
        if order.status == "completed" {
            if let Some(dl) = DownloadToken::find_by_order(pool, order_id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    // Webhook hasn't fired yet — redirect to a waiting page or home
    rocket::response::Redirect::to(base)
}

// ── Square: Create Payment Link ─────────────────────────

#[derive(Deserialize)]
pub struct SquareCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/square/create", format = "json", data = "<body>")]
pub fn square_create_payment(
    pool: &State<DbPool>,
    body: Json<SquareCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_square_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Square is not enabled" }));
    }
    let access_token = settings.get("square_access_token").cloned().unwrap_or_default();
    let location_id = settings.get("square_location_id").cloned().unwrap_or_default();
    if access_token.is_empty() || location_id.is_empty() {
        return Json(json!({ "ok": false, "error": "Square credentials not configured" }));
    }
    let is_sandbox = settings.get("square_mode").map(|v| v.as_str()) != Some("live");
    let api_base = if is_sandbox { "https://connect.squareupsandbox.com" } else { "https://connect.squareup.com" };

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "square", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);
    let amount_minor = (price * 100.0) as i64;

    let client = reqwest::blocking::Client::new();
    let resp = client.post(&format!("{}/v2/online-checkout/payment-links", api_base))
        .bearer_auth(&access_token)
        .json(&json!({
            "idempotency_key": format!("velocty_{}", order_id),
            "quick_pay": {
                "name": item.title,
                "price_money": { "amount": amount_minor, "currency": cur },
                "location_id": location_id
            },
            "checkout_options": {
                "redirect_url": format!("{}/api/square/return?order_id={}", base, order_id)
            },
            "payment_note": format!("Order #{}", order_id)
        }))
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            let url = body.get("payment_link").and_then(|l| l.get("url")).and_then(|u| u.as_str());
            let sq_id = body.get("payment_link").and_then(|l| l.get("id")).and_then(|i| i.as_str()).unwrap_or("");
            if let Some(checkout_url) = url {
                let _ = Order::update_provider_order_id(pool, order_id, sq_id);
                Json(json!({ "ok": true, "order_id": order_id, "checkout_url": checkout_url }))
            } else {
                let errors = body.get("errors").and_then(|e| e.as_array()).and_then(|a| a.first()).and_then(|e| e.get("detail")).and_then(|d| d.as_str()).unwrap_or("Square API error");
                Json(json!({ "ok": false, "error": errors }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Square request failed: {}", e) })),
    }
}

// ── Square: Return redirect ─────────────────────────────

#[get("/api/square/return?<order_id>")]
pub fn square_return(
    pool: &State<DbPool>,
    order_id: i64,
) -> rocket::response::Redirect {
    // Square redirects here after payment; finalize the order
    let settings: HashMap<String, String> = Setting::all(pool);
    let base = site_url(&settings);
    if let Some(order) = Order::find_by_id(pool, order_id) {
        if order.status == "pending" {
            // Square doesn't send buyer email in redirect, use stored value
            if let Ok(result) = finalize_order(pool, order_id, &order.provider_order_id, &order.buyer_email, "") {
                if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                    return rocket::response::Redirect::to(format!("/download/{}", token));
                }
            }
        } else if order.status == "completed" {
            if let Some(dl) = DownloadToken::find_by_order(pool, order_id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}

// ── 2Checkout: Create Hosted Checkout ───────────────────

#[derive(Deserialize)]
pub struct TwoCheckoutCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/2checkout/create", format = "json", data = "<body>")]
pub fn twocheckout_create(
    pool: &State<DbPool>,
    body: Json<TwoCheckoutCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_2checkout_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "2Checkout is not enabled" }));
    }
    let merchant_code = settings.get("twocheckout_merchant_code").cloned().unwrap_or_default();
    if merchant_code.is_empty() {
        return Json(json!({ "ok": false, "error": "2Checkout merchant code not configured" }));
    }
    let is_sandbox = settings.get("twocheckout_mode").map(|v| v.as_str()) != Some("live");

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "2checkout", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    // 2Checkout uses a hosted checkout URL with query parameters
    let checkout_base = if is_sandbox { "https://sandbox.2checkout.com/checkout/purchase" } else { "https://www.2checkout.com/checkout/purchase" };
    let checkout_url = format!(
        "{}?seller_id={}&product_id=velocty_{}&price={:.2}&currency={}&return-url={}/api/2checkout/return?order_id={}&return-type=redirect&prod={}&qty=1",
        checkout_base, merchant_code, order_id, price, cur, base, order_id, urlencoding(item.title.as_str())
    );

    Json(json!({ "ok": true, "order_id": order_id, "checkout_url": checkout_url }))
}

// ── 2Checkout: Return redirect ──────────────────────────

#[get("/api/2checkout/return?<order_id>")]
pub fn twocheckout_return(
    pool: &State<DbPool>,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let base = site_url(&settings);
    if let Some(order) = Order::find_by_id(pool, order_id) {
        if order.status == "pending" {
            if let Ok(result) = finalize_order(pool, order_id, &format!("2co_{}", order_id), &order.buyer_email, "") {
                if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                    return rocket::response::Redirect::to(format!("/download/{}", token));
                }
            }
        } else if order.status == "completed" {
            if let Some(dl) = DownloadToken::find_by_order(pool, order_id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}

// ── Payoneer: Create Checkout ───────────────────────────

#[derive(Deserialize)]
pub struct PayoneerCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/payoneer/create", format = "json", data = "<body>")]
pub fn payoneer_create(
    pool: &State<DbPool>,
    body: Json<PayoneerCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_payoneer_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Payoneer is not enabled" }));
    }
    let client_id = settings.get("payoneer_client_id").cloned().unwrap_or_default();
    let client_secret = settings.get("payoneer_client_secret").cloned().unwrap_or_default();
    let program_id = settings.get("payoneer_program_id").cloned().unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() || program_id.is_empty() {
        return Json(json!({ "ok": false, "error": "Payoneer credentials not configured" }));
    }
    let is_sandbox = settings.get("payoneer_mode").map(|v| v.as_str()) != Some("live");
    let api_base = if is_sandbox { "https://api.sandbox.payoneer.com" } else { "https://api.payoneer.com" };

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "payoneer", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    // Get OAuth token
    let client = reqwest::blocking::Client::new();
    let token_resp = client.post(&format!("{}/v4/programs/{}/token", api_base, program_id))
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send();

    let access_token = match token_resp {
        Ok(r) => {
            let data: Value = r.json().unwrap_or_default();
            match data.get("access_token").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => return Json(json!({ "ok": false, "error": "Failed to get Payoneer access token" })),
            }
        }
        Err(e) => return Json(json!({ "ok": false, "error": format!("Payoneer auth failed: {}", e) })),
    };

    // Create a checkout link
    let resp = client.post(&format!("{}/v4/programs/{}/checkout", api_base, program_id))
        .bearer_auth(&access_token)
        .json(&json!({
            "amount": price,
            "currency": cur,
            "description": item.title,
            "payout_id": format!("velocty_{}", order_id),
            "redirect_url": format!("{}/api/payoneer/return?order_id={}", base, order_id),
            "notification_url": format!("{}/api/payoneer/webhook", base)
        }))
        .send();

    match resp {
        Ok(r) => {
            let data: Value = r.json().unwrap_or_default();
            let checkout_url = data.get("redirect_url").or_else(|| data.get("checkout_url")).and_then(|u| u.as_str());
            if let Some(url) = checkout_url {
                let pyo_id = data.get("payout_id").and_then(|i| i.as_str()).unwrap_or("");
                let _ = Order::update_provider_order_id(pool, order_id, pyo_id);
                Json(json!({ "ok": true, "order_id": order_id, "checkout_url": url }))
            } else {
                let err = data.get("description").and_then(|d| d.as_str()).unwrap_or("Payoneer API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Payoneer request failed: {}", e) })),
    }
}

// ── Payoneer: Return redirect ───────────────────────────

#[get("/api/payoneer/return?<order_id>")]
pub fn payoneer_return(
    pool: &State<DbPool>,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let base = site_url(&settings);
    if let Some(order) = Order::find_by_id(pool, order_id) {
        if order.status == "pending" {
            if let Ok(result) = finalize_order(pool, order_id, &order.provider_order_id, &order.buyer_email, "") {
                if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                    return rocket::response::Redirect::to(format!("/download/{}", token));
                }
            }
        } else if order.status == "completed" {
            if let Some(dl) = DownloadToken::find_by_order(pool, order_id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}

// ── Payoneer: Webhook ───────────────────────────────────

#[post("/api/payoneer/webhook", format = "json", data = "<body>")]
pub fn payoneer_webhook(
    pool: &State<DbPool>,
    body: Json<Value>,
) -> &'static str {
    let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
    let payout_id = body.get("payout_id").and_then(|p| p.as_str()).unwrap_or("");
    if status == "done" || status == "completed" {
        // payout_id format: velocty_{order_id}
        if let Some(oid_str) = payout_id.strip_prefix("velocty_") {
            if let Ok(oid) = oid_str.parse::<i64>() {
                let email = body.get("payee_email").and_then(|e| e.as_str()).unwrap_or("");
                let _ = finalize_order(pool, oid, payout_id, email, "");
            }
        }
    }
    "OK"
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

/// Simple URL encoding for query parameters
fn urlencoding(s: &str) -> String {
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

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        paypal_create_order,
        paypal_capture_order,
        stripe_create_session,
        stripe_success,
        razorpay_create_order,
        razorpay_verify,
        mollie_create_payment,
        mollie_webhook,
        mollie_return,
        square_create_payment,
        square_return,
        twocheckout_create,
        twocheckout_return,
        payoneer_create,
        payoneer_return,
        payoneer_webhook,
        generic_capture_order,
        download_page,
        download_file,
        check_purchase,
    ]
}
