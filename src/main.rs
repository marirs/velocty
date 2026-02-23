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

use std::sync::{Arc, RwLock};

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::{uri::Origin, Header};
use rocket::response::content::RawHtml;

use models::settings::SettingsCache;
use store::Store;

/// Fixed internal mount point for admin routes.
/// The public-facing slug is dynamic and resolved per-request by AdminSlugRewriter.
pub const ADMIN_INTERNAL_MOUNT: &str = "/__adm";

/// Holds the admin URL slug (e.g. "admin"). Protected by RwLock so it can be
/// updated at runtime when the user changes it in Settings > Security.
pub struct AdminSlug {
    inner: RwLock<String>,
}

impl AdminSlug {
    pub fn new(slug: String) -> Self {
        Self {
            inner: RwLock::new(slug),
        }
    }

    /// Read the current admin slug.
    pub fn get(&self) -> String {
        self.inner
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "admin".to_string())
    }

    /// Update the admin slug at runtime (called from settings_save).
    pub fn set(&self, new_slug: &str) {
        if let Ok(mut w) = self.inner.write() {
            *w = new_slug.to_string();
        }
    }
}

/// Marker: true when running in setup-only mode (no DB yet).
pub struct SetupMode(pub bool);

/// Fairing that rewrites incoming requests from `/{admin_slug}/...` to `/__adm/...`
/// so that Rocket's statically-mounted routes can handle them. This makes the admin
/// slug fully dynamic — changing it in settings takes effect immediately.
pub struct AdminSlugRewriter;

#[rocket::async_trait]
impl Fairing for AdminSlugRewriter {
    fn info(&self) -> Info {
        Info {
            name: "Admin Slug Rewriter",
            kind: Kind::Request,
        }
    }

    async fn on_request(&self, req: &mut rocket::Request<'_>, _data: &mut rocket::Data<'_>) {
        let slug = req
            .rocket()
            .state::<AdminSlug>()
            .map(|s| s.get())
            .unwrap_or_else(|| "admin".to_string());

        let path = req.uri().path().as_str().to_string();
        let prefix = format!("/{}/", slug);
        let exact = format!("/{}", slug);

        let new_path = if path.starts_with(&prefix) {
            format!("{}/{}", ADMIN_INTERNAL_MOUNT, &path[prefix.len()..])
        } else if path == exact || path == format!("{}/", exact) {
            ADMIN_INTERNAL_MOUNT.to_string()
        } else {
            return;
        };

        // Preserve query string
        let new_uri = if let Some(q) = req.uri().query() {
            format!("{}?{}", new_path, q)
        } else {
            new_path
        };

        if let Ok(origin) = Origin::parse_owned(new_uri) {
            req.set_uri(origin);
        }
    }
}

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
        // After rewriting, admin paths start with /__adm
        if req.uri().path().starts_with(ADMIN_INTERNAL_MOUNT) {
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
    let slug = req
        .rocket()
        .state::<AdminSlug>()
        .map(|s| s.get())
        .unwrap_or_else(|| "admin".to_string());

    // If in setup mode, redirect everything to /admin/setup
    if let Some(setup) = req.rocket().state::<SetupMode>() {
        if setup.0 {
            let setup_url = format!("/{}/setup", slug);
            return RawHtml(format!(
                "<html><head><meta http-equiv=\"refresh\" content=\"0;url={}\"></head><body></body></html>",
                setup_url
            ));
        }
    }

    // If the 404 is for an admin path (auth guard forwarded → rewritten to /__adm), redirect to login
    let path = req.uri().path().as_str();
    let adm_prefix = format!("{}/", ADMIN_INTERNAL_MOUNT);
    if (path == ADMIN_INTERNAL_MOUNT || path.starts_with(&adm_prefix))
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
        let admin_api_mount = format!("{}/api", ADMIN_INTERNAL_MOUNT);

        let settings_cache = SettingsCache::load_from_store(&*store);

        eprintln!("Database backend: {}", backend);
        eprintln!(
            "Admin slug: /{} (internally mounted at {})",
            admin_slug, ADMIN_INTERNAL_MOUNT
        );
        eprintln!("Dynamic routing enabled — blog/portfolio slugs and enabled flags read from cache at request time");

        #[allow(unused_mut)]
        let mut rocket = rocket::build()
            .manage(store)
            .manage(AdminSlug::new(admin_slug))
            .manage(SetupMode(false))
            .manage(settings_cache)
            .manage(rate_limit::RateLimiter::new())
            .manage(security::firewall::FwRateLimiter::new())
            .attach(Template::fairing())
            .attach(AdminSlugRewriter)
            .attach(security::firewall::FirewallFairing)
            .attach(analytics::AnalyticsFairing)
            .attach(NoCacheAdmin)
            .attach(tasks::BackgroundTasks)
            .mount("/static", FileServer::from("website/static"))
            .mount("/", routes::public::root_routes())
            .mount(ADMIN_INTERNAL_MOUNT, routes::admin::routes())
            .mount(&admin_api_mount, routes::admin::api_routes())
            .mount(&admin_api_mount, routes::ai::routes())
            .mount("/api", routes::api::routes())
            .mount("/api", routes::deploy::public_routes())
            .mount(&admin_api_mount, routes::deploy::admin_routes())
            .mount("/", routes::commerce::routes())
            .mount(ADMIN_INTERNAL_MOUNT, routes::security::routes())
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
            .manage(AdminSlug::new("admin".to_string()))
            .manage(SetupMode(true))
            .attach(Template::fairing())
            .attach(AdminSlugRewriter)
            .attach(NoCacheAdmin)
            .mount("/static", FileServer::from("website/static"))
            .mount(ADMIN_INTERNAL_MOUNT, routes::security::setup_only_routes())
            .register("/", catchers![not_found, server_error])
    }
}
