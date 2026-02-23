use std::sync::Arc;

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::{json, Value};

use super::admin_base;
use super::save_upload;
use crate::security::auth::{AdminUser, AuthorUser, EditorUser};
use crate::store::Store;
use crate::AdminSlug;

// ── Media Library ───────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct MediaFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_human: String,
    pub ext: String,
    pub is_image: bool,
    pub is_video: bool,
    pub media_type: String,
    pub modified: String,
}

/// Scan the uploads directory and return all media files sorted newest-first,
/// along with total disk usage in bytes.
pub(crate) fn scan_media_files(store: &dyn Store) -> (Vec<MediaFile>, u64) {
    let upload_dir = std::path::Path::new("website/site/uploads");
    let img_allowed =
        store.setting_get_or("images_allowed_types", "jpg,jpeg,png,gif,webp,svg,tiff");
    let img_exts: Vec<String> = img_allowed
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();
    let vid_allowed = store.setting_get_or("video_allowed_types", "mp4,webm,mov,avi,mkv");
    let vid_exts: Vec<String> = vid_allowed
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();

    let mut files: Vec<MediaFile> = Vec::new();
    let mut total_disk_bytes: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(upload_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') {
                continue;
            }
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let is_image = img_exts.iter().any(|e| e == &ext);
            let is_video = vid_exts.iter().any(|e| e == &ext);
            if !is_image && !is_video {
                continue;
            }
            let meta = std::fs::metadata(&path).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            total_disk_bytes += size;
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.format("%Y-%m-%d %H:%M").to_string()
                })
                .unwrap_or_default();
            let media_type = if is_image { "image" } else { "video" }.to_string();
            let size_human = if size >= 1_048_576 {
                format!("{:.1} MB", size as f64 / 1_048_576.0)
            } else if size >= 1024 {
                format!("{:.0} KB", size as f64 / 1024.0)
            } else {
                format!("{} B", size)
            };
            files.push(MediaFile {
                path: name.clone(),
                name,
                size,
                size_human,
                ext,
                is_image,
                is_video,
                media_type,
                modified,
            });
        }
    }

    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    (files, total_disk_bytes)
}

#[get("/media?<page>&<filter>")]
pub fn media_library(
    _admin: EditorUser,
    slug: &State<AdminSlug>,
    store: &State<Arc<dyn Store>>,
    page: Option<usize>,
    filter: Option<String>,
) -> Template {
    let per_page = 60usize;
    let current_page = page.unwrap_or(1).max(1);

    let (files, total_disk_bytes) = scan_media_files(&**store.inner());

    let count_all = files.len();
    let count_images = files.iter().filter(|f| f.media_type == "image").count();
    let count_videos = files.iter().filter(|f| f.media_type == "video").count();

    let disk_used = if total_disk_bytes >= 1_073_741_824 {
        format!("{:.1} GB", total_disk_bytes as f64 / 1_073_741_824.0)
    } else if total_disk_bytes >= 1_048_576 {
        format!("{:.1} MB", total_disk_bytes as f64 / 1_048_576.0)
    } else if total_disk_bytes >= 1024 {
        format!("{:.0} KB", total_disk_bytes as f64 / 1024.0)
    } else {
        format!("{} B", total_disk_bytes)
    };
    let disk_capacity_bytes: u64 = 1_073_741_824;
    let disk_pct = ((total_disk_bytes as f64 / disk_capacity_bytes as f64) * 100.0)
        .min(100.0)
        .round() as u32;

    let filtered: Vec<&MediaFile> = match filter.as_deref() {
        Some(f) if !f.is_empty() => files.iter().filter(|file| file.media_type == f).collect(),
        _ => files.iter().collect(),
    };

    let total = filtered.len();
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as usize;
    let offset = (current_page - 1) * per_page;
    let page_files: Vec<&&MediaFile> = filtered.iter().skip(offset).take(per_page).collect();

    let context = json!({
        "page_title": "Media",
        "admin_slug": slug.get(),
        "files": page_files,
        "total": total,
        "filter": filter,
        "count_all": count_all,
        "count_images": count_images,
        "count_videos": count_videos,
        "disk_used": disk_used,
        "disk_pct": disk_pct,
        "current_page": current_page,
        "total_pages": total_pages,
        "settings": store.setting_all(),
    });

    Template::render("admin/media/list", &context)
}

// ── Media Library JSON API (for modal picker) ───────────

#[get("/api/media?<page>&<filter>")]
pub fn api_media_list(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    page: Option<usize>,
    filter: Option<String>,
) -> Json<Value> {
    let per_page = 60usize;
    let current_page = page.unwrap_or(1).max(1);

    let (files, _) = scan_media_files(&**store.inner());

    let filtered: Vec<&MediaFile> = match filter.as_deref() {
        Some(f) if !f.is_empty() => files.iter().filter(|file| file.media_type == f).collect(),
        _ => files.iter().collect(),
    };

    let total = filtered.len();
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as usize;
    let offset = (current_page - 1) * per_page;
    let page_files: Vec<&MediaFile> = filtered
        .iter()
        .skip(offset)
        .take(per_page)
        .copied()
        .collect();

    Json(json!({
        "files": page_files,
        "total": total,
        "current_page": current_page,
        "total_pages": total_pages,
    }))
}

#[post("/media/<filename>/delete")]
pub fn media_delete(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    filename: &str,
) -> Redirect {
    let path = std::path::Path::new("website/site/uploads").join(filename);
    if path.is_file() {
        let _ = std::fs::remove_file(&path);
        store.audit_log(
            Some(_admin.user.id),
            Some(&_admin.user.display_name),
            "delete",
            Some("media"),
            None,
            Some(filename),
            None,
            None,
        );
    }
    Redirect::to(format!("{}/media", admin_base(slug)))
}

// ── Image Upload API (for TinyMCE) ─────────────────────

#[derive(FromForm)]
pub struct ImageUploadForm<'f> {
    pub file: TempFile<'f>,
}

#[post("/upload/image", data = "<form>")]
pub async fn upload_image(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    mut form: Form<ImageUploadForm<'_>>,
) -> Json<Value> {
    if !super::is_allowed_media(&form.file, &**store.inner()) {
        return Json(json!({ "error": "File type not allowed" }));
    }
    match save_upload(&mut form.file, "editor", &**store.inner()).await {
        Some(filename) => Json(json!({ "location": format!("/uploads/{}", filename) })),
        None => Json(json!({ "error": "Upload failed" })),
    }
}

// ── Font Upload API ─────────────────────────────────────

#[derive(FromForm)]
pub struct FontUploadForm<'f> {
    pub file: TempFile<'f>,
    pub font_name: String,
}

#[post("/upload/font", data = "<form>")]
pub async fn upload_font(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    mut form: Form<FontUploadForm<'_>>,
) -> Json<Value> {
    let font_name = form.font_name.trim().to_string();
    if font_name.is_empty() {
        return Json(json!({ "error": "Font name is required" }));
    }

    let raw_name = form
        .file
        .raw_name()
        .map(|n| n.dangerous_unsafe_unsanitized_raw().to_string())
        .unwrap_or_default();
    let ext = raw_name
        .rsplit('.')
        .next()
        .unwrap_or("woff2")
        .to_lowercase();
    let valid_exts = ["woff2", "woff", "ttf", "otf"];
    if !valid_exts.contains(&ext.as_str()) {
        return Json(
            json!({ "error": "Invalid font file type. Use .woff2, .woff, .ttf, or .otf" }),
        );
    }

    let filename = format!(
        "{}_{}.{}",
        font_name.to_lowercase().replace(' ', "-"),
        uuid::Uuid::new_v4(),
        ext
    );
    let fonts_dir = std::path::Path::new("website/site/uploads/fonts");
    let _ = std::fs::create_dir_all(fonts_dir);
    let dest = fonts_dir.join(&filename);

    match form.file.persist_to(&dest).await {
        Ok(_) => {
            let _ = store.setting_set("font_custom_name", &font_name);
            let _ = store.setting_set("font_custom_filename", &filename);
            Json(json!({ "success": true, "font_name": font_name, "filename": filename }))
        }
        Err(e) => Json(json!({ "error": format!("Upload failed: {}", e) })),
    }
}
