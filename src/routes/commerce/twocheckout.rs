use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db::DbPool;
use crate::models::order::{DownloadToken, Order};
use crate::models::portfolio::PortfolioItem;
use crate::models::settings::Setting;
use std::collections::HashMap;

use super::{create_pending_order, finalize_order, site_url, urlencoding};

// ── 2Checkout: Create Hosted Checkout ───────────────────

#[derive(Deserialize)]
pub struct TwoCheckoutCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/2checkout/create", format = "json", data = "<body>")]
pub fn twocheckout_create(
    pool: &State<DbPool>,
    body: Json<TwoCheckoutCreateRequest>,
) -> Json<Value> {
    let settings: HashMap<String, String> = Setting::all(pool);
    if settings.get("commerce_2checkout_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "2Checkout is not enabled" }));
    }
    let merchant_code = settings.get("twocheckout_merchant_code").cloned().unwrap_or_default();
    if merchant_code.is_empty() {
        return Json(json!({ "ok": false, "error": "2Checkout merchant code not configured" }));
    }
    let is_sandbox = settings.get("twocheckout_mode").map(|v| v.as_str()) != Some("live");

    let (order_id, price, cur) = match create_pending_order(pool, body.portfolio_id, "2checkout", body.buyer_email.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };
    let item = PortfolioItem::find_by_id(pool, body.portfolio_id).unwrap();
    let base = site_url(&settings);

    // 2Checkout uses a hosted checkout URL with query parameters
    let checkout_base = if is_sandbox { "https://sandbox.2checkout.com/checkout/purchase" } else { "https://www.2checkout.com/checkout/purchase" };
    let checkout_url = format!(
        "{}?seller_id={}&product_id=velocty_{}&price={:.2}&currency={}&return-url={}/api/2checkout/return?order_id={}&return-type=redirect&prod={}&qty=1",
        checkout_base, merchant_code, order_id, price, cur, base, order_id, urlencoding(&item.title)
    );

    Json(json!({ "ok": true, "order_id": order_id, "checkout_url": checkout_url }))
}

// ── 2Checkout: Return redirect ──────────────────────────

#[get("/api/2checkout/return?<order_id>")]
pub fn twocheckout_return(
    pool: &State<DbPool>,
    order_id: i64,
) -> rocket::response::Redirect {
    let settings: HashMap<String, String> = Setting::all(pool);
    let base = site_url(&settings);
    if let Some(order) = Order::find_by_id(pool, order_id) {
        if order.status == "pending" {
            if let Ok(result) = finalize_order(pool, order_id, &format!("2co_{}", order_id), &order.buyer_email, "") {
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
