use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::security::auth::AdminUser;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::settings::Setting;
use crate::AdminSlug;
use super::save_upload;

// ── Users Management ─────────────────────────────────────────

#[get("/users?<role>&<page>")]
pub fn users_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    role: Option<String>,
    page: Option<i64>,
) -> Template {
    use crate::models::user::User;

    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let settings = Setting::all(pool);
    let users = User::list_paginated(pool, role.as_deref(), per_page, offset);
    let total = User::count_filtered(pool, role.as_deref());
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;
    let users_json: Vec<serde_json::Value> = users.iter().map(|u| u.safe_json()).collect();

    let context = json!({
        "page_title": "Users",
        "admin_slug": slug.0,
        "settings": settings,
        "users": users_json,
        "current_user": _admin.user.safe_json(),
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "role_filter": role,
        "count_all": User::count(pool),
        "count_admin": User::count_by_role(pool, "admin"),
        "count_editor": User::count_by_role(pool, "editor"),
        "count_author": User::count_by_role(pool, "author"),
        "count_subscriber": User::count_by_role(pool, "subscriber"),
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
    pool: &State<DbPool>,
    form: Json<UserCreateForm>,
) -> Json<Value> {
    use crate::models::user::User;
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

    match User::create(pool, email, &hash, display_name, role) {
        Ok(id) => {
            AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "create", Some("user"), Some(id), Some(display_name), Some(role), None);
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
    pool: &State<DbPool>,
    form: Json<UserUpdateForm>,
) -> Json<Value> {
    use crate::models::user::User;
    use crate::security::auth;

    let user = match User::get_by_id(pool, form.id) {
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
        if user.role == "admin" && role != "admin" && User::count_by_role(pool, "admin") <= 1 {
            return Json(json!({"success": false, "error": "Cannot change role of the last admin"}));
        }
        if let Err(e) = User::update_role(pool, form.id, role) {
            return Json(json!({"success": false, "error": e}));
        }
    }

    // Update avatar if explicitly provided (empty string = remove)
    let avatar = match form.avatar {
        Some(ref a) => a.trim().to_string(),
        None => user.avatar.clone(),
    };
    if avatar != user.avatar {
        let _ = User::update_avatar(pool, form.id, &avatar);
    }

    // Update profile fields if provided
    let email = form.email.as_deref().unwrap_or(&user.email).trim().to_string();
    let display_name = form.display_name.as_deref().unwrap_or(&user.display_name).trim().to_string();
    if let Err(e) = User::update_profile(pool, form.id, &display_name, &email, &avatar) {
        return Json(json!({"success": false, "error": e}));
    }

    // Sync to settings if this is the current logged-in user
    if form.id == _admin.user.id {
        let _ = Setting::set(pool, "admin_email", &email);
        let _ = Setting::set(pool, "admin_display_name", &display_name);
        if avatar != user.avatar {
            let _ = Setting::set(pool, "admin_avatar", &avatar);
        }
    }

    // Update password if provided
    if let Some(ref pw) = form.password {
        if !pw.is_empty() {
            if pw.len() < 8 {
                return Json(json!({"success": false, "error": "Password must be at least 8 characters"}));
            }
            let hash = match auth::hash_password(pw) {
                Ok(h) => h,
                Err(e) => return Json(json!({"success": false, "error": e})),
            };
            if let Err(e) = User::update_password(pool, form.id, &hash) {
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
    pool: &State<DbPool>,
    mut form: Form<AvatarUploadForm<'_>>,
) -> Json<Value> {
    use crate::models::user::User;

    let user = match User::get_by_id(pool, form.user_id) {
        Some(u) => u,
        None => return Json(json!({"success": false, "error": "User not found"})),
    };

    match save_upload(&mut form.file, "avatar").await {
        Some(filename) => {
            let avatar_url = format!("/uploads/{}", filename);
            if let Err(e) = User::update_avatar(pool, user.id, &avatar_url) {
                return Json(json!({"success": false, "error": e}));
            }
            // Sync to settings if this is the current user
            if user.id == _admin.user.id {
                let _ = Setting::set(pool, "admin_avatar", &avatar_url);
            }
            Json(json!({"success": true, "avatar": avatar_url}))
        }
        None => Json(json!({"success": false, "error": "Upload failed. Ensure the file is a valid image."})),
    }
}

#[derive(Deserialize)]
pub struct UserActionForm {
    pub id: i64,
}

#[post("/api/users/lock", format = "json", data = "<form>")]
pub fn user_lock(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    use crate::models::user::User;

    if form.id == _admin.user.id {
        return Json(json!({"success": false, "error": "Cannot lock yourself"}));
    }
    if let Some(u) = User::get_by_id(pool, form.id) {
        if u.role == "admin" && User::count_by_role(pool, "admin") <= 1 {
            return Json(json!({"success": false, "error": "Cannot lock the last admin"}));
        }
    }
    let target_name = User::get_by_id(pool, form.id).map(|u| u.display_name).unwrap_or_default();
    match User::lock(pool, form.id) {
        Ok(_) => {
            AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "lock", Some("user"), Some(form.id), Some(&target_name), None, None);
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[post("/api/users/unlock", format = "json", data = "<form>")]
pub fn user_unlock(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    use crate::models::user::User;

    let target_name = User::get_by_id(pool, form.id).map(|u| u.display_name).unwrap_or_default();
    match User::unlock(pool, form.id) {
        Ok(_) => {
            AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "unlock", Some("user"), Some(form.id), Some(&target_name), None, None);
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

#[post("/api/users/reset-password", format = "json", data = "<form>")]
pub fn user_reset_password(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    use crate::models::user::User;
    use crate::security::{auth, password_reset};

    let user = match User::get_by_id(pool, form.id) {
        Some(u) => u,
        None => return Json(json!({"success": false, "error": "User not found"})),
    };

    let temp_pw = password_reset::generate_temp_password();
    let hash = match auth::hash_password(&temp_pw) {
        Ok(h) => h,
        Err(e) => return Json(json!({"success": false, "error": e})),
    };

    if let Err(e) = User::update_password(pool, user.id, &hash) {
        return Json(json!({"success": false, "error": e}));
    }

    // Send the temp password via email in a background thread
    let pool_clone = pool.inner().clone();
    let email = user.email.clone();
    let pw = temp_pw.clone();
    std::thread::spawn(move || {
        if let Err(e) = password_reset::send_admin_reset_email(&pool_clone, &email, &pw) {
            log::error!("Failed to send admin password reset email to {}: {}", email, e);
        }
    });

    Json(json!({"success": true, "message": "Password reset and emailed to user"}))
}

#[post("/api/users/delete", format = "json", data = "<form>")]
pub fn user_delete(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Json<UserActionForm>,
) -> Json<Value> {
    use crate::models::user::User;

    if form.id == _admin.user.id {
        return Json(json!({"success": false, "error": "Cannot delete yourself"}));
    }
    if let Some(u) = User::get_by_id(pool, form.id) {
        if u.role == "admin" && User::count_by_role(pool, "admin") <= 1 {
            return Json(json!({"success": false, "error": "Cannot delete the last admin"}));
        }
    }
    let target_name = User::get_by_id(pool, form.id).map(|u| u.display_name).unwrap_or_default();
    match User::delete(pool, form.id) {
        Ok(_) => {
            AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "delete", Some("user"), Some(form.id), Some(&target_name), None, None);
            Json(json!({"success": true}))
        }
        Err(e) => Json(json!({"success": false, "error": e})),
    }
}

// ── MFA Setup / Disable (per-user) ──────────────────────

#[post("/mfa/setup", format = "json")]
pub fn mfa_setup(
    _admin: AdminUser,
    pool: &State<DbPool>,
) -> Json<Value> {
    use crate::models::user::User;

    let site_name = Setting::get_or(pool, "site_name", "Velocty");

    let secret = crate::security::mfa::generate_secret();
    let qr = match crate::security::mfa::qr_data_uri(&secret, &site_name, &_admin.user.email) {
        Ok(uri) => uri,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Store the pending secret temporarily in settings keyed by user_id
    let pending_key = format!("mfa_pending_secret_{}", _admin.user.id);
    let _ = Setting::set(pool, &pending_key, &secret);

    Json(json!({ "ok": true, "qr": qr, "secret": secret }))
}

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaVerifyForm {
    pub code: String,
}

#[post("/mfa/verify", format = "json", data = "<body>")]
pub fn mfa_verify(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    use crate::models::user::User;

    let pending_key = format!("mfa_pending_secret_{}", _admin.user.id);
    let pending = Setting::get_or(pool, &pending_key, "");
    if pending.is_empty() {
        return Json(json!({ "ok": false, "error": "No pending MFA setup. Start setup first." }));
    }

    if !crate::security::mfa::verify_code(&pending, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. Please try again." }));
    }

    // Code verified — activate MFA on user record
    let recovery_codes = crate::security::mfa::generate_recovery_codes();
    let codes_json = serde_json::to_string(&recovery_codes).unwrap_or_else(|_| "[]".to_string());

    let _ = User::update_mfa(pool, _admin.user.id, true, &pending, &codes_json);
    let _ = Setting::set(pool, &pending_key, "");

    // Keep settings in sync for backward compat
    let _ = Setting::set(pool, "mfa_secret", &pending);
    let _ = Setting::set(pool, "mfa_enabled", "true");
    let _ = Setting::set(pool, "mfa_recovery_codes", &codes_json);

    Json(json!({ "ok": true, "recovery_codes": recovery_codes }))
}

#[post("/mfa/disable", format = "json", data = "<body>")]
pub fn mfa_disable(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    use crate::models::user::User;

    if !_admin.user.mfa_enabled || _admin.user.mfa_secret.is_empty() {
        return Json(json!({ "ok": false, "error": "MFA is not enabled." }));
    }

    // Verify current code before disabling
    if !crate::security::mfa::verify_code(&_admin.user.mfa_secret, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. MFA was not disabled." }));
    }

    let _ = User::update_mfa(pool, _admin.user.id, false, "", "[]");

    // Keep settings in sync for backward compat
    let _ = Setting::set(pool, "mfa_enabled", "false");
    let _ = Setting::set(pool, "mfa_secret", "");
    let _ = Setting::set(pool, "mfa_recovery_codes", "[]");

    Json(json!({ "ok": true }))
}

#[get("/mfa/recovery-codes")]
pub fn mfa_recovery_codes(
    _admin: AdminUser,
) -> Json<Value> {
    let codes: Vec<String> = serde_json::from_str(&_admin.user.mfa_recovery_codes).unwrap_or_default();
    Json(json!({ "ok": true, "codes": codes }))
}
