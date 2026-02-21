//! Oneguy journal renderers.
//! Dispatches to compact, classic, editorial, or grid based on settings.

pub mod grid;
pub mod list_classic;
pub mod list_compact;
pub mod list_editorial;

use serde_json::Value;

/// Dispatch blog list rendering for Oneguy based on `blog_display_type` and `blog_list_style`.
/// Returns `Some(html)` if a design-specific renderer handled it, `None` for the default fallback.
pub fn render_list(context: &Value) -> Option<String> {
    let settings = context.get("settings").cloned().unwrap_or_default();
    let list_style = settings
        .get("blog_list_style")
        .and_then(|v| v.as_str())
        .unwrap_or("compact");
    let display_type = settings
        .get("blog_display_type")
        .and_then(|v| v.as_str())
        .unwrap_or("grid");

    match display_type {
        "grid" => Some(grid::render_list(context)),
        "list" => match list_style {
            "compact" => Some(list_compact::render_list(context)),
            "editorial" => Some(list_editorial::render_list(context)),
            "wide" => Some(crate::designs::inkwell::journal::render_list(context)),
            _ => None, // classic and others fall through to default renderer
        },
        _ => None,
    }
}

/// Dispatch blog single rendering for Oneguy based on `blog_display_type` and `blog_list_style`.
/// Returns `Some(html)` if a design-specific renderer handled it, `None` for the default.
pub fn render_single(context: &Value) -> Option<String> {
    let settings = context.get("settings").cloned().unwrap_or_default();
    let display_type = settings
        .get("blog_display_type")
        .and_then(|v| v.as_str())
        .unwrap_or("grid");

    // Grid display type has its own single renderer
    if display_type == "grid" {
        return Some(grid::render_single(context));
    }

    // List display types dispatch by style
    let list_style = settings
        .get("blog_list_style")
        .and_then(|v| v.as_str())
        .unwrap_or("compact");

    match list_style {
        "classic" => Some(list_classic::render_single(context)),
        "editorial" => Some(list_editorial::render_single(context)),
        "wide" => Some(crate::designs::inkwell::journal::render_single(context)),
        _ => None, // fall through to default in render.rs
    }
}
