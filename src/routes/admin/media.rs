use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::{json, Value};

use super::admin_base;
use super::save_upload;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::settings::Setting;
use crate::security::auth::{AdminUser, AuthorUser, EditorUser};
use crate::AdminSlug;

// ── Media Library ───────────────────────────────────────

#[derive(serde::Serialize)]
pub struct MediaFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_human: String,
    pub ext: String,
    pub is_image: bool,
    pub media_type: String,
    pub modified: String,
}

#[get("/media?<page>&<filter>")]
pub fn media_library(
    _admin: EditorUser,
    slug: &State<AdminSlug>,
    pool: &State<DbPool>,
    page: Option<usize>,
    filter: Option<String>,
) -> Template {
    let upload_dir = std::path::Path::new("website/site/uploads");
    let per_page = 60usize;
    let current_page = page.unwrap_or(1).max(1);

    let mut files: Vec<MediaFile> = Vec::new();
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
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let meta = std::fs::metadata(&path).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.format("%Y-%m-%d %H:%M").to_string()
                })
                .unwrap_or_default();
            let is_image = matches!(
                ext.as_str(),
                "jpg"
                    | "jpeg"
                    | "png"
                    | "gif"
                    | "webp"
                    | "svg"
                    | "bmp"
                    | "tiff"
                    | "ico"
                    | "heic"
                    | "heif"
            );
            let media_type = match ext.as_str() {
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "ico"
                | "heic" | "heif" => "image",
                "mp4" | "webm" | "mov" | "avi" | "mkv" | "ogv" => "video",
                "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" => "audio",
                "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv"
                | "rtf" | "md" => "document",
                _ => "other",
            }
            .to_string();
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
                media_type,
                modified,
            });
        }
    }

    // Sort newest first
    files.sort_by(|a, b| b.modified.cmp(&a.modified));

    let count_all = files.len();
    let count_images = files.iter().filter(|f| f.media_type == "image").count();
    let count_videos = files.iter().filter(|f| f.media_type == "video").count();
    let count_audio = files.iter().filter(|f| f.media_type == "audio").count();
    let count_documents = files.iter().filter(|f| f.media_type == "document").count();
    let count_other = files.iter().filter(|f| f.media_type == "other").count();

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
        "admin_slug": slug.0,
        "files": page_files,
        "total": total,
        "filter": filter,
        "count_all": count_all,
        "count_images": count_images,
        "count_videos": count_videos,
        "count_audio": count_audio,
        "count_documents": count_documents,
        "count_other": count_other,
        "current_page": current_page,
        "total_pages": total_pages,
        "settings": Setting::all(pool),
    });

    Template::render("admin/media/list", &context)
}

#[post("/media/<filename>/delete")]
pub fn media_delete(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    filename: &str,
) -> Redirect {
    let path = std::path::Path::new("website/site/uploads").join(filename);
    if path.is_file() {
        let _ = std::fs::remove_file(&path);
        AuditEntry::log(
            pool,
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
    pool: &State<DbPool>,
    mut form: Form<ImageUploadForm<'_>>,
) -> Json<Value> {
    if !super::is_allowed_image(&form.file, pool) {
        return Json(json!({ "error": "File type not allowed" }));
    }
    match save_upload(&mut form.file, "editor", pool).await {
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
    pool: &State<DbPool>,
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
            let _ = Setting::set(pool, "font_custom_name", &font_name);
            let _ = Setting::set(pool, "font_custom_filename", &filename);
            Json(json!({ "success": true, "font_name": font_name, "filename": filename }))
        }
        Err(e) => Json(json!({ "error": format!("Upload failed: {}", e) })),
    }
}
