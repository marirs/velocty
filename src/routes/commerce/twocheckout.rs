use std::collections::HashMap;
use std::sync::Arc;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Store;

use super::{create_pending_order, finalize_order, site_url, urlencoding};

// ── 2Checkout: Create Hosted Checkout ───────────────────

#[derive(Deserialize)]
pub struct TwoCheckoutCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/2checkout/create", format = "json", data = "<body>")]
pub fn twocheckout_create(
    store: &State<Arc<dyn Store>>,
    body: Json<TwoCheckoutCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings
        .get("commerce_2checkout_enabled")
        .map(|v| v.as_str())
        != Some("true")
    {
        return Json(json!({ "ok": false, "error": "2Checkout is not enabled" }));
    }
    let merchant_code = settings
        .get("twocheckout_merchant_code")
        .cloned()
        .unwrap_or_default();
    if merchant_code.is_empty() {
        return Json(json!({ "ok": false, "error": "2Checkout merchant code not configured" }));
    }
    let is_sandbox = settings.get("twocheckout_mode").map(|v| v.as_str()) != Some("live");

    let (order_id, price, cur) = match create_pending_order(
        s,
        body.portfolio_id,
        "2checkout",
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

    // 2Checkout uses a hosted checkout URL with query parameters
    let checkout_base = if is_sandbox {
        "https://sandbox.2checkout.com/checkout/purchase"
    } else {
        "https://www.2checkout.com/checkout/purchase"
    };
    let checkout_url = format!(
        "{}?seller_id={}&product_id=velocty_{}&price={:.2}&currency={}&return-url={}/api/2checkout/return?order_id={}&return-type=redirect&prod={}&qty=1",
        checkout_base, merchant_code, order_id, price, cur, base, order_id, urlencoding(&item.title)
    );

    Json(json!({ "ok": true, "order_id": order_id, "checkout_url": checkout_url }))
}

// ── 2Checkout: Return redirect ──────────────────────────

#[get("/api/2checkout/return?<order_id>")]
pub fn twocheckout_return(
    store: &State<Arc<dyn Store>>,
    order_id: i64,
) -> rocket::response::Redirect {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let base = site_url(&settings);
    // Only redirect to download if the webhook already completed the order.
    // Never finalize from a return redirect — that must come from the verified webhook.
    if let Some(order) = s.order_find_by_id(order_id) {
        if order.status == "completed" {
            if let Some(dl) = s.download_token_find_by_order(order_id) {
                return rocket::response::Redirect::to(format!("/download/{}", dl.token));
            }
        }
    }
    // Webhook hasn't fired yet — redirect to home
    rocket::response::Redirect::to(base)
}

// ── 2Checkout: IPN/INS Webhook ──────────────────────────

#[derive(FromForm)]
pub struct TwoCheckoutIpn {
    pub sale_id: Option<String>,
    pub invoice_id: Option<String>,
    pub vendor_id: Option<String>,
    pub md5_hash: Option<String>,
    pub message_type: Option<String>,
    pub vendor_order_id: Option<String>,
    pub customer_email: Option<String>,
    pub customer_name: Option<String>,
}

#[post(
    "/api/2checkout/webhook",
    format = "application/x-www-form-urlencoded",
    data = "<body>"
)]
pub fn twocheckout_webhook(
    store: &State<Arc<dyn Store>>,
    body: rocket::form::Form<TwoCheckoutIpn>,
) -> Status {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let secret_key = settings
        .get("twocheckout_secret_key")
        .cloned()
        .unwrap_or_default();

    if secret_key.is_empty() {
        eprintln!("[2checkout] Secret key not configured, rejecting webhook");
        return Status::BadRequest;
    }

    let sale_id = body.sale_id.as_deref().unwrap_or("");
    let vendor_id = body.vendor_id.as_deref().unwrap_or("");
    let invoice_id = body.invoice_id.as_deref().unwrap_or("");
    let md5_hash = body.md5_hash.as_deref().unwrap_or("");

    // 2Checkout IPN hash: UPPERCASE(MD5(sale_id + vendor_id + invoice_id + secret_word))
    use md5::{Digest, Md5};
    let hash_input = format!("{}{}{}{}", sale_id, vendor_id, invoice_id, secret_key);
    let mut hasher = Md5::new();
    hasher.update(hash_input.as_bytes());
    let computed = format!("{:X}", hasher.finalize());

    if !computed.eq_ignore_ascii_case(md5_hash) {
        eprintln!(
            "[2checkout] Invalid IPN hash: expected={}, got={}",
            computed, md5_hash
        );
        return Status::BadRequest;
    }

    let message_type = body.message_type.as_deref().unwrap_or("");
    if message_type == "ORDER_CREATED" || message_type == "INVOICE_STATUS_CHANGED" {
        // vendor_order_id format from our create: velocty_{order_id} or just the order_id
        let vendor_order_id = body.vendor_order_id.as_deref().unwrap_or("");
        let order_id_str = vendor_order_id
            .strip_prefix("velocty_")
            .unwrap_or(vendor_order_id);
        if let Ok(oid) = order_id_str.parse::<i64>() {
            let email = body.customer_email.as_deref().unwrap_or("");
            let name = body.customer_name.as_deref().unwrap_or("");
            let _ = finalize_order(s, oid, sale_id, email, name);
        }
    }

    Status::Ok
}
