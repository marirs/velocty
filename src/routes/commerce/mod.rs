pub mod mollie;
pub mod payoneer;
pub mod paypal;
pub mod razorpay;
pub mod square;
pub mod stripe;
pub mod twocheckout;

use std::collections::HashMap;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Store;

// ── Security Helpers ────────────────────────────────────

/// Constant-time comparison to prevent timing attacks on webhook signatures.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ── Shared Helpers ──────────────────────────────────────

/// Helper: get base URL for webhooks/redirects
pub fn site_url(settings: &HashMap<String, String>) -> String {
    settings
        .get("site_url")
        .cloned()
        .unwrap_or_else(|| "http://localhost:8000".to_string())
}

/// Helper: get currency
pub fn currency(settings: &HashMap<String, String>) -> String {
    settings
        .get("commerce_currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string())
}

/// Generate a secure random token for downloads
pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    hex::encode(bytes)
}

/// Generate a license key in format: XXXX-XXXX-XXXX-XXXX
pub fn generate_license_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let parts: Vec<String> = (0..4)
        .map(|_| format!("{:04X}", rng.gen::<u16>()))
        .collect();
    parts.join("-")
}

/// After a payment provider confirms, this creates the order + download token + license + sends email.
/// Returns JSON with download_token, license_key, etc.
pub fn finalize_order(
    store: &dyn Store,
    order_id: i64,
    provider_order_id: &str,
    buyer_email: &str,
    buyer_name: &str,
) -> Result<Value, String> {
    let order = store.order_find_by_id(order_id).ok_or("Order not found")?;
    if order.status != "pending" {
        return Err("Order already completed".to_string());
    }

    let _ = store.order_update_provider_order_id(order.id, provider_order_id);
    let _ = store.order_update_status(order.id, "completed");
    let _ = store.order_update_buyer_info(order.id, buyer_email, buyer_name);

    let settings: HashMap<String, String> = store.setting_all();
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
    store.download_token_create(order.id, &token, max_downloads, expires_at)?;

    let license_key = generate_license_key();
    store.license_create(order.id, &license_key)?;

    // Send email in background
    let item = store.portfolio_find_by_id(order.portfolio_id);
    let base = site_url(&settings);
    let download_url = format!("{}/download/{}", base, token);
    let cur = currency(&settings);
    // We need the settings for the email — pass them via the thread
    let email = buyer_email.to_string();
    let title = item.as_ref().map(|i| i.title.clone()).unwrap_or_default();
    let note = item
        .as_ref()
        .map(|i| i.purchase_note.clone())
        .unwrap_or_default();
    let lk = license_key.clone();
    let amt = order.amount;
    let cur = cur.clone();
    let dl = download_url.clone();
    let settings_clone = settings.clone();
    std::thread::spawn(move || {
        crate::email::send_purchase_email_with_settings(
            &settings_clone,
            &email,
            &title,
            &note,
            &dl,
            Some(&lk),
            amt,
            &cur,
        );
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
    store: &dyn Store,
    portfolio_id: i64,
    provider: &str,
    buyer_email: &str,
) -> Result<(i64, f64, String), String> {
    let item = store
        .portfolio_find_by_id(portfolio_id)
        .filter(|i| i.sell_enabled)
        .ok_or("Item not available for purchase")?;
    let price = item
        .price
        .filter(|&p| p > 0.0)
        .ok_or("Item has no price set")?;
    let settings: HashMap<String, String> = store.setting_all();
    let cur = currency(&settings);
    let order_id = store.order_create(
        item.id,
        buyer_email,
        "",
        price,
        &cur,
        provider,
        "",
        "pending",
    )?;
    Ok((order_id, price, cur))
}

/// Simple URL encoding for query parameters
pub fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

// ── Download page: Validate token and show download UI ─

#[get("/download/<token>")]
pub fn download_page(store: &State<Arc<dyn Store>>, token: &str) -> rocket_dyn_templates::Template {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();

    let dl_token = match s.download_token_find_by_token(token) {
        Some(t) => t,
        None => {
            return rocket_dyn_templates::Template::render(
                "download",
                json!({
                    "error": "Invalid download link",
                    "settings": &settings,
                }),
            );
        }
    };

    if !dl_token.is_valid() {
        let reason = if dl_token.downloads_used >= dl_token.max_downloads {
            "Download limit reached"
        } else {
            "Download link has expired"
        };
        return rocket_dyn_templates::Template::render(
            "download",
            json!({
                "error": reason,
                "expired": true,
                "settings": &settings,
            }),
        );
    }

    let order = match s.order_find_by_id(dl_token.order_id) {
        Some(o) if o.status == "completed" => o,
        _ => {
            return rocket_dyn_templates::Template::render(
                "download",
                json!({
                    "error": "Order not found or not completed",
                    "settings": &settings,
                }),
            );
        }
    };

    let item = match s.portfolio_find_by_id(order.portfolio_id) {
        Some(i) => i,
        None => {
            return rocket_dyn_templates::Template::render(
                "download",
                json!({
                    "error": "Item not found",
                    "settings": &settings,
                }),
            );
        }
    };

    let license = s.license_find_by_order(order.id);

    rocket_dyn_templates::Template::render(
        "download",
        json!({
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
        }),
    )
}

// ── Actual file download (increments count) ────────────

#[get("/download/<token>/file")]
pub fn download_file(
    store: &State<Arc<dyn Store>>,
    token: &str,
) -> Result<rocket::response::Redirect, Json<Value>> {
    let s: &dyn Store = &**store.inner();
    let dl_token = match s.download_token_find_by_token(token) {
        Some(t) => t,
        None => {
            return Err(Json(
                json!({ "ok": false, "error": "Invalid download link" }),
            ))
        }
    };

    if !dl_token.is_valid() {
        return Err(Json(
            json!({ "ok": false, "error": "Download expired or limit reached" }),
        ));
    }

    let order = match s.order_find_by_id(dl_token.order_id) {
        Some(o) if o.status == "completed" => o,
        _ => return Err(Json(json!({ "ok": false, "error": "Order not completed" }))),
    };

    let item = match s.portfolio_find_by_id(order.portfolio_id) {
        Some(i) => i,
        None => return Err(Json(json!({ "ok": false, "error": "Item not found" }))),
    };

    // Increment download count
    let _ = s.download_token_increment(dl_token.id);

    // Serve download_file_path if set, otherwise fall back to the featured image
    let file_url = if !item.download_file_path.is_empty() {
        item.download_file_path.clone()
    } else {
        format!("/uploads/{}", item.image_path)
    };
    Ok(rocket::response::Redirect::to(file_url))
}

// ── License download (serves license.txt) ───────────────

#[get("/download/<token>/license")]
pub fn download_license(
    store: &State<Arc<dyn Store>>,
    token: &str,
) -> Result<(rocket::http::ContentType, String), Json<Value>> {
    let s: &dyn Store = &**store.inner();
    let dl_token = s
        .download_token_find_by_token(token)
        .ok_or_else(|| Json(json!({ "ok": false, "error": "Invalid download link" })))?;

    let order = s
        .order_find_by_id(dl_token.order_id)
        .filter(|o| o.status == "completed")
        .ok_or_else(|| Json(json!({ "ok": false, "error": "Order not found" })))?;

    let item = s
        .portfolio_find_by_id(order.portfolio_id)
        .ok_or_else(|| Json(json!({ "ok": false, "error": "Item not found" })))?;

    let license = s.license_find_by_order(order.id);

    let settings: HashMap<String, String> = s.setting_all();
    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let license_template = settings
        .get("downloads_license_template")
        .cloned()
        .unwrap_or_default();

    let mut txt = String::new();
    txt.push_str(&format!("License for: {}\n", item.title));
    txt.push_str(&format!("Purchased from: {}\n", site_name));
    let txn_id = if order.provider_order_id.is_empty() {
        format!("ORD-{}", order.id)
    } else {
        order.provider_order_id.clone()
    };
    txt.push_str(&format!("Transaction: {}\n", txn_id));
    txt.push_str(&format!("Date: {}\n", order.created_at));
    if let Some(ref lic) = license {
        txt.push_str(&format!("License Key: {}\n", lic.license_key));
    }
    txt.push_str("\n---\n\n");
    txt.push_str(&license_template);

    Ok((rocket::http::ContentType::Plain, txt))
}

// ── Check purchase status (for public page) ────────────

#[derive(Deserialize)]
pub struct CheckPurchaseRequest {
    pub portfolio_id: i64,
    pub email: String,
}

#[post("/api/checkout/check", format = "json", data = "<body>")]
pub fn check_purchase(
    store: &State<Arc<dyn Store>>,
    body: Json<CheckPurchaseRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();

    // Find a completed order for this email + portfolio item
    match s.order_find_completed_by_email_and_portfolio(&body.email, body.portfolio_id) {
        Some(order) => {
            let dl_token = s.download_token_find_by_order(order.id);
            let license = s.license_find_by_order(order.id);

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
    store: &State<Arc<dyn Store>>,
    body: Json<GenericCaptureRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    match finalize_order(
        s,
        body.order_id,
        &body.provider_order_id,
        &body.buyer_email,
        body.buyer_name.as_deref().unwrap_or(""),
    ) {
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
        stripe::stripe_webhook,
        razorpay::razorpay_create_order,
        razorpay::razorpay_verify,
        mollie::mollie_create_payment,
        mollie::mollie_webhook,
        mollie::mollie_return,
        square::square_create_payment,
        square::square_return,
        square::square_webhook,
        twocheckout::twocheckout_create,
        twocheckout::twocheckout_return,
        twocheckout::twocheckout_webhook,
        payoneer::payoneer_create,
        payoneer::payoneer_return,
        payoneer::payoneer_webhook,
        generic_capture_order,
        download_page,
        download_file,
        download_license,
        check_purchase,
    ]
}
