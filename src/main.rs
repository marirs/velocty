#[macro_use]
extern crate rocket;

use rocket::fs::FileServer;
use rocket_dyn_templates::Template;

mod ai;
mod analytics;
mod boot;
mod db;
mod health;
mod images;
mod rate_limit;
mod email;
mod render;
mod security;
mod rss;
mod seo;
mod typography;

mod import;
mod license;
mod models;
mod routes;
mod tasks;

#[cfg(feature = "multi-site")]
mod site;

#[cfg(test)]
mod tests;

use rocket::response::content::RawHtml;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;

use models::settings::{Setting, SettingsCache};

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
fn not_found(req: &rocket::Request<'_>) -> RawHtml<String> {
    if let Some(pool) = req.rocket().state::<db::DbPool>() {
        let settings = Setting::all(pool);
        let context = serde_json::json!({
            "settings": settings,
            "page_type": "404",
            "seo": "<title>404 — Page Not Found</title>",
        });
        return RawHtml(render::render_page(pool, "404", &context));
    }
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
    health::init_uptime();

    let pool = db::init_pool().expect("Failed to initialize database pool");
    db::run_migrations(&pool).expect("Failed to run database migrations");
    db::seed_defaults(&pool).expect("Failed to seed default settings");

    let admin_slug = Setting::get_or(&pool, "admin_slug", "admin");
    let admin_mount = format!("/{}", admin_slug);
    let admin_api_mount = format!("/{}/api", admin_slug);

    let settings_cache = SettingsCache::load(&pool);

    eprintln!("Admin panel mounted at: {}", admin_mount);
    eprintln!("Dynamic routing enabled — blog/portfolio slugs and enabled flags read from cache at request time");

    let mut rocket = rocket::build()
        .manage(pool)
        .manage(AdminSlug(admin_slug))
        .manage(settings_cache)
        .manage(rate_limit::RateLimiter::new())
        .manage(security::firewall::FwRateLimiter::new())
        .attach(Template::fairing())
        .attach(security::firewall::FirewallFairing)
        .attach(analytics::AnalyticsFairing)
        .attach(NoCacheAdmin)
        .attach(tasks::BackgroundTasks)
        .mount("/static", FileServer::from("website/static"))
        .mount("/uploads", FileServer::from("website/site/uploads"))
        .mount(
            "/",
            routes::public::root_routes(),
        )
        .mount(
            &admin_mount,
            routes::admin::routes(),
        )
        .mount(
            &admin_api_mount,
            routes::admin::api_routes(),
        )
        .mount(
            &admin_api_mount,
            routes::ai::routes(),
        )
        .mount(
            "/api",
            routes::api::routes(),
        )
        .mount(
            "/",
            routes::commerce::routes(),
        )
        .mount(
            &admin_mount,
            routes::security::routes(),
        )
        .register("/", catchers![not_found, server_error]);

    // Multi-site: initialize registry and mount super admin routes
    #[cfg(feature = "multi-site")]
    {
        let registry = site::init_registry().expect("Failed to initialize site registry");
        site::run_registry_migrations(&registry).expect("Failed to run registry migrations");

        // Auto-migrate single-site data into multi-site if website/site/ exists
        if let Err(e) = site::migrate_single_to_multi(&registry, "localhost", "My Site") {
            eprintln!("Warning: single→multi migration failed: {}", e);
        }

        eprintln!("Multi-site mode enabled. Super admin at: /super/");
        rocket = rocket
            .manage(registry)
            .manage(site::SitePoolManager::new())
            .attach(site::SiteResolver)
            .mount("/super", routes::super_admin::routes());
    }

    rocket
}
