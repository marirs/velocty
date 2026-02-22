use rocket::http::CookieJar;
use rocket::serde::json::Json;
use rocket::State;
use serde_json::{json, Value};

use std::sync::Arc;

use crate::security::{auth, passkey};
use crate::store::Store;
use crate::AdminSlug;

/// Check if any user has passkey auth enabled (used by login page to show passkey button)
#[get("/passkey/check?<email>")]
pub fn passkey_check(store: &State<Arc<dyn Store>>, email: Option<&str>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let email = match email {
        Some(e) if !e.is_empty() => e,
        _ => return Json(json!({ "ok": true, "has_passkey": false })),
    };
    let user = match s.user_get_by_email(email) {
        Some(u) => u,
        None => return Json(json!({ "ok": true, "has_passkey": false })),
    };
    let has = user.auth_method == "passkey" && s.passkey_count_for_user(user.id) > 0;
    Json(json!({ "ok": true, "has_passkey": has }))
}

/// Start passkey authentication — returns challenge options for navigator.credentials.get()
#[post("/passkey/auth/start", format = "json", data = "<body>")]
pub fn passkey_auth_start(store: &State<Arc<dyn Store>>, body: Json<Value>) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("");

    if email.is_empty() {
        return Json(json!({ "ok": false, "error": "Email is required" }));
    }

    let user = match s.user_get_by_email(email) {
        Some(u) => u,
        None => return Json(json!({ "ok": false, "error": "Invalid credentials" })),
    };

    if user.auth_method != "passkey" {
        return Json(json!({ "ok": false, "error": "Passkey not enabled for this account" }));
    }

    let credentials = passkey::load_credentials(s, user.id);
    if credentials.is_empty() {
        return Json(json!({ "ok": false, "error": "No passkeys registered" }));
    }

    let webauthn = match passkey::build_webauthn(s) {
        Ok(w) => w,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    match webauthn.start_passkey_authentication(&credentials) {
        Ok((rcr, auth_state)) => {
            let token = uuid::Uuid::new_v4().to_string();
            passkey::store_auth_state(s, &token, &auth_state);
            let _ = s.setting_set(
                &format!("passkey_auth_user_{}", token),
                &user.id.to_string(),
            );
            Json(json!({ "ok": true, "options": rcr, "token": token }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Auth start failed: {}", e) })),
    }
}

/// Finish passkey authentication — verify assertion and create session
#[post("/passkey/auth/finish", format = "json", data = "<body>")]
pub fn passkey_auth_finish(
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    cookies: &CookieJar<'_>,
    client_ip: auth::ClientIp,
    body: Json<Value>,
) -> Json<Value> {
    let s: &dyn Store = &**store.inner();
    let ip = &client_ip.0;
    let data = body.into_inner();

    let token = match data.get("token").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return Json(json!({ "ok": false, "error": "Missing token" })),
    };

    let credential = match data.get("credential") {
        Some(c) => c.clone(),
        None => return Json(json!({ "ok": false, "error": "Missing credential" })),
    };

    // Retrieve stored auth state
    let auth_state = match passkey::take_auth_state(s, &token) {
        Some(state) => state,
        None => {
            return Json(
                json!({ "ok": false, "error": "No pending authentication. Please try again." }),
            )
        }
    };

    // Retrieve user_id
    let user_key = format!("passkey_auth_user_{}", token);
    let user_id_str = s.setting_get_or(&user_key, "");
    let _ = s.setting_set(&user_key, "");
    let user_id: i64 = match user_id_str.parse() {
        Ok(id) => id,
        Err(_) => return Json(json!({ "ok": false, "error": "Invalid session state" })),
    };

    let user = match s.user_get_by_id(user_id) {
        Some(u) => u,
        None => return Json(json!({ "ok": false, "error": "User not found" })),
    };

    // Check account status
    if !user.is_active() {
        return Json(json!({ "ok": false, "error": "Account is suspended or locked" }));
    }
    if user.role == "subscriber" {
        return Json(json!({ "ok": false, "error": "No admin panel access" }));
    }

    let webauthn = match passkey::build_webauthn(s) {
        Ok(w) => w,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Parse the browser's assertion response
    let pub_cred: webauthn_rs::prelude::PublicKeyCredential =
        match serde_json::from_value(credential) {
            Ok(r) => r,
            Err(e) => {
                return Json(json!({ "ok": false, "error": format!("Invalid credential: {}", e) }))
            }
        };

    match webauthn.finish_passkey_authentication(&pub_cred, &auth_state) {
        Ok(auth_result) => {
            // Update sign counter for the used credential
            let cred_id_b64 = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                auth_result.cred_id().as_ref(),
            );
            let _ = s.passkey_update_sign_count(&cred_id_b64, auth_result.counter() as i64);

            // Also update the stored Passkey object with new counter
            if let Some(stored) = s.passkey_get_by_credential_id(&cred_id_b64) {
                if let Ok(mut pk) =
                    serde_json::from_str::<webauthn_rs::prelude::Passkey>(&stored.public_key)
                {
                    pk.update_credential(&auth_result);
                    if let Ok(updated_json) = serde_json::to_string(&pk) {
                        let _ = s.passkey_update_public_key(
                            &cred_id_b64,
                            &updated_json,
                            auth_result.counter() as i64,
                        );
                    }
                }
            }

            // Create session — passkey replaces both password + MFA
            let _ = s.user_touch_last_login(user.id);
            match auth::create_session(s, user.id, None, None) {
                Ok(session_id) => {
                    auth::set_session_cookie(cookies, &session_id);
                    s.audit_log(
                        Some(user.id),
                        Some(&user.display_name),
                        "login",
                        Some("user"),
                        Some(user.id),
                        Some(&user.email),
                        Some("Passkey authentication"),
                        Some(ip),
                    );
                    Json(json!({ "ok": true, "redirect": format!("/{}", admin_slug.0) }))
                }
                Err(_) => Json(json!({ "ok": false, "error": "Session creation failed" })),
            }
        }
        Err(e) => {
            s.audit_log(
                Some(user.id),
                Some(&user.display_name),
                "login_failed",
                Some("user"),
                Some(user.id),
                Some(&user.email),
                Some(&format!("Passkey auth failed: {}", e)),
                Some(ip),
            );
            Json(
                json!({ "ok": false, "error": "Passkey verification failed. Try again or use another method." }),
            )
        }
    }
}
