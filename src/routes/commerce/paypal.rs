use std::collections::HashMap;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Store;

use super::{currency, finalize_order};

// ── PayPal: Create Order ───────────────────────────────

#[derive(Deserialize)]
pub struct PaypalCreateRequest {
    pub portfolio_id: i64,
}

#[post("/api/checkout/paypal/create", format = "json", data = "<body>")]
pub fn paypal_create_order(
    store: &State<Arc<dyn Store>>,
    body: Json<PaypalCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();

    if settings.get("commerce_paypal_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "PayPal is not enabled" }));
    }

    let item = match s.portfolio_find_by_id(body.portfolio_id) {
        Some(i) if i.sell_enabled => i,
        _ => return Json(json!({ "ok": false, "error": "Item not available for purchase" })),
    };

    let price = match item.price {
        Some(p) if p > 0.0 => p,
        _ => return Json(json!({ "ok": false, "error": "Item has no price set" })),
    };

    let cur = currency(&settings);

    // Create a pending order in our DB
    let (_order_id, order_uuid) =
        match s.order_create(item.id, "", "", price, &cur, "paypal", "", "pending") {
            Ok(v) => v,
            Err(e) => return Json(json!({ "ok": false, "error": e })),
        };

    // Return the order info for the PayPal JS SDK to create the PayPal order client-side
    Json(json!({
        "ok": true,
        "order_id": order_uuid,
        "amount": format!("{:.2}", price),
        "currency": cur,
        "item_title": item.title,
    }))
}

// ── PayPal: Capture Order (after buyer approves) ───────

#[derive(Deserialize)]
pub struct PaypalCaptureRequest {
    pub order_id: String,
    pub paypal_order_id: String,
    pub buyer_email: String,
    pub buyer_name: Option<String>,
}

#[post("/api/checkout/paypal/capture", format = "json", data = "<body>")]
pub fn paypal_capture_order(
    store: &State<Arc<dyn Store>>,
    body: Json<PaypalCaptureRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings.get("commerce_paypal_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "PayPal is not enabled" }));
    }

    // Server-side verification: fetch order details from PayPal API
    let client_id = settings
        .get("paypal_client_id")
        .cloned()
        .unwrap_or_default();
    let secret = settings.get("paypal_secret").cloned().unwrap_or_default();
    if client_id.is_empty() || secret.is_empty() {
        return Json(json!({ "ok": false, "error": "PayPal credentials not configured" }));
    }
    let is_sandbox = settings.get("paypal_mode").map(|v| v.as_str()) != Some("live");
    let api_base = if is_sandbox {
        "https://api-m.sandbox.paypal.com"
    } else {
        "https://api-m.paypal.com"
    };

    // Get OAuth token
    let client = reqwest::blocking::Client::new();
    let token_resp = client
        .post(format!("{}/v1/oauth2/token", api_base))
        .basic_auth(&client_id, Some(&secret))
        .form(&[("grant_type", "client_credentials")])
        .send();
    let access_token = match token_resp {
        Ok(r) => match r.json::<Value>().ok().and_then(|v| {
            v.get("access_token")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        }) {
            Some(t) => t,
            None => {
                return Json(json!({ "ok": false, "error": "Failed to get PayPal access token" }))
            }
        },
        Err(e) => {
            return Json(json!({ "ok": false, "error": format!("PayPal auth failed: {}", e) }))
        }
    };

    // Verify the PayPal order is COMPLETED/APPROVED
    let order_resp = client
        .get(format!(
            "{}/v2/checkout/orders/{}",
            api_base, body.paypal_order_id
        ))
        .bearer_auth(&access_token)
        .send();
    let verified = match order_resp {
        Ok(r) => {
            let data: Value = r.json().unwrap_or_default();
            let status = data.get("status").and_then(|s| s.as_str()).unwrap_or("");
            status == "COMPLETED" || status == "APPROVED"
        }
        Err(_) => false,
    };
    if !verified {
        return Json(json!({ "ok": false, "error": "PayPal payment not verified" }));
    }

    match finalize_order(
        s,
        &body.order_id,
        &body.paypal_order_id,
        &body.buyer_email,
        body.buyer_name.as_deref().unwrap_or(""),
    ) {
        Ok(v) => Json(v),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}
