use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::Order;
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

use super::{create_pending_order, currency, finalize_order};

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

    let cur = currency(&settings);

    // Create a pending order in our DB
    let order_id = match Order::create(
        pool, item.id, "", "", price, &cur, "paypal", "", "pending",
    ) {
        Ok(id) => id,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Return the order info for the PayPal JS SDK to create the PayPal order client-side
    Json(json!({
        "ok": true,
        "order_id": order_id,
        "amount": format!("{:.2}", price),
        "currency": cur,
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
