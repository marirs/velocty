use rocket::serde::json::Json;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

use super::{create_pending_order, finalize_order, site_url};
use super::stripe::RawBody;

/// Extract Square webhook signature header
pub struct SquareSignature(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for SquareSignature {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("x-square-hmacsha256-signature") {
            Some(sig) => Outcome::Success(SquareSignature(sig.to_string())),
            None => Outcome::Error((Status::BadRequest, ())),
        }
    }
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
    let item = match PortfolioItem::find_by_id(pool, body.portfolio_id) {
        Some(i) => i,
        None => return Json(json!({ "ok": false, "error": "Item not found" })),
    };
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

// ── Square: Webhook (payment confirmation) ──────────────

/// Verify Square webhook signature: HMAC-SHA256(webhook_url + body) with signature key
fn verify_square_signature(webhook_url: &str, payload: &[u8], signature: &str, signature_key: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(signature_key.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(webhook_url.as_bytes());
    mac.update(payload);

    use base64::{Engine, engine::general_purpose::STANDARD};
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    expected == signature
}

#[post("/api/square/webhook", data = "<body>")]
pub fn square_webhook(
    pool: &State<DbPool>,
    sig: SquareSignature,
    body: RawBody,
) -> Status {
    let settings: HashMap<String, String> = Setting::all(pool);
    let signature_key = settings.get("square_webhook_signature_key").cloned().unwrap_or_default();
    let base = site_url(&settings);
    let webhook_url = format!("{}/api/square/webhook", base);

    if !signature_key.is_empty() {
        if !verify_square_signature(&webhook_url, &body.0, &sig.0, &signature_key) {
            eprintln!("[square] Invalid webhook signature");
            return Status::BadRequest;
        }
    }

    let event: Value = match serde_json::from_slice(&body.0) {
        Ok(v) => v,
        Err(_) => return Status::BadRequest,
    };

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if event_type == "payment.completed" {
        let payment = match event.get("data").and_then(|d| d.get("object")).and_then(|o| o.get("payment")) {
            Some(p) => p,
            None => return Status::Ok,
        };

        let note = payment.get("note").and_then(|n| n.as_str()).unwrap_or("");
        // note format: "Order #123"
        let order_id_str = note.strip_prefix("Order #").unwrap_or("");
        let buyer_email = payment.get("buyer_email_address").and_then(|e| e.as_str()).unwrap_or("");
        let sq_payment_id = payment.get("id").and_then(|i| i.as_str()).unwrap_or("");

        if let Ok(oid) = order_id_str.parse::<i64>() {
            let _ = finalize_order(pool, oid, sq_payment_id, buyer_email, "");
        }
    }

    Status::Ok
}
