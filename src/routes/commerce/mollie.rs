use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

use super::{create_pending_order, finalize_order, site_url};

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
    // Webhook hasn't fired yet — redirect to home
    rocket::response::Redirect::to(base)
}
