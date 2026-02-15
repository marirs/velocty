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
    let item = match PortfolioItem::find_by_id(pool, body.portfolio_id) {
        Some(i) => i,
        None => return Json(json!({ "ok": false, "error": "Item not found" })),
    };
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
