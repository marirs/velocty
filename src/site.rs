#![cfg(feature = "multi-site")]

use dashmap::DashMap;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::DbPool;
use crate::store::Store;

// ── Registry pool newtype (distinct from DbPool for Rocket managed state) ──

pub struct RegistryPool(pub DbPool);

impl RegistryPool {
    pub fn get(
        &self,
    ) -> Result<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>, r2d2::Error> {
        self.0.get()
    }
}

impl std::ops::Deref for RegistryPool {
    type Target = DbPool;
    fn deref(&self) -> &DbPool {
        &self.0
    }
}

// ── Site record from the central registry ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: i64,
    pub slug: String,
    pub hostname: String,
    pub display_name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── SiteContext: injected per-request via request guard ───────

pub struct SiteContext {
    pub site: Site,
    pub store: Arc<dyn Store>,
    pub uploads_dir: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r SiteContext {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.local_cache(|| Option::<SiteContext>::None) {
            Some(ctx) => Outcome::Success(ctx),
            None => Outcome::Error((Status::NotFound, ())),
        }
    }
}

// ── SiteStoreManager: caches per-site Store instances ────────────────

pub struct SiteStoreManager {
    stores: DashMap<String, Arc<dyn Store>>,
}

impl SiteStoreManager {
    pub fn new() -> Self {
        SiteStoreManager {
            stores: DashMap::new(),
        }
    }

    /// Get or create a Store for the given site slug.
    /// Uses the global backend setting to decide SQLite vs MongoDB.
    pub fn get_store(&self, slug: &str) -> Result<Arc<dyn Store>, String> {
        if let Some(store) = self.stores.get(slug) {
            return Ok(store.clone());
        }

        let store = create_site_store(slug)?;
        store.run_migrations().map_err(|e| e.to_string())?;
        store.seed_defaults().map_err(|e| e.to_string())?;
        self.stores.insert(slug.to_string(), store.clone());
        Ok(store)
    }
}

/// Create a Store instance for a specific site based on the global backend config.
fn create_site_store(slug: &str) -> Result<Arc<dyn Store>, String> {
    let backend = crate::health::read_db_backend();
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
            // Each site gets its own MongoDB database: velocty_site_<slug>
            let db_name = format!("velocty_site_{}", slug);
            let ms = crate::store::mongo::MongoStore::new(uri, &db_name)?;
            Ok(Arc::new(ms) as Arc<dyn Store>)
        }
        _ => {
            let db_path = format!("website/sites/{}/db/velocty.db", slug);
            let pool = crate::db::init_pool_at(&db_path)?;
            crate::db::run_migrations(&pool).map_err(|e| e.to_string())?;
            crate::db::seed_defaults(&pool).map_err(|e| e.to_string())?;
            Ok(Arc::new(crate::store::sqlite::SqliteStore::new(pool)) as Arc<dyn Store>)
        }
    }
}

// ── Legacy SitePoolManager (kept for backward compat with super_admin health tools) ──

pub struct SitePoolManager {
    pools: DashMap<String, DbPool>,
}

impl SitePoolManager {
    pub fn new() -> Self {
        SitePoolManager {
            pools: DashMap::new(),
        }
    }

    /// Get or create a connection pool for the given site slug (SQLite only).
    pub fn get_pool(&self, slug: &str) -> Result<DbPool, String> {
        if let Some(pool) = self.pools.get(slug) {
            return Ok(pool.clone());
        }

        let db_path = format!("website/sites/{}/db/velocty.db", slug);
        let pool = crate::db::init_pool_at(&db_path)?;
        crate::db::run_migrations(&pool).map_err(|e| e.to_string())?;
        crate::db::seed_defaults(&pool).map_err(|e| e.to_string())?;
        self.pools.insert(slug.to_string(), pool.clone());
        Ok(pool)
    }
}

// ── Central registry DB helpers ──────────────────────────────

pub fn init_registry() -> Result<RegistryPool, String> {
    let pool = crate::db::init_pool_at("website/sites.db")?;
    Ok(RegistryPool(pool))
}

/// Detect a single-site installation at `website/site/` and migrate it
/// into the multi-site `website/sites/<uuid>/` layout, registering it
/// in the central registry with the given hostname.
pub fn migrate_single_to_multi(
    registry: &RegistryPool,
    hostname: &str,
    display_name: &str,
) -> Result<(), String> {
    use std::fs;
    use std::path::Path;

    let single = Path::new("website/site");
    if !single.exists() || !single.join("db/velocty.db").exists() {
        return Ok(()); // nothing to migrate
    }

    // Check if any sites already exist in the registry — if so, skip
    if !list_sites(registry).is_empty() {
        return Ok(());
    }

    log::info!("Migrating single-site to multi-site layout...");

    let slug = uuid::Uuid::new_v4().to_string();
    let dest = format!("website/sites/{}", slug);

    // Move the entire website/site/ directory to website/sites/<uuid>/
    fs::create_dir_all("website/sites").map_err(|e| e.to_string())?;
    fs::rename("website/site", &dest)
        .map_err(|e| format!("Failed to move website/site → {}: {}", dest, e))?;

    // Register in the central registry
    let conn = registry.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sites (slug, hostname, display_name) VALUES (?1, ?2, ?3)",
        params![slug, hostname, display_name],
    )
    .map_err(|e| e.to_string())?;

    log::info!(
        "Single-site migrated to multi-site as '{}' (slug: {})",
        hostname,
        slug
    );
    Ok(())
}

pub fn run_registry_migrations(pool: &RegistryPool) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sites (
            id INTEGER PRIMARY KEY,
            slug TEXT UNIQUE NOT NULL,
            hostname TEXT UNIQUE NOT NULL,
            display_name TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS super_admins (
            id INTEGER PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS super_sessions (
            token TEXT PRIMARY KEY,
            admin_id INTEGER NOT NULL,
            expires_at DATETIME NOT NULL,
            FOREIGN KEY (admin_id) REFERENCES super_admins(id)
        );",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_sites(pool: &RegistryPool) -> Vec<Site> {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare("SELECT * FROM sites ORDER BY display_name") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(Site {
            id: row.get("id")?,
            slug: row.get("slug")?,
            hostname: row.get("hostname")?,
            display_name: row.get("display_name")?,
            status: row.get("status")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn find_site_by_hostname(pool: &RegistryPool, hostname: &str) -> Option<Site> {
    let conn = pool.get().ok()?;
    conn.query_row(
        "SELECT * FROM sites WHERE hostname = ?1",
        params![hostname],
        |row| {
            Ok(Site {
                id: row.get("id")?,
                slug: row.get("slug")?,
                hostname: row.get("hostname")?,
                display_name: row.get("display_name")?,
                status: row.get("status")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        },
    )
    .ok()
}

pub fn find_site_by_id(pool: &RegistryPool, id: i64) -> Option<Site> {
    let conn = pool.get().ok()?;
    conn.query_row("SELECT * FROM sites WHERE id = ?1", params![id], |row| {
        Ok(Site {
            id: row.get("id")?,
            slug: row.get("slug")?,
            hostname: row.get("hostname")?,
            display_name: row.get("display_name")?,
            status: row.get("status")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    })
    .ok()
}

/// Create a new site. Returns (Site, temp_password, email_sent).
pub fn create_site(
    pool: &RegistryPool,
    hostname: &str,
    display_name: &str,
    admin_email: &str,
    email_settings: &HashMap<String, String>,
) -> Result<(Site, String, bool), String> {
    // Use a random UUID as the folder name so the filesystem
    // doesn't reveal which database belongs to which site.
    let slug = uuid::Uuid::new_v4().to_string();

    // Create directory structure
    let base = format!("website/sites/{}", slug);
    std::fs::create_dir_all(format!("{}/db", base)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(format!("{}/uploads", base)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(format!("{}/designs", base)).map_err(|e| e.to_string())?;

    // Insert into registry
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sites (slug, hostname, display_name) VALUES (?1, ?2, ?3)",
        params![slug, hostname, display_name],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();

    // Initialize the site's own database via Store
    let store = create_site_store(&slug)?;
    store.run_migrations().map_err(|e| e.to_string())?;
    store.seed_defaults().map_err(|e| e.to_string())?;

    // Auto-set site_url, site_name, admin_slug, and mark setup as completed
    let _ = store.setting_set("site_url", &format!("https://{}", hostname));
    let _ = store.setting_set("site_name", display_name);
    let _ = store.setting_set("admin_slug", "admin");
    let _ = store.setting_set("setup_completed", "true");
    let _ = store.setting_set("admin_email", admin_email);

    // Generate a random temporary password and create the site admin user
    let temp_password = generate_temp_password();
    let hash = crate::security::auth::hash_password(&temp_password)
        .map_err(|e| format!("Failed to hash temp password: {}", e))?;
    let user_id = store
        .user_create(admin_email, &hash, display_name, "admin")
        .map_err(|e| format!("Failed to create site admin: {}", e))?;
    let _ = store.user_set_force_password_change(user_id, true);

    // Send welcome email with temporary password (uses super admin's email settings)
    let email_sent = send_site_welcome_email(email_settings, hostname, admin_email, &temp_password);

    Ok((
        Site {
            id,
            slug,
            hostname: hostname.to_string(),
            display_name: display_name.to_string(),
            status: "active".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        },
        temp_password,
        email_sent,
    ))
}

/// Generate a random 16-character alphanumeric temporary password.
fn generate_temp_password() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Send a welcome email to the new site admin with their temporary password.
/// Uses the super admin's email settings since the sub-site has no email configured yet.
/// Returns true if the email was sent successfully.
fn send_site_welcome_email(
    settings: &HashMap<String, String>,
    hostname: &str,
    admin_email: &str,
    temp_password: &str,
) -> bool {
    let from = crate::email::get_from_or_admin(settings);
    let subject = format!("Your new site on {} is ready", hostname);
    let body = format!(
        "Hello,\n\n\
         Your site at https://{hostname}/admin is ready.\n\n\
         Log in with:\n\
         Email: {email}\n\
         Temporary password: {password}\n\n\
         You will be asked to set a new password on first login.\n\n\
         — Velocty",
        hostname = hostname,
        email = admin_email,
        password = temp_password,
    );
    match crate::email::send_via_provider(settings, &from, admin_email, &subject, &body) {
        Ok(_) => true,
        Err(e) => {
            log::warn!(
                "Failed to send welcome email to {} for site {}: {}",
                admin_email,
                hostname,
                e
            );
            false
        }
    }
}

pub fn update_site_status(pool: &RegistryPool, id: i64, status: &str) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sites SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![status, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_site(pool: &RegistryPool, id: i64) -> Result<(), String> {
    let site = find_site_by_id(pool, id).ok_or("Site not found")?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sites WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    // Remove site directory
    let base = format!("website/sites/{}", site.slug);
    let _ = std::fs::remove_dir_all(&base);
    Ok(())
}

// ── Super Admin auth helpers ─────────────────────────────────

pub fn super_admin_exists(pool: &RegistryPool) -> bool {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM super_admins", [], |row| row.get(0))
        .unwrap_or(0);
    count > 0
}

pub fn create_super_admin(pool: &RegistryPool, email: &str, password: &str) -> Result<(), String> {
    let hash = crate::security::auth::hash_password(password)?;
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO super_admins (email, password_hash) VALUES (?1, ?2)",
        params![email, hash],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn verify_super_admin(pool: &RegistryPool, email: &str, password: &str) -> bool {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let hash: String = match conn.query_row(
        "SELECT password_hash FROM super_admins WHERE email = ?1",
        params![email],
        |row| row.get(0),
    ) {
        Ok(h) => h,
        Err(_) => return false,
    };
    crate::security::auth::verify_password(password, &hash)
}

pub fn create_super_session(pool: &RegistryPool, admin_email: &str) -> Result<String, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let admin_id: i64 = conn
        .query_row(
            "SELECT id FROM super_admins WHERE email = ?1",
            params![admin_email],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let token = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(24);
    conn.execute(
        "INSERT INTO super_sessions (token, admin_id, expires_at) VALUES (?1, ?2, ?3)",
        params![token, admin_id, expires],
    )
    .map_err(|e| e.to_string())?;
    Ok(token)
}

pub fn validate_super_session(pool: &RegistryPool, token: &str) -> bool {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let now = chrono::Utc::now().naive_utc();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM super_sessions WHERE token = ?1 AND expires_at > ?2",
            params![token, now],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

pub fn destroy_super_session(pool: &RegistryPool, token: &str) -> Result<(), String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM super_sessions WHERE token = ?1",
        params![token],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── SiteResolver Fairing ─────────────────────────────────────

pub struct SiteResolver;

#[rocket::async_trait]
impl Fairing for SiteResolver {
    fn info(&self) -> Info {
        Info {
            name: "Site Resolver",
            kind: Kind::Request,
        }
    }

    async fn on_request(&self, req: &mut Request<'_>, _data: &mut rocket::Data<'_>) {
        // Skip super-admin routes
        let path = req.uri().path().as_str();
        if path.starts_with("/super") || path.starts_with("/static") {
            return;
        }

        let registry = match req.rocket().state::<RegistryPool>() {
            Some(r) => r,
            None => return,
        };

        let hostname = req
            .headers()
            .get_one("Host")
            .unwrap_or("localhost")
            .split(':')
            .next()
            .unwrap_or("localhost")
            .to_string();

        let site = match find_site_by_hostname(registry, &hostname) {
            Some(s) => s,
            None => return,
        };

        if site.status != "active" {
            return;
        }

        let store_mgr = match req.rocket().state::<SiteStoreManager>() {
            Some(m) => m,
            None => return,
        };

        let store = match store_mgr.get_store(&site.slug) {
            Ok(s) => s,
            Err(_) => return,
        };

        let uploads_dir = format!("website/sites/{}/uploads", site.slug);

        let ctx = SiteContext {
            site,
            store,
            uploads_dir,
        };

        req.local_cache(|| Some(ctx));
    }
}
