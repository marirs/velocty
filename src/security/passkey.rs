use std::sync::Arc;

use webauthn_rs::prelude::*;
use webauthn_rs::Webauthn;
use webauthn_rs::WebauthnBuilder;

use crate::store::Store;

/// Build a Webauthn instance from the site's settings.
/// RP ID = domain extracted from site_url, Origin = site_url.
pub fn build_webauthn(store: &dyn Store) -> Result<Arc<Webauthn>, String> {
    let site_url = store.setting_get_or("site_url", "http://localhost:8000");
    let url = url::Url::parse(&site_url).map_err(|e| format!("Invalid site_url: {}", e))?;

    let host = url.host_str().ok_or("No host in site_url")?.to_string();

    // For localhost / 127.0.0.1 dev environments, normalise origin to http://localhost:<port>
    // so the RP ID is always "localhost" (a proper domain the WebAuthn spec allows).
    let (rp_id, rp_origin) = if host == "localhost" || host == "127.0.0.1" {
        let port = url.port().unwrap_or(8000);
        let origin =
            url::Url::parse(&format!("http://localhost:{}", port)).map_err(|e| e.to_string())?;
        ("localhost".to_string(), origin)
    } else {
        (host, url)
    };

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin)
        .map_err(|e| format!("WebauthnBuilder error: {}", e))?
        .rp_name("Velocty");

    let webauthn = builder
        .build()
        .map_err(|e| format!("Webauthn build error: {}", e))?;

    Ok(Arc::new(webauthn))
}

/// Load existing credentials for a user (for exclusion during registration
/// and for authentication).
pub fn load_credentials(store: &dyn Store, user_id: i64) -> Vec<Passkey> {
    let rows = store.passkey_list_for_user(user_id);
    rows.iter()
        .filter_map(|pk| {
            let cred: Passkey = serde_json::from_str(&pk.public_key).ok()?;
            Some(cred)
        })
        .collect()
}

/// Store a registration challenge in settings (keyed by user_id).
pub fn store_reg_state(store: &dyn Store, user_id: i64, state: &PasskeyRegistration) {
    let key = format!("passkey_reg_state_{}", user_id);
    if let Ok(json) = serde_json::to_string(state) {
        let _ = store.setting_set(&key, &json);
    }
}

/// Retrieve and clear a registration challenge.
pub fn take_reg_state(store: &dyn Store, user_id: i64) -> Option<PasskeyRegistration> {
    let key = format!("passkey_reg_state_{}", user_id);
    let json = store.setting_get(&key)?;
    let _ = store.setting_set(&key, "");
    if json.is_empty() {
        return None;
    }
    serde_json::from_str(&json).ok()
}

/// Store an authentication challenge in settings (keyed by pending token).
pub fn store_auth_state(store: &dyn Store, token: &str, state: &PasskeyAuthentication) {
    let key = format!("passkey_auth_state_{}", token);
    if let Ok(json) = serde_json::to_string(state) {
        let _ = store.setting_set(&key, &json);
    }
}

/// Retrieve and clear an authentication challenge.
pub fn take_auth_state(store: &dyn Store, token: &str) -> Option<PasskeyAuthentication> {
    let key = format!("passkey_auth_state_{}", token);
    let json = store.setting_get(&key)?;
    let _ = store.setting_set(&key, "");
    if json.is_empty() {
        return None;
    }
    serde_json::from_str(&json).ok()
}
