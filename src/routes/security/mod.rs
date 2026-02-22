pub mod auth;

use rocket::http::Header;
use rocket::response::{self, Responder};
use rocket::Request;
use rocket_dyn_templates::Template;

/// Wrapper that adds no-cache headers to a Template response
pub struct NoCacheTemplate(pub Template);

impl<'r> Responder<'r, 'static> for NoCacheTemplate {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let mut resp = self.0.respond_to(req)?;
        resp.set_header(Header::new(
            "Cache-Control",
            "no-store, no-cache, must-revalidate, max-age=0",
        ));
        resp.set_header(Header::new("Pragma", "no-cache"));
        Ok(resp)
    }
}

pub fn routes() -> Vec<rocket::Route> {
    auth::routes()
}

/// Minimal routes for setup-only mode (no DB configured yet).
/// Only mounts the setup wizard page, submit handler, and mongo test.
pub fn setup_only_routes() -> Vec<rocket::Route> {
    auth::setup_only_routes()
}
