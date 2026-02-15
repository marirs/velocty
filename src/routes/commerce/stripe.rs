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

// ── Stripe: Create Checkout Session ────────────────────

#[derive(Deserialize)]
pub struct StripeCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/stripe/create", format = "json", data = "<body>")]
pub fn stripe_create_session(
    pool: &State<DbPool>,
    body: Json<StripeCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_stripe_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Stripe is not enabled" }));
    }
    let secret_key = settings.get("stripe_secret_key").cloned().unwrap_or_default();
    if secret_key.is_empty() {
        return Json(json!({ "ok": false, "error": "Stripe secret key not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "stripe", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    // Call Stripe API to create a Checkout Session
    let client = reqwest::blocking::Client::new();
    let resp = client.post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&secret_key, None::<&str>)
        .form(&[
            ("mode", "payment"),
            ("success_url", &format!("{}/api/stripe/success?session_id={{CHECKOUT_SESSION_ID}}&order_id={}", base, order_id)),
            ("cancel_url", &format!("{}/portfolio/{}", base, item.slug)),
            ("line_items[0][price_data][currency]", &cur.to_lowercase()),
            ("line_items[0][price_data][unit_amount]", &format!("{}", (price * 100.0) as i64)),
            ("line_items[0][price_data][product_data][name]", &item.title),
            ("line_items[0][quantity]", "1"),
            ("metadata[order_id]", &order_id.to_string()),
        ])
        .send();

    match resp {
        Ok(r) => {
            let body: Value = r.json().unwrap_or_default();
            if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
                let session_id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let _ = Order::update_provider_order_id(pool, order_id, session_id);
                Json(json!({ "ok": true, "order_id": order_id, "checkout_url": url, "session_id": session_id }))
            } else {
                let err = body.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("Stripe API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Stripe request failed: {}", e) })),
    }
}

// ── Stripe: Success redirect (captures after checkout) ──

#[get("/api/stripe/success?<session_id>&<order_id>")]
pub fn stripe_success(
    pool: &State<DbPool>,
    session_id: &str,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let secret_key = settings.get("stripe_secret_key").cloned().unwrap_or_default();
    let base = site_url(&settings);

    // Verify session is paid
    let client = reqwest::blocking::Client::new();
    let session_data = client.get(&format!("https://api.stripe.com/v1/checkout/sessions/{}", session_id))
        .basic_auth(&secret_key, None::<&str>)
        .send()
        .ok()
        .and_then(|r| r.json::<Value>().ok());

    let verified = session_data.as_ref()
        .and_then(|v| v.get("payment_status").and_then(|s| s.as_str()).map(|s| s == "paid"))
        .unwrap_or(false);

    if verified {
        let buyer_email = session_data.as_ref()
            .and_then(|v| v.get("customer_details"))
            .and_then(|c| c.get("email"))
            .and_then(|e| e.as_str())
            .unwrap_or("");

        if let Ok(result) = finalize_order(pool, order_id, session_id, buyer_email, "") {
            if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                return rocket::response::Redirect::to(format!("/download/{}", token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}
