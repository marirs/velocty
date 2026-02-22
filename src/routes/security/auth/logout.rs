use rocket::response::Redirect;
use rocket::State;
use std::sync::Arc;

use crate::security::auth;
use crate::store::Store;
use crate::AdminSlug;

use super::login::needs_setup;

#[get("/logout")]
pub fn logout(
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
    cookies: &rocket::http::CookieJar<'_>,
) -> Redirect {
    if let Some(cookie) = cookies.get_private("velocty_session") {
        let _ = auth::destroy_session(&**store.inner(), cookie.value());
    }
    auth::clear_session_cookie(cookies);
    Redirect::to(format!("/{}/login", admin_slug.0))
}

/// Catch-all for any /<admin_slug>/* route that failed the AdminUser guard.
/// This fires when the guard returns Forward(Unauthorized).
#[get("/<_path..>", rank = 99)]
pub fn admin_redirect_to_login(
    _path: std::path::PathBuf,
    store: &State<Arc<dyn Store>>,
    admin_slug: &State<AdminSlug>,
) -> Redirect {
    if needs_setup(&**store.inner()) {
        Redirect::to(format!("/{}/setup", admin_slug.0))
    } else {
        Redirect::to(format!("/{}/login", admin_slug.0))
    }
}
