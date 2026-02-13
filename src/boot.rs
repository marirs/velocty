use log::{error, info, warn};
use std::fs;
use std::path::Path;
use std::process;

/// Required directories that will be created if missing
const REQUIRED_DIRS: &[&str] = &[
    "website",
    "website/db",
    "website/uploads",
    "website/static",
    "website/static/css",
    "website/static/js",
    "website/templates",
    "website/templates/admin",
    "website/designs",
];

/// Critical template files — server cannot function without these
const CRITICAL_TEMPLATES: &[&str] = &[
    "website/templates/admin/base.html.tera",
    "website/templates/admin/login.html.tera",
    "website/templates/admin/dashboard.html.tera",
];

/// Critical static assets
const CRITICAL_STATIC: &[&str] = &[
    "website/static/css/admin.css",
];

/// Run all boot checks. Call this before Rocket launches.
/// Creates missing directories, warns about missing files, and
/// aborts if critical dependencies are absent.
pub fn run() {
    info!("Velocty boot check starting...");

    let mut warnings = 0u32;
    let mut errors = 0u32;

    // ── 1. Directories ─────────────────────────────────
    for dir in REQUIRED_DIRS {
        let path = Path::new(dir);
        if !path.exists() {
            match fs::create_dir_all(path) {
                Ok(_) => info!("  Created directory: {}", dir),
                Err(e) => {
                    error!("  FAILED to create directory {}: {}", dir, e);
                    errors += 1;
                }
            }
        }
    }

    // ── 2. Critical templates ──────────────────────────
    for file in CRITICAL_TEMPLATES {
        if !Path::new(file).exists() {
            error!("  MISSING critical template: {}", file);
            errors += 1;
        }
    }

    // ── 3. Critical static assets ──────────────────────
    for file in CRITICAL_STATIC {
        if !Path::new(file).exists() {
            warn!("  Missing static asset: {} (admin UI will be unstyled)", file);
            warnings += 1;
        }
    }

    // ── 4. Template subdirectories ─────────────────────
    let template_subdirs = [
        "website/templates/admin/posts",
        "website/templates/admin/portfolio",
        "website/templates/admin/comments",
        "website/templates/admin/categories",
        "website/templates/admin/tags",
        "website/templates/admin/designs",
        "website/templates/admin/import",
        "website/templates/admin/settings",
    ];

    for dir in &template_subdirs {
        let path = Path::new(dir);
        if !path.exists() {
            warn!("  Missing template directory: {} (some admin pages will 500)", dir);
            warnings += 1;
        } else {
            // Check at least one .tera file exists in each
            let has_templates = fs::read_dir(path)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .any(|e| {
                            e.path()
                                .extension()
                                .map(|ext| ext == "tera")
                                .unwrap_or(false)
                        })
                })
                .unwrap_or(false);

            if !has_templates {
                warn!("  Template directory empty: {}", dir);
                warnings += 1;
            }
        }
    }

    // ── 5. Database directory writable ──────────────────
    let db_dir = Path::new("website/db");
    if db_dir.exists() {
        let test_file = db_dir.join(".write_test");
        match fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
            }
            Err(e) => {
                error!("  Database directory not writable: {}", e);
                errors += 1;
            }
        }
    }

    // ── 6. Uploads directory writable ───────────────────
    let uploads_dir = Path::new("website/uploads");
    if uploads_dir.exists() {
        let test_file = uploads_dir.join(".write_test");
        match fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
            }
            Err(e) => {
                warn!("  Uploads directory not writable: {} (file uploads will fail)", e);
                warnings += 1;
            }
        }
    }

    // ── 7. Rocket.toml exists ───────────────────────────
    if !Path::new("Rocket.toml").exists() {
        warn!("  Rocket.toml not found — using default config");
        warnings += 1;
    }

    // ── Summary ─────────────────────────────────────────
    if errors > 0 {
        error!(
            "Boot check FAILED: {} error(s), {} warning(s). Aborting.",
            errors, warnings
        );
        process::exit(1);
    }

    if warnings > 0 {
        warn!(
            "Boot check passed with {} warning(s). Some features may not work correctly.",
            warnings
        );
    } else {
        info!("Boot check passed. All systems go.");
    }
}
