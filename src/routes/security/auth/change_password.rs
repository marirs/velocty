use rocket::form::Form;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use std::collections::HashMap;
use std::sync::Arc;

use crate::security::auth::{self, AuthorUser};
use crate::store::Store;
use crate::AdminSlug;

#[derive(Debug, FromForm)]
pub struct ChangePasswordForm {
    pub new_password: String,
    pub confirm_password: String,
}

#[get("/change-password")]
pub fn change_password_page(
    user: AuthorUser,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
) -> Result<Template, Redirect> {
    let s: &dyn Store = &**store.inner();
    let u = match s.user_get_by_id(user.user.id) {
        Some(u) => u,
        None => return Err(Redirect::to(format!("/{}/login", admin_slug.get()))),
    };

    // Only show this page if force_password_change is set
    if !u.force_password_change {
        return Err(Redirect::to(format!("/{}", admin_slug.get())));
    }

    let mut ctx = HashMap::new();
    ctx.insert(
        "admin_theme".to_string(),
        s.setting_get_or("admin_theme", "dark"),
    );
    ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
    Ok(Template::render("admin/change_password", &ctx))
}

#[post("/change-password", data = "<form>")]
pub fn change_password_submit(
    user: AuthorUser,
    form: Form<ChangePasswordForm>,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
) -> Result<Redirect, Template> {
    let s: &dyn Store = &**store.inner();
    let theme = s.setting_get_or("admin_theme", "dark");

    let make_err = |msg: &str| -> Template {
        let mut ctx = HashMap::new();
        ctx.insert("error".to_string(), msg.to_string());
        ctx.insert("admin_theme".to_string(), theme.clone());
        ctx.insert("admin_slug".to_string(), admin_slug.get().clone());
        Template::render("admin/change_password", &ctx)
    };

    if form.new_password.len() < 8 {
        return Err(make_err("Password must be at least 8 characters."));
    }
    if form.new_password != form.confirm_password {
        return Err(make_err("Passwords do not match."));
    }

    let hash = match auth::hash_password(&form.new_password) {
        Ok(h) => h,
        Err(_) => return Err(make_err("Failed to hash password.")),
    };

    let _ = s.user_update_password(user.user.id, &hash);
    let _ = s.user_set_force_password_change(user.user.id, false);

    s.audit_log(
        Some(user.user.id),
        Some(&user.user.display_name),
        "password_changed",
        Some("user"),
        Some(user.user.id),
        Some(&user.user.email),
        Some("Forced password change completed"),
        None,
    );

    // Redirect to dashboard
    Ok(Redirect::to(format!("/{}", admin_slug.get())))
}
