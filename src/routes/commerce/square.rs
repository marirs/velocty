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
