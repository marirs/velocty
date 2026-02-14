#[macro_use]
extern crate rocket;

use rocket::fs::FileServer;
use rocket_dyn_templates::Template;

mod analytics;
mod auth;
mod boot;
mod db;
mod images;
mod render;
mod rss;
mod seo;

mod import;
mod license;
mod models;
mod routes;

use rocket::response::content::RawHtml;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;

use models::settings::Setting;

/// Holds the admin URL slug, read from DB at startup.
/// Shared via Rocket managed state so routes, fairings, and templates can access it.
pub struct AdminSlug(pub String);

pub struct NoCacheAdmin;

#[rocket::async_trait]
impl Fairing for NoCacheAdmin {
    fn info(&self) -> Info {
        Info { name: "No-Cache Admin Pages", kind: Kind::Response }
    }

    async fn on_response<'r>(&self, req: &'r rocket::Request<'_>, res: &mut rocket::Response<'r>) {
        let slug = req.rocket().state::<AdminSlug>()
            .map(|s| s.0.as_str())
            .unwrap_or("admin");
        let prefix = format!("/{}", slug);
        if req.uri().path().starts_with(&*prefix) {
            res.set_header(Header::new("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0"));
            res.set_header(Header::new("Pragma", "no-cache"));
        }
    }
}

#[catch(404)]
fn not_found() -> RawHtml<String> {
    RawHtml("<html><body style='font-family:sans-serif;text-align:center;padding:80px'><h1>404</h1><p>Page not found.</p><a href='/'>← Home</a></body></html>".to_string())
}

#[catch(500)]
fn server_error() -> RawHtml<String> {
    RawHtml("<html><body style='font-family:sans-serif;text-align:center;padding:80px'><h1>500</h1><p>Internal server error.</p><a href='/'>← Home</a></body></html>".to_string())
}

#[launch]
fn rocket() -> _ {
    env_logger::init();

    // Boot check — verify/create directories, validate critical files
    boot::run();

    let pool = db::init_pool().expect("Failed to initialize database pool");
    db::run_migrations(&pool).expect("Failed to run database migrations");
    db::seed_defaults(&pool).expect("Failed to seed default settings");

    let admin_slug = Setting::get_or(&pool, "admin_slug", "admin");
    let admin_mount = format!("/{}", admin_slug);
    let admin_api_mount = format!("/{}/api", admin_slug);

    eprintln!("Admin panel mounted at: {}", admin_mount);

    rocket::build()
        .manage(pool)
        .manage(AdminSlug(admin_slug))
        .attach(Template::fairing())
        .attach(analytics::AnalyticsFairing)
        .attach(NoCacheAdmin)
        .mount("/static", FileServer::from("website/static"))
        .mount("/uploads", FileServer::from("website/uploads"))
        .mount(
            "/",
            routes::public::routes(),
        )
        .mount(
            &admin_mount,
            routes::admin::routes(),
        )
        .mount(
            &admin_api_mount,
            routes::admin_api::routes(),
        )
        .mount(
            "/api",
            routes::api::routes(),
        )
        .mount(
            &admin_mount,
            routes::auth::routes(),
        )
        .register("/", catchers![not_found, server_error])
}
