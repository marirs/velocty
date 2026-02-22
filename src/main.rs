#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_large_err)]
#![allow(deprecated)]

#[macro_use]
extern crate rocket;

use rocket::fs::FileServer;
use rocket_dyn_templates::Template;

mod ai;
mod analytics;
mod boot;
mod db;
mod designs;
mod email;
mod health;
mod image_proxy;
mod images;
mod mta;
mod rate_limit;
mod render;
mod rss;
mod security;
mod seo;
mod svg_sanitizer;
mod typography;

mod import;
mod license;
mod models;
mod routes;
mod store;
mod tasks;

#[cfg(feature = "multi-site")]
mod site;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::response::content::RawHtml;

use models::settings::SettingsCache;
use store::Store;

/// Holds the admin URL slug, read from DB at startup.
/// Shared via Rocket managed state so routes, fairings, and templates can access it.
pub struct AdminSlug(pub String);

/// Marker: true when running in setup-only mode (no DB yet).
pub struct SetupMode(pub bool);

pub struct NoCacheAdmin;

#[rocket::async_trait]
impl Fairing for NoCacheAdmin {
    fn info(&self) -> Info {
        Info {
            name: "No-Cache Admin Pages",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, req: &'r rocket::Request<'_>, res: &mut rocket::Response<'r>) {
        let slug = req
            .rocket()
            .state::<AdminSlug>()
            .map(|s| s.0.as_str())
            .unwrap_or("admin");
        let prefix = format!("/{}", slug);
        if req.uri().path().starts_with(&*prefix) {
            res.set_header(Header::new(
                "Cache-Control",
                "no-store, no-cache, must-revalidate, max-age=0",
            ));
            res.set_header(Header::new("Pragma", "no-cache"));
        }
    }
}

#[catch(404)]
fn not_found(req: &rocket::Request<'_>) -> RawHtml<String> {
    // If in setup mode, redirect everything to /admin/setup
    if let Some(setup) = req.rocket().state::<SetupMode>() {
        if setup.0 {
            let slug = req
                .rocket()
                .state::<AdminSlug>()
                .map(|s| s.0.as_str())
                .unwrap_or("admin");
            let setup_url = format!("/{}/setup", slug);
            return RawHtml(format!(
                "<html><head><meta http-equiv=\"refresh\" content=\"0;url={}\"></head><body></body></html>",
                setup_url
            ));
        }
    }

    // If the 404 is for an admin path (auth guard forwarded), redirect to login
    let slug = req
        .rocket()
        .state::<AdminSlug>()
        .map(|s| s.0.as_str())
        .unwrap_or("admin");
    let admin_prefix = format!("/{}/", slug);
    let path = req.uri().path().as_str();
    if (path == format!("/{}", slug) || path.starts_with(&admin_prefix))
        && !path.ends_with("/login")
        && !path.ends_with("/setup")
    {
        let login_url = format!("/{}/login", slug);
        return RawHtml(format!(
                "<html><head><meta http-equiv=\"refresh\" content=\"0;url={}\"></head><body></body></html>",
                login_url
            ));
    }

    if let Some(store) = req.rocket().state::<Arc<dyn Store>>() {
        let s: &dyn Store = &**store;
        let settings = s.setting_all();
        let nav_cats = s.category_list_nav_visible(Some("portfolio"));
        let nav_journal_cats = s.category_list_nav_visible(Some("post"));
        let context = serde_json::json!({
            "settings": settings,
            "nav_categories": nav_cats,
            "nav_journal_categories": nav_journal_cats,
            "page_type": "404",
            "seo": "<title>404 — Page Not Found</title>",
        });
        return RawHtml(render::render_page(s, "404", &context));
    }
    RawHtml("<html><body style='font-family:sans-serif;text-align:center;padding:80px'><h1>404</h1><p>Page not found.</p><a href='/'>← Home</a></body></html>".to_string())
}

#[catch(500)]
fn server_error() -> RawHtml<String> {
    RawHtml("<html><body style='font-family:sans-serif;text-align:center;padding:80px'><h1>500</h1><p>Internal server error.</p><a href='/'>← Home</a></body></html>".to_string())
}

/// Read velocty.toml and return the backend string ("sqlite", "mongodb", or empty if missing).
fn read_config_backend() -> String {
    health::read_db_backend()
}

/// Instantiate the correct Store based on velocty.toml config.
/// Returns None if velocty.toml doesn't exist (first boot → setup mode).
fn create_store() -> Option<Arc<dyn Store>> {
    if !std::path::Path::new("velocty.toml").exists() {
        return None;
    }

    let backend = read_config_backend();
    match backend.as_str() {
        "mongodb" => {
            let toml_str = std::fs::read_to_string("velocty.toml").unwrap_or_default();
            let toml_val: toml::Value = toml_str
                .parse()
                .unwrap_or(toml::Value::Table(Default::default()));
            let uri = toml_val
                .get("database")
                .and_then(|d| d.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("mongodb://localhost:27017");
            let db_name = toml_val
                .get("database")
                .and_then(|d| d.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("velocty");
            eprintln!("Connecting to MongoDB at {} (db: {})", uri, db_name);
            match store::mongo::MongoStore::new(uri, db_name) {
                Ok(ms) => Some(Arc::new(ms) as Arc<dyn Store>),
                Err(e) => {
                    eprintln!("ERROR: Failed to connect to MongoDB: {}", e);
                    eprintln!("Falling back to SQLite...");
                    let pool = db::init_pool().expect("Failed to initialize SQLite pool");
                    Some(Arc::new(store::sqlite::SqliteStore::new(pool)) as Arc<dyn Store>)
                }
            }
        }
        _ => {
            // Default: SQLite
            let pool = db::init_pool().expect("Failed to initialize SQLite pool");
            Some(Arc::new(store::sqlite::SqliteStore::new(pool)) as Arc<dyn Store>)
        }
    }
}

#[launch]
fn rocket() -> _ {
    env_logger::init();

    // Boot check — verify/create directories, validate critical files
    boot::run();
    health::init_uptime();

    let maybe_store = create_store();

    if let Some(store) = maybe_store {
        // ── Full server mode: DB is configured ──

        // Run migrations and seed defaults
        store
            .run_migrations()
            .expect("Failed to run database migrations");
        store
            .seed_defaults()
            .expect("Failed to seed default settings");

        let backend = read_config_backend();

        let admin_slug = store.setting_get_or("admin_slug", "admin");
        let admin_mount = format!("/{}", admin_slug);
        let admin_api_mount = format!("/{}/api", admin_slug);

        let settings_cache = SettingsCache::load_from_store(&*store);

        eprintln!("Database backend: {}", backend);
        eprintln!("Admin panel mounted at: {}", admin_mount);
        eprintln!("Dynamic routing enabled — blog/portfolio slugs and enabled flags read from cache at request time");

        #[allow(unused_mut)]
        let mut rocket = rocket::build()
            .manage(store)
            .manage(AdminSlug(admin_slug))
            .manage(SetupMode(false))
            .manage(settings_cache)
            .manage(rate_limit::RateLimiter::new())
            .manage(security::firewall::FwRateLimiter::new())
            .attach(Template::fairing())
            .attach(security::firewall::FirewallFairing)
            .attach(analytics::AnalyticsFairing)
            .attach(NoCacheAdmin)
            .attach(tasks::BackgroundTasks)
            .mount("/static", FileServer::from("website/static"))
            .mount("/", routes::public::root_routes())
            .mount(&admin_mount, routes::admin::routes())
            .mount(&admin_api_mount, routes::admin::api_routes())
            .mount(&admin_api_mount, routes::ai::routes())
            .mount("/api", routes::api::routes())
            .mount("/", routes::commerce::routes())
            .mount(&admin_mount, routes::security::routes())
            .register("/", catchers![not_found, server_error]);

        // SQLite backend: also manage the raw DbPool for SQLite-specific
        // health tools (VACUUM, WAL checkpoint, integrity check)
        if backend != "mongodb" {
            let pool = db::init_pool().expect("Failed to initialize SQLite pool");
            db::run_migrations(&pool).expect("Failed to run SQLite migrations");
            db::seed_defaults(&pool).expect("Failed to seed SQLite defaults");
            rocket = rocket.manage(pool);
        }

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
                .manage(site::SiteStoreManager::new())
                .manage(site::SitePoolManager::new())
                .attach(site::SiteResolver)
                .mount("/super", routes::super_admin::routes());
        }

        rocket
    } else {
        // ── Setup-only mode: no velocty.toml → minimal server with just the setup wizard ──
        eprintln!("No velocty.toml found — starting in setup mode.");
        eprintln!("Visit http://localhost:8000/admin/setup to configure your database.");

        rocket::build()
            .manage(AdminSlug("admin".to_string()))
            .manage(SetupMode(true))
            .attach(Template::fairing())
            .attach(NoCacheAdmin)
            .mount("/static", FileServer::from("website/static"))
            .mount("/admin", routes::security::setup_only_routes())
            .register("/", catchers![not_found, server_error])
    }
}
