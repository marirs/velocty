use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::db::DbPool;

/// Boot instant — set once at startup via `init_uptime()`
static mut BOOT_INSTANT: Option<Instant> = None;

pub fn init_uptime() {
    unsafe {
        BOOT_INSTANT = Some(Instant::now());
    }
}

fn uptime_secs() -> u64 {
    unsafe { BOOT_INSTANT.map(|b| b.elapsed().as_secs()).unwrap_or(0) }
}

// ── Data Structures ─────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub disk: DiskInfo,
    pub database: DbInfo,
    pub resources: ResourceInfo,
    pub filesystem: Vec<FsCheck>,
    pub content: ContentStats,
    pub running_as_root: bool,
    pub process_user: String,
}

#[derive(Debug, Serialize)]
pub struct DiskInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub db_size_bytes: u64,
    pub uploads_size_bytes: u64,
    pub uploads_image_bytes: u64,
    pub uploads_video_bytes: u64,
    pub uploads_other_bytes: u64,
    pub uploads_file_count: u64,
}

#[derive(Debug, Serialize)]
pub struct DbInfo {
    pub backend: String,
    // SQLite fields
    pub file_size_bytes: u64,
    pub wal_size_bytes: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub fragmentation_pct: f64,
    pub integrity_ok: bool,
    pub table_counts: HashMap<String, u64>,
    // MongoDB fields
    pub mongo_connected: bool,
    pub mongo_latency_ms: u64,
    pub mongo_version: String,
    pub mongo_data_size: u64,
    pub mongo_storage_size: u64,
    pub mongo_index_size: u64,
    pub mongo_collections: u64,
    pub mongo_uri: String,
}

#[derive(Debug, Serialize)]
pub struct ResourceInfo {
    pub uptime_secs: u64,
    pub uptime_human: String,
    pub memory_rss_bytes: u64,
    pub os: String,
    pub arch: String,
    pub rust_version: String,
}

#[derive(Debug, Serialize)]
pub struct FsCheck {
    pub path: String,
    pub exists: bool,
    pub writable: bool,
    pub permissions: String,
    pub recommended: String,
    pub perms_ok: bool,
    pub owner: String,
    pub group: String,
    pub owned_by_root: bool,
    pub world_writable: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContentStats {
    pub posts_total: u64,
    pub posts_published: u64,
    pub posts_draft: u64,
    pub portfolio_total: u64,
    pub comments_total: u64,
    pub comments_pending: u64,
    pub categories_count: u64,
    pub tags_count: u64,
    pub sessions_total: u64,
    pub sessions_expired: u64,
}

// ── Tool Results ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub ok: bool,
    pub message: String,
    pub details: Option<String>,
}

// ── Gather Functions ────────────────────────────────────────

/// Read the database backend from velocty.toml (defaults to "sqlite")
pub fn read_db_backend() -> String {
    std::fs::read_to_string("velocty.toml")
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|v| v.get("database")?.get("backend")?.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "sqlite".to_string())
}

/// Read the MongoDB URI from velocty.toml
fn read_mongo_uri() -> String {
    std::fs::read_to_string("velocty.toml")
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|v| v.get("database")?.get("uri")?.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "mongodb://localhost:27017".to_string())
}

pub fn gather(pool: &DbPool) -> HealthReport {
    let backend = read_db_backend();
    let running_as_root = is_running_as_root();
    let process_user = get_process_user();
    HealthReport {
        disk: gather_disk(&backend),
        database: gather_db(pool, &backend),
        resources: gather_resources(),
        filesystem: gather_filesystem(&backend),
        content: gather_content(pool),
        running_as_root,
        process_user,
    }
}

fn gather_disk(backend: &str) -> DiskInfo {
    let (total, free) = disk_space(".");
    let (db_size, wal_size) = if backend == "sqlite" {
        (file_size("website/site/db/velocty.db"), file_size("website/site/db/velocty.db-wal"))
    } else {
        (0, 0) // MongoDB DB is remote, no local file
    };
    let (uploads_total, img_bytes, vid_bytes, other_bytes, file_count) =
        walk_uploads("website/site/uploads");

    DiskInfo {
        total_bytes: total,
        used_bytes: total.saturating_sub(free),
        free_bytes: free,
        db_size_bytes: db_size + wal_size,
        uploads_size_bytes: uploads_total,
        uploads_image_bytes: img_bytes,
        uploads_video_bytes: vid_bytes,
        uploads_other_bytes: other_bytes,
        uploads_file_count: file_count,
    }
}

fn gather_db(pool: &DbPool, backend: &str) -> DbInfo {
    if backend == "mongodb" {
        return gather_db_mongo();
    }
    gather_db_sqlite(pool)
}

fn gather_db_sqlite(pool: &DbPool) -> DbInfo {
    let conn = pool.get().ok();

    let db_file_size = file_size("website/site/db/velocty.db");
    let db_wal_size = file_size("website/site/db/velocty.db-wal");

    let page_count = conn
        .as_ref()
        .and_then(|c| c.query_row("PRAGMA page_count", [], |r| r.get::<_, u64>(0)).ok())
        .unwrap_or(0);

    let freelist_count = conn
        .as_ref()
        .and_then(|c| c.query_row("PRAGMA freelist_count", [], |r| r.get::<_, u64>(0)).ok())
        .unwrap_or(0);

    let fragmentation_pct = if page_count > 0 {
        (freelist_count as f64 / page_count as f64) * 100.0
    } else {
        0.0
    };

    let integrity_ok = conn
        .as_ref()
        .and_then(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).ok())
        .map(|s| s == "ok")
        .unwrap_or(false);

    let tables = [
        "posts", "portfolio_items", "comments", "categories", "tags",
        "settings", "sessions", "imports", "analytics_events",
        "post_tags", "portfolio_tags", "post_categories", "portfolio_categories",
    ];
    let mut table_counts = HashMap::new();
    if let Some(ref c) = conn {
        for t in &tables {
            let sql = format!("SELECT COUNT(*) FROM {}", t);
            if let Ok(count) = c.query_row(&sql, [], |r| r.get::<_, u64>(0)) {
                table_counts.insert(t.to_string(), count);
            }
        }
    }

    DbInfo {
        backend: "sqlite".to_string(),
        file_size_bytes: db_file_size,
        wal_size_bytes: db_wal_size,
        page_count,
        freelist_count,
        fragmentation_pct,
        integrity_ok,
        table_counts,
        mongo_connected: false,
        mongo_latency_ms: 0,
        mongo_version: String::new(),
        mongo_data_size: 0,
        mongo_storage_size: 0,
        mongo_index_size: 0,
        mongo_collections: 0,
        mongo_uri: String::new(),
    }
}

fn gather_db_mongo() -> DbInfo {
    let uri = read_mongo_uri();
    let (connected, latency_ms) = mongo_ping(&uri);

    // Mask credentials in URI for display
    let display_uri = if let Some(at_pos) = uri.find('@') {
        let scheme_end = uri.find("://").map(|p| p + 3).unwrap_or(0);
        format!("{}***@{}", &uri[..scheme_end], &uri[at_pos + 1..])
    } else {
        uri.clone()
    };

    DbInfo {
        backend: "mongodb".to_string(),
        file_size_bytes: 0,
        wal_size_bytes: 0,
        page_count: 0,
        freelist_count: 0,
        fragmentation_pct: 0.0,
        integrity_ok: connected,
        table_counts: HashMap::new(),
        mongo_connected: connected,
        mongo_latency_ms: latency_ms,
        mongo_version: String::new(), // Would need serverStatus to get this
        mongo_data_size: 0,
        mongo_storage_size: 0,
        mongo_index_size: 0,
        mongo_collections: 0,
        mongo_uri: display_uri,
    }
}

/// Public wrapper for mongo_ping, used by the health route
pub fn gather_mongo_ping(uri: &str) -> (bool, u64) {
    mongo_ping(uri)
}

/// Ping MongoDB via TCP + OP_MSG isMaster, return (connected, latency_ms)
fn mongo_ping(uri: &str) -> (bool, u64) {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let stripped = uri
        .strip_prefix("mongodb+srv://")
        .or_else(|| uri.strip_prefix("mongodb://"))
        .unwrap_or(uri);
    let after_creds = if let Some(pos) = stripped.find('@') {
        &stripped[pos + 1..]
    } else {
        stripped
    };
    let host_part = after_creds.split('/').next().unwrap_or(after_creds);
    let first_host = host_part.split(',').next().unwrap_or(host_part);
    let (host, port) = if let Some(colon) = first_host.rfind(':') {
        (first_host[..colon].to_string(), first_host[colon + 1..].parse::<u16>().unwrap_or(27017))
    } else {
        (first_host.to_string(), 27017)
    };

    let addr = format!("{}:{}", host, port);
    let start = Instant::now();

    let resolved = match addr.parse() {
        Ok(a) => a,
        Err(_) => match std::net::ToSocketAddrs::to_socket_addrs(&addr.as_str()) {
            Ok(mut addrs) => match addrs.next() {
                Some(a) => a,
                None => return (false, 0),
            },
            Err(_) => return (false, 0),
        },
    };

    let stream = match TcpStream::connect_timeout(&resolved, Duration::from_secs(3)) {
        Ok(s) => s,
        Err(_) => return (false, start.elapsed().as_millis() as u64),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    // Build isMaster OP_MSG
    let bson_doc: Vec<u8> = {
        let mut doc = Vec::new();
        doc.push(0x10); doc.extend_from_slice(b"isMaster\0"); doc.extend_from_slice(&1i32.to_le_bytes());
        doc.push(0x02); doc.extend_from_slice(b"$db\0");
        let db_val = b"admin\0";
        doc.extend_from_slice(&(db_val.len() as i32).to_le_bytes()); doc.extend_from_slice(db_val);
        doc.push(0x00);
        let total = (4 + doc.len()) as i32;
        let mut full = total.to_le_bytes().to_vec();
        full.extend_from_slice(&doc);
        full
    };

    let msg_body_len = 4 + 1 + bson_doc.len();
    let total_msg_len = (16 + msg_body_len) as i32;
    let mut msg = Vec::new();
    msg.extend_from_slice(&total_msg_len.to_le_bytes());
    msg.extend_from_slice(&1i32.to_le_bytes()); // request_id
    msg.extend_from_slice(&0i32.to_le_bytes()); // response_to
    msg.extend_from_slice(&2013i32.to_le_bytes()); // OP_MSG
    msg.extend_from_slice(&0u32.to_le_bytes()); // flags
    msg.push(0); // section kind
    msg.extend_from_slice(&bson_doc);

    let mut stream = stream;
    if stream.write_all(&msg).is_err() {
        return (false, start.elapsed().as_millis() as u64);
    }

    let mut header = [0u8; 16];
    match stream.read_exact(&mut header) {
        Ok(_) => {
            let resp_op = i32::from_le_bytes([header[12], header[13], header[14], header[15]]);
            let latency = start.elapsed().as_millis() as u64;
            (resp_op == 2013, latency)
        }
        Err(_) => (false, start.elapsed().as_millis() as u64),
    }
}

fn gather_resources() -> ResourceInfo {
    let secs = uptime_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let uptime_human = if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    };

    ResourceInfo {
        uptime_secs: secs,
        uptime_human,
        memory_rss_bytes: get_rss_bytes(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        arch: std::env::consts::ARCH.to_string(),
        rust_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn gather_filesystem(backend: &str) -> Vec<FsCheck> {
    let mut dirs: Vec<(&str, u32)> = Vec::new();
    if backend == "sqlite" {
        dirs.push(("website/site/db", 0o750));
    }
    dirs.push(("website/site/uploads", 0o755));
    dirs.push(("website/site/designs", 0o755));
    dirs.push(("website/static", 0o755));
    dirs.push(("website/templates", 0o755));

    dirs.iter()
        .map(|(p, recommended_mode)| {
            let path = Path::new(p);
            let exists = path.exists();
            let writable = if exists {
                let test_file = path.join(".velocty_write_test");
                let ok = std::fs::write(&test_file, b"test").is_ok();
                let _ = std::fs::remove_file(&test_file);
                ok
            } else {
                false
            };

            let (permissions, actual_mode, owner, group, uid, gid) = if exists {
                get_path_info(path)
            } else {
                ("-".to_string(), 0u32, "-".to_string(), "-".to_string(), u32::MAX, u32::MAX)
            };

            let recommended = format!("{:o}", recommended_mode);
            let perms_ok = !exists || actual_mode == *recommended_mode;
            let owned_by_root = exists && uid == 0;
            let world_writable = exists && (actual_mode & 0o002) != 0;

            // Build warning message
            let mut warnings: Vec<String> = Vec::new();
            if exists && !perms_ok {
                warnings.push(format!("Permissions are {} but should be {}. Run: chmod {} {}", permissions, recommended, recommended, p));
            }
            if world_writable {
                warnings.push(format!("World-writable ({}). Security risk! Run: chmod o-w {}", permissions, p));
            }
            if owned_by_root {
                warnings.push(format!("Owned by root:{}. Should be owned by the application user.", group));
            }
            let warning = if warnings.is_empty() { None } else { Some(warnings.join(" ")) };

            FsCheck {
                path: p.to_string(),
                exists,
                writable,
                permissions,
                recommended,
                perms_ok,
                owner,
                group,
                owned_by_root,
                world_writable,
                warning,
            }
        })
        .collect()
}

#[cfg(unix)]
fn get_path_info(path: &Path) -> (String, u32, String, String, u32, u32) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    match std::fs::metadata(path) {
        Ok(meta) => {
            let mode = meta.permissions().mode() & 0o777;
            let uid = meta.uid();
            let gid = meta.gid();
            let owner = resolve_username(uid);
            let group = resolve_groupname(gid);
            (format!("{:o}", mode), mode, owner, group, uid, gid)
        }
        Err(_) => ("???".to_string(), 0, "???".to_string(), "???".to_string(), u32::MAX, u32::MAX),
    }
}

#[cfg(not(unix))]
fn get_path_info(_path: &Path) -> (String, u32, String, String, u32, u32) {
    ("n/a".to_string(), 0, "n/a".to_string(), "n/a".to_string(), u32::MAX, u32::MAX)
}

#[cfg(unix)]
fn resolve_username(uid: u32) -> String {
    unsafe {
        let pw = libc::getpwuid(uid);
        if !pw.is_null() {
            let name = std::ffi::CStr::from_ptr((*pw).pw_name);
            return name.to_string_lossy().to_string();
        }
    }
    uid.to_string()
}

#[cfg(unix)]
fn resolve_groupname(gid: u32) -> String {
    unsafe {
        let gr = libc::getgrgid(gid);
        if !gr.is_null() {
            let name = std::ffi::CStr::from_ptr((*gr).gr_name);
            return name.to_string_lossy().to_string();
        }
    }
    gid.to_string()
}

fn is_running_as_root() -> bool {
    #[cfg(unix)]
    { unsafe { libc::geteuid() == 0 } }
    #[cfg(not(unix))]
    { false }
}

fn get_process_user() -> String {
    #[cfg(unix)]
    { resolve_username(unsafe { libc::geteuid() }) }
    #[cfg(not(unix))]
    { "unknown".to_string() }
}

fn gather_content(pool: &DbPool) -> ContentStats {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return ContentStats {
                posts_total: 0,
                posts_published: 0,
                posts_draft: 0,
                portfolio_total: 0,
                comments_total: 0,
                comments_pending: 0,
                categories_count: 0,
                tags_count: 0,
                sessions_total: 0,
                sessions_expired: 0,
            }
        }
    };

    let count = |sql: &str| -> u64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
    };

    ContentStats {
        posts_total: count("SELECT COUNT(*) FROM posts"),
        posts_published: count("SELECT COUNT(*) FROM posts WHERE status = 'published'"),
        posts_draft: count("SELECT COUNT(*) FROM posts WHERE status = 'draft'"),
        portfolio_total: count("SELECT COUNT(*) FROM portfolio_items"),
        comments_total: count("SELECT COUNT(*) FROM comments"),
        comments_pending: count("SELECT COUNT(*) FROM comments WHERE status = 'pending'"),
        categories_count: count("SELECT COUNT(*) FROM categories"),
        tags_count: count("SELECT COUNT(*) FROM tags"),
        sessions_total: count("SELECT COUNT(*) FROM sessions"),
        sessions_expired: count("SELECT COUNT(*) FROM sessions WHERE expires_at < datetime('now')"),
    }
}

// ── Tool Actions ────────────────────────────────────────────

pub fn run_vacuum(pool: &DbPool) -> ToolResult {
    let old_size = file_size("website/site/db/velocty.db");
    match pool.get() {
        Ok(conn) => match conn.execute_batch("VACUUM") {
            Ok(_) => {
                let new_size = file_size("website/site/db/velocty.db");
                let reclaimed = old_size.saturating_sub(new_size);
                let pct = if old_size > 0 {
                    (reclaimed as f64 / old_size as f64) * 100.0
                } else {
                    0.0
                };
                let detail = if reclaimed > 0 {
                    format!("{} → {} ({:.1}% reclaimed)", human_bytes(old_size), human_bytes(new_size), pct)
                } else {
                    format!("{} → {} (already compact)", human_bytes(old_size), human_bytes(new_size))
                };
                ToolResult {
                    ok: true,
                    message: "Database vacuumed successfully.".to_string(),
                    details: Some(detail),
                }
            }
            Err(e) => ToolResult {
                ok: false,
                message: format!("Vacuum failed: {}", e),
                details: None,
            },
        },
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn run_wal_checkpoint(pool: &DbPool) -> ToolResult {
    match pool.get() {
        Ok(conn) => match conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
            Ok(_) => ToolResult {
                ok: true,
                message: "WAL checkpoint completed.".to_string(),
                details: Some("WAL file truncated.".to_string()),
            },
            Err(e) => ToolResult {
                ok: false,
                message: format!("Checkpoint failed: {}", e),
                details: None,
            },
        },
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn run_integrity_check(pool: &DbPool) -> ToolResult {
    match pool.get() {
        Ok(conn) => {
            let mut stmt = match conn.prepare("PRAGMA integrity_check") {
                Ok(s) => s,
                Err(e) => {
                    return ToolResult {
                        ok: false,
                        message: format!("Failed: {}", e),
                        details: None,
                    }
                }
            };
            let results: Vec<String> = stmt
                .query_map([], |r| r.get(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            if results.len() == 1 && results[0] == "ok" {
                ToolResult {
                    ok: true,
                    message: "Database integrity check passed.".to_string(),
                    details: None,
                }
            } else {
                ToolResult {
                    ok: false,
                    message: "Integrity issues found.".to_string(),
                    details: Some(results.join("\n")),
                }
            }
        }
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn run_session_cleanup(pool: &DbPool) -> ToolResult {
    match pool.get() {
        Ok(conn) => {
            let expired: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE expires_at < datetime('now')",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            match conn.execute("DELETE FROM sessions WHERE expires_at < datetime('now')", []) {
                Ok(_) => ToolResult {
                    ok: true,
                    message: format!("Cleaned up {} expired session(s).", expired),
                    details: None,
                },
                Err(e) => ToolResult {
                    ok: false,
                    message: format!("Failed: {}", e),
                    details: None,
                },
            }
        }
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn run_orphan_scan(pool: &DbPool) -> ToolResult {
    let uploads_dir = "website/site/uploads";
    let dir = Path::new(uploads_dir);
    if !dir.exists() {
        return ToolResult {
            ok: true,
            message: "Uploads directory does not exist.".to_string(),
            details: None,
        };
    }

    // Collect all referenced filenames from DB
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                ok: false,
                message: format!("Cannot get connection: {}", e),
                details: None,
            }
        }
    };

    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Post featured images
    if let Ok(mut stmt) = conn.prepare("SELECT featured_image FROM posts WHERE featured_image IS NOT NULL AND featured_image != ''") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Some(name) = row.split('/').last() {
                    referenced.insert(name.to_string());
                }
            }
        }
    }

    // Portfolio images
    if let Ok(mut stmt) = conn.prepare("SELECT image_path FROM portfolio_items WHERE image_path IS NOT NULL AND image_path != ''") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Some(name) = row.split('/').last() {
                    referenced.insert(name.to_string());
                }
            }
        }
    }

    // Images referenced in post/portfolio body content
    if let Ok(mut stmt) = conn.prepare("SELECT body FROM posts WHERE body IS NOT NULL") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for body in rows.flatten() {
                extract_upload_refs(&body, &mut referenced);
            }
        }
    }
    if let Ok(mut stmt) = conn.prepare("SELECT description FROM portfolio_items WHERE description IS NOT NULL") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for body in rows.flatten() {
                extract_upload_refs(&body, &mut referenced);
            }
        }
    }

    // Settings (logo, favicon, etc.)
    if let Ok(mut stmt) = conn.prepare("SELECT value FROM settings WHERE value LIKE '%/uploads/%'") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for val in rows.flatten() {
                if let Some(name) = val.split('/').last() {
                    referenced.insert(name.to_string());
                }
            }
        }
    }

    // Walk uploads and find orphans (skip thumbnail dirs)
    let mut orphans: Vec<String> = Vec::new();
    let mut orphan_bytes: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip thumbnail variants (small_, medium_, large_)
                let base_name = name
                    .strip_prefix("small_")
                    .or_else(|| name.strip_prefix("medium_"))
                    .or_else(|| name.strip_prefix("large_"))
                    .unwrap_or(&name);

                if !referenced.contains(base_name) && !referenced.contains(&name) {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        orphan_bytes += meta.len();
                    }
                    orphans.push(name);
                }
            }
        }
    }

    if orphans.is_empty() {
        ToolResult {
            ok: true,
            message: "No orphan files found.".to_string(),
            details: None,
        }
    } else {
        ToolResult {
            ok: true,
            message: format!(
                "Found {} orphan file(s) ({}). Use 'Delete Orphans' to remove them.",
                orphans.len(),
                human_bytes(orphan_bytes)
            ),
            details: Some(orphans.join("\n")),
        }
    }
}

pub fn run_orphan_delete(pool: &DbPool) -> ToolResult {
    // Re-scan to get current orphans, then delete
    let scan = run_orphan_scan(pool);
    if let Some(ref details) = scan.details {
        let uploads_dir = Path::new("website/site/uploads");
        let mut deleted = 0u64;
        let mut freed = 0u64;
        for name in details.lines() {
            let path = uploads_dir.join(name);
            if let Ok(meta) = std::fs::metadata(&path) {
                freed += meta.len();
            }
            if std::fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        }
        ToolResult {
            ok: true,
            message: format!("Deleted {} orphan file(s), freed {}.", deleted, human_bytes(freed)),
            details: None,
        }
    } else {
        ToolResult {
            ok: true,
            message: "No orphan files to delete.".to_string(),
            details: None,
        }
    }
}

pub fn run_unused_tags_cleanup(pool: &DbPool) -> ToolResult {
    match pool.get() {
        Ok(conn) => {
            let count: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tags WHERE id NOT IN (SELECT tag_id FROM post_tags) AND id NOT IN (SELECT tag_id FROM portfolio_tags)",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            if count == 0 {
                return ToolResult {
                    ok: true,
                    message: "No unused tags found.".to_string(),
                    details: None,
                };
            }

            match conn.execute(
                "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM post_tags) AND id NOT IN (SELECT tag_id FROM portfolio_tags)",
                [],
            ) {
                Ok(_) => ToolResult {
                    ok: true,
                    message: format!("Deleted {} unused tag(s).", count),
                    details: None,
                },
                Err(e) => ToolResult {
                    ok: false,
                    message: format!("Failed: {}", e),
                    details: None,
                },
            }
        }
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn run_analytics_prune(pool: &DbPool, days: u64) -> ToolResult {
    if days == 0 {
        return ToolResult {
            ok: false,
            message: "Please specify a number of days greater than 0.".to_string(),
            details: None,
        };
    }

    match pool.get() {
        Ok(conn) => {
            let count: u64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM analytics_events WHERE created_at < datetime('now', '-{} days')",
                        days
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            match conn.execute(
                &format!(
                    "DELETE FROM analytics_events WHERE created_at < datetime('now', '-{} days')",
                    days
                ),
                [],
            ) {
                Ok(_) => ToolResult {
                    ok: true,
                    message: format!("Pruned {} analytics event(s) older than {} days.", count, days),
                    details: None,
                },
                Err(e) => ToolResult {
                    ok: false,
                    message: format!("Failed: {}", e),
                    details: None,
                },
            }
        }
        Err(e) => ToolResult {
            ok: false,
            message: format!("Cannot get connection: {}", e),
            details: None,
        },
    }
}

pub fn export_database() -> ToolResult {
    let src = Path::new("website/site/db/velocty.db");
    if !src.exists() {
        return ToolResult {
            ok: false,
            message: "Database file not found.".to_string(),
            details: None,
        };
    }
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let export_name = format!("velocty_backup_{}.db", timestamp);
    let export_path = format!("website/site/db/{}", export_name);
    match std::fs::copy(src, &export_path) {
        Ok(_) => ToolResult {
            ok: true,
            message: format!("Database exported as {}", export_name),
            details: Some(export_path),
        },
        Err(e) => ToolResult {
            ok: false,
            message: format!("Export failed: {}", e),
            details: None,
        },
    }
}

pub fn export_content(pool: &DbPool) -> ToolResult {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                ok: false,
                message: format!("Cannot get connection: {}", e),
                details: None,
            }
        }
    };

    let mut export = serde_json::Map::new();

    // Export posts
    if let Ok(mut stmt) = conn.prepare("SELECT id, title, slug, body, excerpt, featured_image, status, created_at, updated_at FROM posts ORDER BY id") {
        let posts: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "title": r.get::<_, String>(1).unwrap_or_default(),
                    "slug": r.get::<_, String>(2).unwrap_or_default(),
                    "body": r.get::<_, String>(3).unwrap_or_default(),
                    "excerpt": r.get::<_, String>(4).unwrap_or_default(),
                    "featured_image": r.get::<_, String>(5).unwrap_or_default(),
                    "status": r.get::<_, String>(6).unwrap_or_default(),
                    "created_at": r.get::<_, String>(7).unwrap_or_default(),
                    "updated_at": r.get::<_, String>(8).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("posts".to_string(), serde_json::Value::Array(posts));
    }

    // Export portfolio items
    if let Ok(mut stmt) = conn.prepare("SELECT id, title, slug, description, image_path, status, created_at, updated_at FROM portfolio_items ORDER BY id") {
        let items: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "title": r.get::<_, String>(1).unwrap_or_default(),
                    "slug": r.get::<_, String>(2).unwrap_or_default(),
                    "description": r.get::<_, String>(3).unwrap_or_default(),
                    "image_path": r.get::<_, String>(4).unwrap_or_default(),
                    "status": r.get::<_, String>(5).unwrap_or_default(),
                    "created_at": r.get::<_, String>(6).unwrap_or_default(),
                    "updated_at": r.get::<_, String>(7).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("portfolio_items".to_string(), serde_json::Value::Array(items));
    }

    // Export categories
    if let Ok(mut stmt) = conn.prepare("SELECT id, name, slug FROM categories ORDER BY id") {
        let cats: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1).unwrap_or_default(),
                    "slug": r.get::<_, String>(2).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("categories".to_string(), serde_json::Value::Array(cats));
    }

    // Export tags
    if let Ok(mut stmt) = conn.prepare("SELECT id, name, slug FROM tags ORDER BY id") {
        let tags: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1).unwrap_or_default(),
                    "slug": r.get::<_, String>(2).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("tags".to_string(), serde_json::Value::Array(tags));
    }

    // Export comments
    if let Ok(mut stmt) = conn.prepare("SELECT id, post_id, author_name, author_email, body, status, created_at FROM comments ORDER BY id") {
        let comments: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "post_id": r.get::<_, i64>(1)?,
                    "author_name": r.get::<_, String>(2).unwrap_or_default(),
                    "author_email": r.get::<_, String>(3).unwrap_or_default(),
                    "body": r.get::<_, String>(4).unwrap_or_default(),
                    "status": r.get::<_, String>(5).unwrap_or_default(),
                    "created_at": r.get::<_, String>(6).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("comments".to_string(), serde_json::Value::Array(comments));
    }

    // Export post-tag and post-category relationships
    if let Ok(mut stmt) = conn.prepare("SELECT post_id, tag_id FROM post_tags") {
        let rels: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "post_id": r.get::<_, i64>(0)?,
                    "tag_id": r.get::<_, i64>(1)?,
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("post_tags".to_string(), serde_json::Value::Array(rels));
    }
    if let Ok(mut stmt) = conn.prepare("SELECT post_id, category_id FROM post_categories") {
        let rels: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "post_id": r.get::<_, i64>(0)?,
                    "category_id": r.get::<_, i64>(1)?,
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("post_categories".to_string(), serde_json::Value::Array(rels));
    }
    if let Ok(mut stmt) = conn.prepare("SELECT portfolio_item_id, tag_id FROM portfolio_tags") {
        let rels: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "portfolio_item_id": r.get::<_, i64>(0)?,
                    "tag_id": r.get::<_, i64>(1)?,
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("portfolio_tags".to_string(), serde_json::Value::Array(rels));
    }
    if let Ok(mut stmt) = conn.prepare("SELECT portfolio_item_id, category_id FROM portfolio_categories") {
        let rels: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "portfolio_item_id": r.get::<_, i64>(0)?,
                    "category_id": r.get::<_, i64>(1)?,
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("portfolio_categories".to_string(), serde_json::Value::Array(rels));
    }

    // Export settings
    if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM settings ORDER BY key") {
        let settings: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "value": r.get::<_, String>(1).unwrap_or_default(),
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        export.insert("settings".to_string(), serde_json::Value::Array(settings));
    }

    let json_data = serde_json::Value::Object(export);
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let export_name = format!("velocty_export_{}.json", timestamp);
    let export_path = format!("website/site/db/{}", export_name);

    match std::fs::write(&export_path, serde_json::to_string_pretty(&json_data).unwrap_or_default()) {
        Ok(_) => ToolResult {
            ok: true,
            message: format!("Content exported as {}", export_name),
            details: Some(export_path),
        },
        Err(e) => ToolResult {
            ok: false,
            message: format!("Export failed: {}", e),
            details: None,
        },
    }
}

// ── Helpers ─────────────────────────────────────────────────

fn file_size(path: &str) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn disk_space(path: &str) -> (u64, u64) {
    // Use libc statvfs on unix
    #[cfg(unix)]
    {
        use std::ffi::CString;
        let c_path = CString::new(path).unwrap_or_default();
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
                let total = stat.f_blocks as u64 * stat.f_frsize as u64;
                let free = stat.f_bavail as u64 * stat.f_frsize as u64;
                return (total, free);
            }
        }
    }
    (0, 0)
}

fn walk_uploads(path: &str) -> (u64, u64, u64, u64, u64) {
    let dir = Path::new(path);
    if !dir.exists() {
        return (0, 0, 0, 0, 0);
    }

    let image_exts = ["jpg", "jpeg", "png", "gif", "webp", "svg", "tiff", "heic", "bmp", "ico"];
    let video_exts = ["mp4", "webm", "mov", "avi", "mkv"];

    let mut total: u64 = 0;
    let mut img: u64 = 0;
    let mut vid: u64 = 0;
    let mut other: u64 = 0;
    let mut count: u64 = 0;

    fn walk_recursive(
        dir: &Path,
        image_exts: &[&str],
        video_exts: &[&str],
        total: &mut u64,
        img: &mut u64,
        vid: &mut u64,
        other: &mut u64,
        count: &mut u64,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_recursive(&path, image_exts, video_exts, total, img, vid, other, count);
                } else if path.is_file() {
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    *total += size;
                    *count += 1;
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if image_exts.contains(&ext.as_str()) {
                        *img += size;
                    } else if video_exts.contains(&ext.as_str()) {
                        *vid += size;
                    } else {
                        *other += size;
                    }
                }
            }
        }
    }

    walk_recursive(dir, &image_exts, &video_exts, &mut total, &mut img, &mut vid, &mut other, &mut count);
    (total, img, vid, other, count)
}

fn get_rss_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        unsafe {
            let mut info: libc::mach_task_basic_info_data_t = mem::zeroed();
            let mut count = (mem::size_of::<libc::mach_task_basic_info_data_t>()
                / mem::size_of::<libc::natural_t>()) as libc::mach_msg_type_number_t;
            let kr = libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut info as *mut _ as libc::task_info_t,
                &mut count,
            );
            if kr == libc::KERN_SUCCESS {
                return info.resident_size as u64;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let kb: u64 = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    return kb * 1024;
                }
            }
        }
    }
    0
}

pub fn human_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn extract_upload_refs(html: &str, refs: &mut std::collections::HashSet<String>) {
    // Simple extraction of filenames from /uploads/... references in HTML
    let mut search_from = 0;
    while let Some(pos) = html[search_from..].find("/uploads/") {
        let start = search_from + pos + 9; // skip "/uploads/"
        if start >= html.len() {
            break;
        }
        let end = html[start..]
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c == ' ' || c == '?' || c == '#')
            .map(|i| start + i)
            .unwrap_or(html.len());
        let filename = &html[start..end];
        if !filename.is_empty() && !filename.contains('/') {
            refs.insert(filename.to_string());
        }
        search_from = end;
    }
}

