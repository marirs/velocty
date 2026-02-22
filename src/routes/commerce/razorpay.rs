use std::collections::HashMap;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Store;

use super::{create_pending_order, finalize_order};

// ── Razorpay: Create Order ──────────────────────────────

#[derive(Deserialize)]
pub struct RazorpayCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/razorpay/create", format = "json", data = "<body>")]
pub fn razorpay_create_order(
    store: &State<Arc<dyn Store>>,
    body: Json<RazorpayCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings
        .get("commerce_razorpay_enabled")
        .map(|v| v.as_str())
        != Some("true")
    {
        return Json(json!({ "ok": false, "error": "Razorpay is not enabled" }));
    }
    let key_id = settings.get("razorpay_key_id").cloned().unwrap_or_default();
    let key_secret = settings
        .get("razorpay_key_secret")
        .cloned()
        .unwrap_or_default();
    if key_id.is_empty() || key_secret.is_empty() {
        return Json(json!({ "ok": false, "error": "Razorpay credentials not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(
        s,
        body.portfolio_id,
        "razorpay",
        body.buyer_email.as_deref().unwrap_or(""),
    ) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Razorpay amounts are in smallest currency unit (paise for INR, cents for USD)
    let amount_minor = (price * 100.0) as i64;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post("https://api.razorpay.com/v1/orders")
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
                let _ = s.order_update_provider_order_id(order_id, rp_order_id);
                Json(json!({
                    "ok": true,
                    "order_id": order_id,
                    "razorpay_order_id": rp_order_id,
                    "razorpay_key_id": key_id,
                    "amount": amount_minor,
                    "currency": cur,
                }))
            } else {
                let err = body
                    .get("error")
                    .and_then(|e| e.get("description"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Razorpay API error");
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
    store: &State<Arc<dyn Store>>,
    body: Json<RazorpayVerifyRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let key_secret = settings
        .get("razorpay_key_secret")
        .cloned()
        .unwrap_or_default();

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

    match finalize_order(
        s,
        body.order_id,
        &body.razorpay_payment_id,
        &body.buyer_email,
        body.buyer_name.as_deref().unwrap_or(""),
    ) {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}
