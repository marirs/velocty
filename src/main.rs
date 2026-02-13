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
mod models;
mod routes;

use rocket::response::content::RawHtml;

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

    rocket::build()
        .manage(pool)
        .attach(Template::fairing())
        .attach(analytics::AnalyticsFairing)
        .mount("/static", FileServer::from("website/static"))
        .mount("/uploads", FileServer::from("website/uploads"))
        .mount(
            "/",
            routes::public::routes(),
        )
        .mount(
            "/admin",
            routes::admin::routes(),
        )
        .mount(
            "/admin/api",
            routes::admin_api::routes(),
        )
        .mount(
            "/api",
            routes::api::routes(),
        )
        .mount(
            "/",
            routes::auth::routes(),
        )
        .register("/", catchers![not_found, server_error])
}
