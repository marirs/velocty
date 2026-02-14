use image::imageops::FilterType;
use image::GenericImageView;
use std::fs;
use std::path::Path;

use crate::db::DbPool;
use crate::models::settings::Setting;

/// Result of processing an uploaded image
pub struct ProcessedImage {
    pub original_path: String,
    pub thumbnail_path: String,
}

/// Save an uploaded image, generate thumbnails, optionally convert to WebP
pub fn process_upload(
    pool: &DbPool,
    file_bytes: &[u8],
    original_filename: &str,
) -> Result<ProcessedImage, String> {
    let storage_path = Setting::get_or(pool, "images_storage_path", "website/site/uploads/");
    let quality = Setting::get_i64(pool, "images_quality") as u8;
    let quality = if quality == 0 { 85 } else { quality };

    // Parse thumbnail sizes
    let thumb_medium = Setting::get_or(pool, "images_thumb_medium", "300x300");
    let (thumb_w, thumb_h) = parse_dimensions(&thumb_medium);

    // Generate unique filename
    let ext = Path::new(original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let unique_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let thumb_name = format!("thumb_{}", unique_name);

    // Ensure upload directory exists
    fs::create_dir_all(&storage_path).map_err(|e| e.to_string())?;

    let original_full = format!("{}{}", storage_path, unique_name);
    let thumb_full = format!("{}{}", storage_path, thumb_name);

    // Save original
    fs::write(&original_full, file_bytes).map_err(|e| e.to_string())?;

    // Generate thumbnail
    let img = image::load_from_memory(file_bytes).map_err(|e| e.to_string())?;
    let thumbnail = img.resize(thumb_w, thumb_h, FilterType::Lanczos3);
    thumbnail.save(&thumb_full).map_err(|e| e.to_string())?;

    // WebP conversion (if enabled)
    let webp_enabled = Setting::get_bool(pool, "images_webp_convert");
    if webp_enabled {
        let webp_name = format!("{}.webp", uuid::Uuid::new_v4());
        let webp_full = format!("{}{}", storage_path, webp_name);
        convert_to_webp(&original_full, &webp_full, quality)?;
    }

    Ok(ProcessedImage {
        original_path: unique_name,
        thumbnail_path: thumb_name,
    })
}

/// Generate multiple thumbnail sizes for an image
pub fn generate_thumbnails(
    pool: &DbPool,
    image_path: &str,
) -> Result<Vec<(String, String)>, String> {
    let storage_path = Setting::get_or(pool, "images_storage_path", "website/site/uploads/");
    let full_path = format!("{}{}", storage_path, image_path);

    let img = image::open(&full_path).map_err(|e| e.to_string())?;

    let sizes = vec![
        ("small", Setting::get_or(pool, "images_thumb_small", "150x150")),
        ("medium", Setting::get_or(pool, "images_thumb_medium", "300x300")),
        ("large", Setting::get_or(pool, "images_thumb_large", "1024x1024")),
    ];

    let mut results = Vec::new();

    for (label, dim_str) in sizes {
        let (w, h) = parse_dimensions(&dim_str);
        let thumb = img.resize(w, h, FilterType::Lanczos3);
        let thumb_name = format!("{}_{}", label, image_path);
        let thumb_path = format!("{}{}", storage_path, thumb_name);
        thumb.save(&thumb_path).map_err(|e| e.to_string())?;
        results.push((label.to_string(), thumb_name));
    }

    Ok(results)
}

fn convert_to_webp(input_path: &str, output_path: &str, quality: u8) -> Result<(), String> {
    let img = image::open(input_path).map_err(|e| e.to_string())?;
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();

    let encoder = webp::Encoder::from_rgba(&rgba, w, h);
    let webp_data = encoder.encode(quality as f32);

    fs::write(output_path, &*webp_data).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_dimensions(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().unwrap_or(300);
        let h = parts[1].parse().unwrap_or(300);
        (w, h)
    } else {
        (300, 300)
    }
}

/// Check if file size is within the configured limit
pub fn check_file_size(pool: &DbPool, size_bytes: usize) -> bool {
    let max_mb = Setting::get_i64(pool, "images_max_upload_mb").max(1) as usize;
    size_bytes <= max_mb * 1024 * 1024
}

/// Delete an image and its thumbnails
pub fn delete_image(pool: &DbPool, image_path: &str) -> Result<(), String> {
    let storage_path = Setting::get_or(pool, "images_storage_path", "website/site/uploads/");

    let full_path = format!("{}{}", storage_path, image_path);
    let _ = fs::remove_file(&full_path);

    // Remove thumbnails
    for prefix in &["thumb_", "small_", "medium_", "large_"] {
        let thumb_path = format!("{}{}{}", storage_path, prefix, image_path);
        let _ = fs::remove_file(&thumb_path);
    }

    // Remove WebP version
    let webp_path = full_path.replace(
        Path::new(&full_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
        "webp",
    );
    let _ = fs::remove_file(&webp_path);

    Ok(())
}
