use std::sync::Arc;

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};

use super::save_upload;
use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Users Management ─────────────────────────────────────────

#[get("/users?<role>&<page>")]
pub fn users_list(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    role: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let settings = store.setting_all();
    let users = store.user_list_paginated(role.as_deref(), per_page, offset);
    let total = store.user_count_filtered(role.as_deref());
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;
    let users_json: Vec<serde_json::Value> = users.iter().map(|u| u.safe_json()).collect();

    let context = json!({
        "page_title": "Users",
        "admin_slug": slug.get(),
        "settings": settings,
        "users": users_json,
        "current_user": _admin.user.safe_json(),
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "role_filter": role,
        "count_all": store.user_count(),
        "count_admin": store.user_count_by_role("admin"),
        "count_editor": store.user_count_by_role("editor"),
        "count_author": store.user_count_by_role("author"),
        "count_subscriber": store.user_count_by_role("subscriber"),
    });
    Template::render("admin/users", &context)
}

#[derive(Deserialize)]
pub struct UserCreateForm {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub role: String,
}

#[post("/api/users/create", format = "json", data = "<form>")]
pub fn user_create(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserCreateForm>,
) -> Json<Value> {
    use crate::security::auth;

    let email = form.email.trim();
    let display_name = form.display_name.trim();
    let role = form.role.trim();

    if email.is_empty() || display_name.is_empty() {
        return Json(json!({"success": false, "error": "Email and display name are required"}));
    }
    if form.password.len() < 8 {
        return Json(json!({"success": false, "error": "Password must be at least 8 characters"}));
    }
    if !["admin", "editor", "author", "subscriber"].contains(&role) {
        return Json(json!({"success": false, "error": "Invalid role"}));
    }

    let hash = match auth::hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => return Json(json!({"success": false, "error": e})),
    };

    match store.user_create(email, &hash, display_name, role) {
        Ok(id) => {
            store.audit_log(
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "create",
                Some("user"),
                Some(id),
                Some(display_name),
                Some(role),
                None,
            );
            Json(json!({"success": true, "id": id}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[derive(Deserialize)]
pub struct UserUpdateForm {
    pub id: i64,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub password: Option<String>,
    pub avatar: Option<String>,
}

#[post("/api/users/update", format = "json", data = "<form>")]
pub fn user_update(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserUpdateForm>,
) -> Json<Value> {
    use crate::security::auth;

    let user = match store.user_get_by_id(form.id) {
        Some(u) => u,
        None => return Json(json!({"success": false, "error": "User not found"})),
    };

    // Update role if provided
    if let Some(ref role) = form.role {
        let role = role.trim();
        if !["admin", "editor", "author", "subscriber"].contains(&role) {
            return Json(json!({"success": false, "error": "Invalid role"}));
        }
        // Prevent demoting the last admin
        if user.role == "admin" && role != "admin" && store.user_count_by_role("admin") <= 1 {
            return Json(
                json!({"success": false, "error": "Cannot change role of the last admin"}),
            );
        }
        if let Err(e) = store.user_update_role(form.id, role) {
            return Json(json!({"success": false, "error": e}));
        }
    }

    // Update avatar if explicitly provided (empty string = remove)
    let avatar = match form.avatar {
        Some(ref a) => a.trim().to_string(),
        None => user.avatar.clone(),
    };
    if avatar != user.avatar {
        let _ = store.user_update_avatar(form.id, &avatar);
    }

    // Update profile fields if provided
    let email = form
        .email
        .as_deref()
        .unwrap_or(&user.email)
        .trim()
        .to_string();
    let display_name = form
        .display_name
        .as_deref()
        .unwrap_or(&user.display_name)
        .trim()
        .to_string();
    if let Err(e) = store.user_update_profile(form.id, &display_name, &email, &avatar) {
        return Json(json!({"success": false, "error": e}));
    }

    // Sync to settings if this is the current logged-in user
    if form.id == _admin.user.id {
        let _ = store.setting_set("admin_email", &email);
        let _ = store.setting_set("admin_display_name", &display_name);
        if avatar != user.avatar {
            let _ = store.setting_set("admin_avatar", &avatar);
        }
    }

    // Update password if provided
    if let Some(ref pw) = form.password {
        if !pw.is_empty() {
            if pw.len() < 8 {
                return Json(
                    json!({"success": false, "error": "Password must be at least 8 characters"}),
                );
            }
            let hash = match auth::hash_password(pw) {
                Ok(h) => h,
                Err(e) => return Json(json!({"success": false, "error": e})),
            };
            if let Err(e) = store.user_update_password(form.id, &hash) {
                return Json(json!({"success": false, "error": e}));
            }
        }
    }

    Json(json!({"success": true}))
}

#[derive(FromForm)]
pub struct AvatarUploadForm<'f> {
    pub user_id: i64,
    pub file: TempFile<'f>,
}

#[post("/api/users/avatar", data = "<form>")]
pub async fn user_avatar_upload(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    mut form: Form<AvatarUploadForm<'_>>,
) -> Json<Value> {
    let user = match store.user_get_by_id(form.user_id) {
        Some(u) => u,
        None => return Json(json!({"success": false, "error": "User not found"})),
    };

    match save_upload(&mut form.file, "avatar", &**store.inner()).await {
        Some(filename) => {
            let avatar_url = format!("/uploads/{}", filename);
            if let Err(e) = store.user_update_avatar(user.id, &avatar_url) {
                return Json(json!({"success": false, "error": e}));
            }
            // Sync to settings if this is the current user
            if user.id == _admin.user.id {
                let _ = store.setting_set("admin_avatar", &avatar_url);
            }
            Json(json!({"success": true, "avatar": avatar_url}))
        }
        None => Json(
            json!({"success": false, "error": "Upload failed. Ensure the file is a valid image."}),
        ),
    }
}

#[derive(Deserialize)]
pub struct UserActionForm {
    pub id: i64,
}

#[post("/api/users/lock", format = "json", data = "<form>")]
pub fn user_lock(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    if form.id == _admin.user.id {
        return Json(json!({"success": false, "error": "Cannot lock yourself"}));
    }
    if let Some(u) = store.user_get_by_id(form.id) {
        if u.role == "admin" && store.user_count_by_role("admin") <= 1 {
            return Json(json!({"success": false, "error": "Cannot lock the last admin"}));
        }
    }
    let target_name = store
        .user_get_by_id(form.id)
        .map(|u| u.display_name)
        .unwrap_or_default();
    match store.user_lock(form.id) {
        Ok(_) => {
            store.audit_log(
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "lock",
                Some("user"),
                Some(form.id),
                Some(&target_name),
                None,
                None,
            );
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[post("/api/users/unlock", format = "json", data = "<form>")]
pub fn user_unlock(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    let target_name = store
        .user_get_by_id(form.id)
        .map(|u| u.display_name)
        .unwrap_or_default();
    match store.user_unlock(form.id) {
        Ok(_) => {
            store.audit_log(
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "unlock",
                Some("user"),
                Some(form.id),
                Some(&target_name),
                None,
                None,
            );
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[post("/api/users/reset-password", format = "json", data = "<form>")]
pub fn user_reset_password(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    use crate::security::{auth, password_reset};

    let user = match store.user_get_by_id(form.id) {
        Some(u) => u,
        None => return Json(json!({"success": false, "error": "User not found"})),
    };

    let temp_pw = password_reset::generate_temp_password();
    let hash = match auth::hash_password(&temp_pw) {
        Ok(h) => h,
        Err(e) => return Json(json!({"success": false, "error": e})),
    };

    if let Err(e) = store.user_update_password(user.id, &hash) {
        return Json(json!({"success": false, "error": e}));
    }

    // Send the temp password via email in a background thread
    let store_clone = Arc::clone(store.inner());
    let email = user.email.clone();
    let pw = temp_pw.clone();
    std::thread::spawn(move || {
        if let Err(e) = password_reset::send_admin_reset_email(store_clone.as_ref(), &email, &pw) {
            log::error!(
                "Failed to send admin password reset email to {}: {}",
                email,
                e
            );
        }
    });

    Json(json!({"success": true, "message": "Password reset and emailed to user"}))
}

#[post("/api/users/delete", format = "json", data = "<form>")]
pub fn user_delete(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    if form.id == _admin.user.id {
        return Json(json!({"success": false, "error": "Cannot delete yourself"}));
    }
    if let Some(u) = store.user_get_by_id(form.id) {
        if u.role == "admin" && store.user_count_by_role("admin") <= 1 {
            return Json(json!({"success": false, "error": "Cannot delete the last admin"}));
        }
    }
    let target_name = store
        .user_get_by_id(form.id)
        .map(|u| u.display_name)
        .unwrap_or_default();
    match store.user_delete(form.id) {
        Ok(_) => {
            store.audit_log(
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "delete",
                Some("user"),
                Some(form.id),
                Some(&target_name),
                None,
                None,
            );
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

// ── MFA Setup / Disable (per-user) ──────────────────────

#[post("/mfa/setup", format = "json")]
pub fn mfa_setup(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let site_name = store.setting_get_or("site_name", "Velocty");

    let secret = crate::security::mfa::generate_secret();
    let qr = match crate::security::mfa::qr_data_uri(&secret, &site_name, &_admin.user.email) {
        Ok(uri) => uri,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Store the pending secret temporarily in settings keyed by user_id
    let pending_key = format!("mfa_pending_secret_{}", _admin.user.id);
    let _ = store.setting_set(&pending_key, &secret);

    Json(json!({ "ok": true, "qr": qr, "secret": secret }))
}

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaVerifyForm {
    pub code: String,
}

#[post("/mfa/verify", format = "json", data = "<body>")]
pub fn mfa_verify(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    let pending_key = format!("mfa_pending_secret_{}", _admin.user.id);
    let pending = store.setting_get_or(&pending_key, "");
    if pending.is_empty() {
        return Json(json!({ "ok": false, "error": "No pending MFA setup. Start setup first." }));
    }

    if !crate::security::mfa::verify_code(&pending, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. Please try again." }));
    }

    // Code verified — activate MFA on user record
    let recovery_codes = crate::security::mfa::generate_recovery_codes();
    let codes_json = serde_json::to_string(&recovery_codes).unwrap_or_else(|_| "[]".to_string());

    let _ = store.user_update_mfa(_admin.user.id, true, &pending, &codes_json);
    let _ = store.setting_set(&pending_key, "");

    // Keep settings in sync for backward compat
    let _ = store.setting_set("mfa_secret", &pending);
    let _ = store.setting_set("mfa_enabled", "true");
    let _ = store.setting_set("mfa_recovery_codes", &codes_json);

    Json(json!({ "ok": true, "recovery_codes": recovery_codes }))
}

#[post("/mfa/disable", format = "json", data = "<body>")]
pub fn mfa_disable(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    if !_admin.user.mfa_enabled || _admin.user.mfa_secret.is_empty() {
        return Json(json!({ "ok": false, "error": "MFA is not enabled." }));
    }

    // Verify current code before disabling
    if !crate::security::mfa::verify_code(&_admin.user.mfa_secret, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. MFA was not disabled." }));
    }

    let _ = store.user_update_mfa(_admin.user.id, false, "", "[]");

    // Keep settings in sync for backward compat
    let _ = store.setting_set("mfa_enabled", "false");
    let _ = store.setting_set("mfa_secret", "");
    let _ = store.setting_set("mfa_recovery_codes", "[]");

    Json(json!({ "ok": true }))
}

#[get("/mfa/recovery-codes")]
pub fn mfa_recovery_codes(_admin: AdminUser) -> Json<Value> {
    let codes: Vec<String> =
        serde_json::from_str(&_admin.user.mfa_recovery_codes).unwrap_or_default();
    Json(json!({ "ok": true, "codes": codes }))
}

// ── Passkey (WebAuthn) Management ───────────────────────

#[get("/passkeys")]
pub fn passkey_list(_admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let keys = store.passkey_list_for_user(_admin.user.id);
    let list: Vec<Value> = keys
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "created_at": k.created_at,
            })
        })
        .collect();
    Json(json!({ "ok": true, "passkeys": list }))
}

#[derive(Debug, Deserialize)]
pub struct PasskeyNameForm {
    pub name: Option<String>,
}

#[post("/passkeys/register/start", format = "json", data = "<body>")]
pub fn passkey_register_start(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<PasskeyNameForm>,
) -> Json<Value> {
    use crate::security::passkey;
    let s: &dyn Store = &**store.inner();

    let webauthn = match passkey::build_webauthn(s) {
        Ok(w) => w,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Deterministic user UUID from email for WebAuthn user handle
    let user_id = uuid::Uuid::new_v4();
    let existing = passkey::load_credentials(s, _admin.user.id);
    let exclude: Vec<webauthn_rs::prelude::CredentialID> =
        existing.iter().map(|pk| pk.cred_id().clone()).collect();

    match webauthn.start_passkey_registration(
        user_id,
        &_admin.user.email,
        &_admin.user.display_name,
        Some(exclude),
    ) {
        Ok((ccr, reg_state)) => {
            passkey::store_reg_state(s, _admin.user.id, &reg_state);
            // Store the desired name for use in finish
            let name = body.name.as_deref().unwrap_or("Passkey").to_string();
            let name_key = format!("passkey_reg_name_{}", _admin.user.id);
            let _ = store.setting_set(&name_key, &name);
            Json(json!({ "ok": true, "options": ccr }))
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("{}", e) })),
    }
}

#[post("/passkeys/register/finish", format = "json", data = "<body>")]
pub fn passkey_register_finish(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<Value>,
) -> Json<Value> {
    use crate::security::passkey;
    let s: &dyn Store = &**store.inner();

    let webauthn = match passkey::build_webauthn(s) {
        Ok(w) => w,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    let reg_state = match passkey::take_reg_state(s, _admin.user.id) {
        Some(st) => st,
        None => {
            return Json(
                json!({ "ok": false, "error": "No pending registration. Start registration first." }),
            )
        }
    };

    // Parse the browser's credential response
    let reg: webauthn_rs::prelude::RegisterPublicKeyCredential =
        match serde_json::from_value(body.into_inner()) {
            Ok(r) => r,
            Err(e) => {
                return Json(json!({ "ok": false, "error": format!("Invalid credential: {}", e) }))
            }
        };

    match webauthn.finish_passkey_registration(&reg, &reg_state) {
        Ok(passkey_data) => {
            let cred_id = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                passkey_data.cred_id().as_ref(),
            );
            let public_key_json =
                serde_json::to_string(&passkey_data).unwrap_or_else(|_| "{}".to_string());

            let name_key = format!("passkey_reg_name_{}", _admin.user.id);
            let name = store.setting_get_or(&name_key, "Passkey");
            let _ = store.setting_set(&name_key, "");

            match store.passkey_create(_admin.user.id, &cred_id, &public_key_json, 0, "[]", &name) {
                Ok(_) => {
                    // Auto-enable passkey as auth method on first registration
                    let count = store.passkey_count_for_user(_admin.user.id);
                    if count == 1 {
                        // First passkey — save current method as fallback, switch to passkey
                        let current = &_admin.user.auth_method;
                        let fallback = if current == "passkey" {
                            &_admin.user.auth_method_fallback
                        } else {
                            current
                        };
                        let _ = store.user_update_auth_method(_admin.user.id, "passkey", fallback);
                    }
                    Json(json!({ "ok": true }))
                }
                Err(e) => Json(json!({ "ok": false, "error": format!("Failed to save: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "ok": false, "error": format!("Registration failed: {}", e) })),
    }
}

#[derive(Debug, Deserialize)]
pub struct PasskeyDeleteForm {
    pub id: i64,
}

#[post("/passkeys/delete", format = "json", data = "<body>")]
pub fn passkey_delete(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    body: Json<PasskeyDeleteForm>,
) -> Json<Value> {
    match store.passkey_delete(body.id, _admin.user.id) {
        Ok(()) => {
            // If no passkeys remain, auto-revert to fallback method
            let remaining = store.passkey_count_for_user(_admin.user.id);
            if remaining == 0 && _admin.user.auth_method == "passkey" {
                let fallback = &_admin.user.auth_method_fallback;
                let _ = store.user_update_auth_method(_admin.user.id, fallback, fallback);
            }
            Json(json!({ "ok": true, "remaining": remaining }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}
