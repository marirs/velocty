use rocket::fs::TempFile;

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::AdminSlug;

pub mod api;
pub mod categories;
pub mod comments;
pub mod dashboard;
pub mod designs;
pub mod firewall;
pub mod health;
pub mod import;
pub mod media;
pub mod portfolio;
pub mod posts;
pub mod sales;
pub mod settings;
pub mod users;

/// Helper: get the admin base path from managed state
pub(crate) fn admin_base(slug: &AdminSlug) -> String {
    format!("/{}", slug.0)
}

/// If status is "published" but published_at is in the future, override to "scheduled".
pub(crate) fn resolve_status(status: &str, published_at: &Option<String>) -> String {
    if status == "published" {
        if let Some(ref dt_str) = published_at {
            if !dt_str.is_empty() {
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M") {
                    if dt > chrono::Utc::now().naive_utc() {
                        return "scheduled".to_string();
                    }
                }
            }
        }
    }
    status.to_string()
}

pub(crate) async fn save_upload(
    file: &mut TempFile<'_>,
    prefix: &str,
    pool: &DbPool,
) -> Option<String> {
    // Try content-type extension first, then original filename (raw_name), then field name
    let ext = file
        .content_type()
        .and_then(|ct| ct.extension())
        .map(|e| e.to_string())
        .or_else(|| {
            file.raw_name().and_then(|rn| {
                let s = rn.dangerous_unsafe_unsanitized_raw().as_str().to_string();
                s.rsplit('.').next().map(|e| e.to_lowercase())
            })
        })
        .or_else(|| {
            file.name()
                .and_then(|n| n.rsplit('.').next())
                .map(|e| e.to_lowercase())
        })
        .unwrap_or_else(|| "jpg".to_string());

    let uid = uuid::Uuid::new_v4();
    let filename = format!("{}_{}.{}", prefix, uid, ext);
    let upload_dir = std::path::Path::new("website/site/uploads");
    let _ = std::fs::create_dir_all(upload_dir);
    let dest = upload_dir.join(&filename);

    if file.persist_to(&dest).await.is_err() {
        return None;
    }

    let ext_lower = ext.to_lowercase();

    // ── HEIC/HEIF → JPG conversion (always, browsers can't display HEIC) ──
    if ext_lower == "heic" || ext_lower == "heif" {
        let jpg_filename = format!("{}_{}.jpg", prefix, uid);
        let jpg_dest = upload_dir.join(&jpg_filename);

        let converted = convert_heic_to_jpg(&dest, &jpg_dest);
        // Remove the original HEIC file regardless
        let _ = std::fs::remove_file(&dest);

        if !converted {
            return None;
        }

        // If WebP conversion is enabled, convert the JPG to WebP
        if Setting::get_bool(pool, "images_webp_convert") {
            if let Some(webp_name) = convert_to_webp_file(&jpg_dest, prefix, &uid, upload_dir) {
                let _ = std::fs::remove_file(&jpg_dest);
                return Some(webp_name);
            }
        }

        return Some(jpg_filename);
    }

    // ── WebP conversion for other image types ──
    if Setting::get_bool(pool, "images_webp_convert") && ext_lower != "webp" && ext_lower != "svg" {
        if let Some(webp_name) = convert_to_webp_file(&dest, prefix, &uid, upload_dir) {
            let _ = std::fs::remove_file(&dest);
            return Some(webp_name);
        }
    }

    Some(filename)
}

/// Convert HEIC/HEIF to JPG using system tools (sips on macOS, magick/heif-convert on Linux)
fn convert_heic_to_jpg(src: &std::path::Path, dst: &std::path::Path) -> bool {
    // Try sips (macOS built-in)
    if let Ok(status) = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "85"])
        .arg(src)
        .arg("--out")
        .arg(dst)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        if status.success() {
            return true;
        }
    }
    // Try ImageMagick (magick convert)
    if let Ok(status) = std::process::Command::new("magick")
        .arg(src)
        .arg("-quality")
        .arg("85")
        .arg(dst)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        if status.success() {
            return true;
        }
    }
    // Try heif-convert
    if let Ok(status) = std::process::Command::new("heif-convert")
        .arg(src)
        .arg(dst)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        if status.success() {
            return true;
        }
    }
    false
}

/// Convert an image file to WebP using the image + webp crates
fn convert_to_webp_file(
    src: &std::path::Path,
    prefix: &str,
    uid: &uuid::Uuid,
    upload_dir: &std::path::Path,
) -> Option<String> {
    let img = image::open(src).ok()?;
    let (w, h) = image::GenericImageView::dimensions(&img);
    let rgba = img.to_rgba8();
    let encoder = webp::Encoder::from_rgba(&rgba, w, h);
    let webp_data = encoder.encode(85.0);
    let webp_filename = format!("{}_{}.webp", prefix, uid);
    let webp_dest = upload_dir.join(&webp_filename);
    std::fs::write(&webp_dest, &*webp_data).ok()?;
    Some(webp_filename)
}

/// Check if a file extension is in the allowed image types
pub(crate) fn is_allowed_image(file: &TempFile<'_>, pool: &DbPool) -> bool {
    let allowed = Setting::get(pool, "images_allowed_types")
        .unwrap_or_else(|| "jpg,jpeg,png,gif,webp,svg,tiff".to_string());
    let allowed_list: Vec<&str> = allowed.split(',').map(|s| s.trim()).collect();

    // Get extension from content type, then original filename (raw_name), then field name
    let ext = file
        .content_type()
        .and_then(|ct| ct.extension())
        .map(|e| e.to_string().to_lowercase())
        .or_else(|| {
            file.raw_name().and_then(|rn| {
                let s = rn.dangerous_unsafe_unsanitized_raw().as_str().to_string();
                s.rsplit('.').next().map(|e| e.to_lowercase())
            })
        })
        .or_else(|| {
            file.name()
                .and_then(|n| n.rsplit('.').next())
                .map(|e| e.to_lowercase())
        })
        .unwrap_or_default();

    allowed_list.iter().any(|a| a.eq_ignore_ascii_case(&ext))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        dashboard::dashboard,
        posts::posts_list,
        posts::posts_new,
        posts::posts_edit,
        posts::posts_delete,
        posts::posts_create,
        posts::posts_update,
        portfolio::portfolio_list,
        portfolio::portfolio_new,
        portfolio::portfolio_edit,
        portfolio::portfolio_delete,
        portfolio::portfolio_create,
        portfolio::portfolio_update,
        comments::comments_list,
        comments::comment_approve,
        comments::comment_spam,
        comments::comment_delete,
        categories::categories_list,
        categories::category_create,
        categories::api_category_create,
        categories::category_update,
        categories::category_delete,
        categories::api_category_toggle_nav,
        categories::tags_list,
        categories::tag_delete,
        designs::designs_list,
        designs::design_activate,
        designs::design_overview,
        import::import_page,
        import::import_wordpress,
        import::import_velocty,
        settings::settings_page,
        settings::settings_save,
        media::media_library,
        media::media_delete,
        media::upload_image,
        media::upload_font,
        health::health_page,
        health::health_vacuum,
        health::health_wal_checkpoint,
        health::health_integrity_check,
        health::health_session_cleanup,
        health::health_orphan_scan,
        health::health_orphan_delete,
        health::health_unused_tags,
        health::health_analytics_prune,
        health::health_export_db,
        health::health_export_content,
        health::health_mongo_ping,
        users::mfa_setup,
        users::mfa_verify,
        users::mfa_disable,
        users::mfa_recovery_codes,
        sales::sales_dashboard,
        sales::sales_orders,
        firewall::firewall_dashboard,
        firewall::firewall_ban,
        firewall::firewall_unban,
        users::users_list,
        users::user_create,
        users::user_update,
        users::user_avatar_upload,
        users::user_lock,
        users::user_unlock,
        users::user_reset_password,
        users::user_delete,
    ]
}

pub fn api_routes() -> Vec<rocket::Route> {
    routes![
        api::stats_overview,
        api::stats_flow,
        api::stats_geo,
        api::stats_stream,
        api::stats_calendar,
        api::stats_top_portfolio,
        api::stats_top_referrers,
        api::stats_tags,
        api::set_theme,
        api::seo_check_post,
        api::seo_check_portfolio,
    ]
}
