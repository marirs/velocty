use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use serde_json::{json, Value};

use std::collections::HashMap;
use std::sync::Arc;

use crate::store::Store;

use super::{create_pending_order, finalize_order, site_url};

/// Raw body guard for webhook signature verification
pub struct RawBody(pub Vec<u8>);

#[rocket::async_trait]
impl<'r> rocket::data::FromData<'r> for RawBody {
    type Error = std::io::Error;

    async fn from_data(
        _req: &'r Request<'_>,
        data: rocket::data::Data<'r>,
    ) -> rocket::data::Outcome<'r, Self> {
        use rocket::data::ToByteUnit;
        match data.open(1.mebibytes()).into_bytes().await {
            Ok(bytes) if bytes.is_complete() => {
                rocket::data::Outcome::Success(RawBody(bytes.into_inner()))
            }
            Ok(_) => rocket::data::Outcome::Error((
                Status::PayloadTooLarge,
                std::io::Error::other("Payload too large"),
            )),
            Err(e) => rocket::data::Outcome::Error((Status::InternalServerError, e)),
        }
    }
}

/// Extract Stripe-Signature header
pub struct StripeSignature(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for StripeSignature {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match req.headers().get_one("Stripe-Signature") {
            Some(sig) => Outcome::Success(StripeSignature(sig.to_string())),
            None => Outcome::Error((Status::BadRequest, ())),
        }
    }
}

// ── Stripe: Create Checkout Session ────────────────────

#[derive(Deserialize)]
pub struct StripeCreateRequest {
    pub portfolio_id: i64,
    pub buyer_email: Option<String>,
}

#[post("/api/checkout/stripe/create", format = "json", data = "<body>")]
pub fn stripe_create_session(
    store: &State<Arc<dyn Store>>,
    body: Json<StripeCreateRequest>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    if settings.get("commerce_stripe_enabled").map(|v| v.as_str()) != Some("true") {
        return Json(json!({ "ok": false, "error": "Stripe is not enabled" }));
    }
    let secret_key = settings
        .get("stripe_secret_key")
        .cloned()
        .unwrap_or_default();
    if secret_key.is_empty() {
        return Json(json!({ "ok": false, "error": "Stripe secret key not configured" }));
    }

    let (order_id, price, cur) = match create_pending_order(
        s,
        body.portfolio_id,
        "stripe",
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

    // Call Stripe API to create a Checkout Session
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&secret_key, None::<&str>)
        .form(&[
            ("mode", "payment"),
            (
                "success_url",
                &format!(
                    "{}/api/stripe/success?session_id={{CHECKOUT_SESSION_ID}}&order_id={}",
                    base, order_id
                ),
            ),
            ("cancel_url", &format!("{}/portfolio/{}", base, item.slug)),
            ("line_items[0][price_data][currency]", &cur.to_lowercase()),
            (
                "line_items[0][price_data][unit_amount]",
                &format!("{}", (price * 100.0) as i64),
            ),
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
                let _ = s.order_update_provider_order_id(order_id, session_id);
                Json(
                    json!({ "ok": true, "order_id": order_id, "checkout_url": url, "session_id": session_id }),
                )
            } else {
                let err = body
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Stripe API error");
                Json(json!({ "ok": false, "error": err }))
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Stripe request failed: {}", e) })),
    }
}

// ── Stripe: Success redirect (captures after checkout) ──

#[get("/api/stripe/success?<session_id>&<order_id>")]
pub fn stripe_success(
    store: &State<Arc<dyn Store>>,
    session_id: &str,
    order_id: i64,
) -> rocket::response::Redirect {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let secret_key = settings
        .get("stripe_secret_key")
        .cloned()
        .unwrap_or_default();
    let base = site_url(&settings);

    // Verify session is paid
    let client = reqwest::blocking::Client::new();
    let session_data = client
        .get(format!(
            "https://api.stripe.com/v1/checkout/sessions/{}",
            session_id
        ))
        .basic_auth(&secret_key, None::<&str>)
        .send()
        .ok()
        .and_then(|r| r.json::<Value>().ok());

    let verified = session_data
        .as_ref()
        .and_then(|v| {
            v.get("payment_status")
                .and_then(|s| s.as_str())
                .map(|s| s == "paid")
        })
        .unwrap_or(false);

    if verified {
        let buyer_email = session_data
            .as_ref()
            .and_then(|v| v.get("customer_details"))
            .and_then(|c| c.get("email"))
            .and_then(|e| e.as_str())
            .unwrap_or("");

        if let Ok(result) = finalize_order(s, order_id, session_id, buyer_email, "") {
            if let Some(token) = result.get("download_token").and_then(|t| t.as_str()) {
                return rocket::response::Redirect::to(format!("/download/{}", token));
            }
        }
    }
    rocket::response::Redirect::to(base)
}

// ── Stripe: Webhook (primary payment confirmation) ──────

/// Verify Stripe webhook signature: HMAC-SHA256 of "timestamp.payload" with webhook secret
fn verify_stripe_signature(payload: &[u8], sig_header: &str, secret: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    // Parse header: t=timestamp,v1=signature
    let mut timestamp = "";
    let mut signature = "";
    for part in sig_header.split(',') {
        if let Some(t) = part.strip_prefix("t=") {
            timestamp = t;
        } else if let Some(s) = part.strip_prefix("v1=") {
            signature = s;
        }
    }
    if timestamp.is_empty() || signature.is_empty() {
        return false;
    }

    // Compute expected signature
    let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(signed_payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    expected == signature
}

#[post("/api/stripe/webhook", data = "<body>")]
pub fn stripe_webhook(
    store: &State<Arc<dyn Store>>,
    sig: StripeSignature,
    body: RawBody,
) -> Status {
    let s: &dyn Store = &**store.inner();
    let settings: HashMap<String, String> = s.setting_all();
    let webhook_secret = settings
        .get("stripe_webhook_secret")
        .cloned()
        .unwrap_or_default();

    if webhook_secret.is_empty() {
        eprintln!("[stripe] Webhook secret not configured, rejecting");
        return Status::BadRequest;
    }

    if !verify_stripe_signature(&body.0, &sig.0, &webhook_secret) {
        eprintln!("[stripe] Invalid webhook signature");
        return Status::BadRequest;
    }

    // Parse the event
    let event: Value = match serde_json::from_slice(&body.0) {
        Ok(v) => v,
        Err(_) => return Status::BadRequest,
    };

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

    if event_type == "checkout.session.completed" {
        let session = match event.get("data").and_then(|d| d.get("object")) {
            Some(s) => s,
            None => return Status::BadRequest,
        };

        let payment_status = session
            .get("payment_status")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        if payment_status != "paid" {
            return Status::Ok;
        }

        let order_id_str = session
            .get("metadata")
            .and_then(|m| m.get("order_id"))
            .and_then(|o| o.as_str())
            .unwrap_or("");
        let session_id = session.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let buyer_email = session
            .get("customer_details")
            .and_then(|c| c.get("email"))
            .and_then(|e| e.as_str())
            .unwrap_or("");

        if let Ok(order_id) = order_id_str.parse::<i64>() {
            let _ = finalize_order(s, order_id, session_id, buyer_email, "");
        }
    }

    Status::Ok
}
