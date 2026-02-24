use std::collections::HashMap;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Store;

use super::{create_pending_order, finalize_order, site_url};

// ── Payoneer: Create Checkout ───────────────────────────

#[derive(Deserialize)]
pub struct PayoneerCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/payoneer/create", format = "json", data = "<body>")]
pub fn payoneer_create(
    store: &State<Arc<dyn Store>>,
    body: Json<PayoneerCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings
        .get("commerce_payoneer_enabled")
        .map(|v| v.as_str())
        != Some("true")
    {
        return Json(json!({ "ok": false, "error": "Payoneer is not enabled" }));
    }
    let client_id = settings
        .get("payoneer_client_id")
        .cloned()
        .unwrap_or_default();
    let client_secret = settings
        .get("payoneer_client_secret")
        .cloned()
        .unwrap_or_default();
    let program_id = settings
        .get("payoneer_program_id")
        .cloned()
        .unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() || program_id.is_empty() {
        return Json(json!({ "ok": false, "error": "Payoneer credentials not configured" }));
    }
    let is_sandbox = settings.get("payoneer_mode").map(|v| v.as_str()) != Some("live");
    let api_base = if is_sandbox {
        "https://api.sandbox.payoneer.com"
    } else {
        "https://api.payoneer.com"
    };

    let (order_id, order_uuid, price, cur) = match create_pending_order(
        s,
        body.portfolio_id,
        "payoneer",
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

    // Get OAuth token
    let client = reqwest::blocking::Client::new();
    let token_resp = client
        .post(format!("{}/v4/programs/{}/token", api_base, program_id))
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send();

    let access_token = match token_resp {
        Ok(r) => {
            let data: Value = r.json().unwrap_or_default();
            match data.get("access_token").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => {
                    return Json(
                        json!({ "ok": false, "error": "Failed to get Payoneer access token" }),
                    )
                }
            }
        }
        Err(e) => {
            return Json(json!({ "ok": false, "error": format!("Payoneer auth failed: {}", e) }))
        }
    };

    // Create a checkout link
    let resp = client
        .post(format!("{}/v4/programs/{}/checkout", api_base, program_id))
        .bearer_auth(&access_token)
        .json(&json!({
            "amount": price,
            "currency": cur,
            "description": item.title,
            "payout_id": format!("velocty_{}", order_uuid),
            "redirect_url": format!("{}/api/payoneer/return?order_id={}", base, order_uuid),
            "notification_url": format!("{}/api/payoneer/webhook", base)
        }))
        .send();

    match resp {
        Ok(r) => {
            let data: Value = r.json().unwrap_or_default();
            let checkout_url = data
                .get("redirect_url")
                .or_else(|| data.get("checkout_url"))
                .and_then(|u| u.as_str());
            if let Some(url) = checkout_url {
                let pyo_id = data.get("payout_id").and_then(|i| i.as_str()).unwrap_or("");
                let _ = s.order_update_provider_order_id(order_id, pyo_id);
                Json(json!({ "ok": true, "order_id": order_uuid, "checkout_url": url }))
            } else {
                let err = data
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("Payoneer API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Payoneer request failed: {}", e) })),
    }
}

// ── Payoneer: Return redirect ───────────────────────────

#[get("/api/payoneer/return?<order_id>")]
pub fn payoneer_return(
    store: &State<Arc<dyn Store>>,
    order_id: &str,
) -> rocket::response::Redirect {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let base = site_url(&settings);
    // Only redirect to download if the webhook already completed the order.
    // Never finalize from a return redirect — that must come from the webhook.
    if let Some(order) = s.order_find_by_uuid(order_id) {
        if order.status == "completed" {
            if let Some(dl) = s.download_token_find_by_order(order.id) {
                return rocket::response::Redirect::to(format!("/download/{}/file", dl.token));
            }
        }
    }
    // Webhook hasn't fired yet — redirect to home
    rocket::response::Redirect::to(base)
}

// ── Payoneer: Webhook ───────────────────────────────────

#[post("/api/payoneer/webhook", format = "json", data = "<body>")]
pub fn payoneer_webhook(store: &State<Arc<dyn Store>>, body: Json<Value>) -> &'static str {
    let s: &dyn Store = &**store.inner();
    let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
    let payout_id = body.get("payout_id").and_then(|p| p.as_str()).unwrap_or("");
    if status == "done" || status == "completed" {
        // payout_id format: velocty_{uuid}
        if let Some(order_uuid) = payout_id.strip_prefix("velocty_") {
            if !order_uuid.is_empty() {
                // Verify order exists and is pending before finalizing
                match s.order_find_by_uuid(order_uuid) {
                    Some(order) if order.status == "pending" => {
                        let email = body
                            .get("payee_email")
                            .and_then(|e| e.as_str())
                            .unwrap_or("");
                        let _ = finalize_order(s, order_uuid, payout_id, email, "");
                    }
                    Some(order) => {
                        log::warn!(
                            "[payoneer] Webhook for order {} with status '{}', ignoring",
                            order_uuid,
                            order.status
                        );
                    }
                    None => {
                        log::warn!("[payoneer] Webhook for unknown order {}", order_uuid);
                    }
                }
            }
        }
    }
    "OK"
}
