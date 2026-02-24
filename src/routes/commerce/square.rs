use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use std::collections::HashMap;
use std::sync::Arc;

use crate::store::Store;

use super::stripe::RawBody;
use super::{create_pending_order, finalize_order, site_url};

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
    store: &State<Arc<dyn Store>>,
    body: Json<SquareCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings.get("commerce_square_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Square is not enabled" }));
    }
    let access_token = settings
        .get("square_access_token")
        .cloned()
        .unwrap_or_default();
    let location_id = settings
        .get("square_location_id")
        .cloned()
        .unwrap_or_default();
    if access_token.is_empty() || location_id.is_empty() {
        return Json(json!({ "ok": false, "error": "Square credentials not configured" }));
    }
    let is_sandbox = settings.get("square_mode").map(|v| v.as_str()) != Some("live");
    let api_base = if is_sandbox {
        "https://connect.squareupsandbox.com"
    } else {
        "https://connect.squareup.com"
    };

    let (order_id, order_uuid, price, cur) = match create_pending_order(
        s,
        body.portfolio_id,
        "square",
        body.buyer_email.as_deref().unwrap_or(""),
    ) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = match s.portfolio_find_by_id(body.portfolio_id) {
        Some(i) => i,
        None => return Json(json!({ "ok": false, "error": "Item not found" })),
    };
    let base = site_url(&settings);
    let amount_minor = (price * 100.0) as i64;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/v2/online-checkout/payment-links", api_base))
        .bearer_auth(&access_token)
        .json(&json!({
            "idempotency_key": format!("velocty_{}", order_id),
            "quick_pay": {
                "name": item.title,
                "price_money": { "amount": amount_minor, "currency": cur },
                "location_id": location_id
            },
            "checkout_options": {
                "redirect_url": format!("{}/api/square/return?order_id={}", base, order_uuid)
            },
            "payment_note": format!("Order #{}", order_uuid)
        }))
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            let url = body
                .get("payment_link")
                .and_then(|l| l.get("url"))
                .and_then(|u| u.as_str());
            let sq_id = body
                .get("payment_link")
                .and_then(|l| l.get("id"))
                .and_then(|i| i.as_str())
                .unwrap_or("");
            if let Some(checkout_url) = url {
                let _ = s.order_update_provider_order_id(order_id, sq_id);
                Json(json!({ "ok": true, "order_id": order_uuid, "checkout_url": checkout_url }))
            } else {
                let errors = body
                    .get("errors")
                    .and_then(|e| e.as_array())
                    .and_then(|a| a.first())
                    .and_then(|e| e.get("detail"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("Square API error");
                Json(json!({ "ok": false, "error": errors }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Square request failed: {}", e) })),
    }
}

// ── Square: Return redirect ─────────────────────────────

#[get("/api/square/return?<order_id>")]
pub fn square_return(store: &State<Arc<dyn Store>>, order_id: &str) -> rocket::response::Redirect {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let base = site_url(&settings);
    // Only redirect to download if the webhook already completed the order.
    // Never finalize from a return redirect — that must come from the verified webhook.
    if let Some(order) = s.order_find_by_uuid(order_id) {
        if order.status == "completed" {
            if let Some(dl) = s.download_token_find_by_order(order.id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    // Webhook hasn't fired yet — redirect to home
    rocket::response::Redirect::to(base)
}

// ── Square: Webhook (payment confirmation) ──────────────

/// Verify Square webhook signature: HMAC-SHA256(webhook_url + body) with signature key
fn verify_square_signature(
    webhook_url: &str,
    payload: &[u8],
    signature: &str,
    signature_key: &str,
) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(signature_key.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(webhook_url.as_bytes());
    mac.update(payload);

    use base64::{engine::general_purpose::STANDARD, Engine};
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    super::constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

#[post("/api/square/webhook", data = "<body>")]
pub fn square_webhook(
    store: &State<Arc<dyn Store>>,
    sig: SquareSignature,
    body: RawBody,
) -> Status {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let signature_key = settings
        .get("square_webhook_signature_key")
        .cloned()
        .unwrap_or_default();
    let base = site_url(&settings);
    let webhook_url = format!("{}/api/square/webhook", base);

    if signature_key.is_empty() {
        eprintln!("[square] Webhook signature key not configured, rejecting");
        return Status::BadRequest;
    }
    if !verify_square_signature(&webhook_url, &body.0, &sig.0, &signature_key) {
        eprintln!("[square] Invalid webhook signature");
        return Status::BadRequest;
    }

    let event: Value = match serde_json::from_slice(&body.0) {
        Ok(v) => v,
        Err(_) => return Status::BadRequest,
    };

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if event_type == "payment.completed" {
        let payment = match event
            .get("data")
            .and_then(|d| d.get("object"))
            .and_then(|o| o.get("payment"))
        {
            Some(p) => p,
            None => return Status::Ok,
        };

        let note = payment.get("note").and_then(|n| n.as_str()).unwrap_or("");
        // note format: "Order #<uuid>"
        let order_uuid = note.strip_prefix("Order #").unwrap_or("");
        let buyer_email = payment
            .get("buyer_email_address")
            .and_then(|e| e.as_str())
            .unwrap_or("");
        let sq_payment_id = payment.get("id").and_then(|i| i.as_str()).unwrap_or("");

        if !order_uuid.is_empty() {
            let _ = finalize_order(s, order_uuid, sq_payment_id, buyer_email, "");
        }
    }

    Status::Ok
}
