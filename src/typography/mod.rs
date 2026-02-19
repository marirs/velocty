use serde_json::Value;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build CSS custom properties for typography, layout, and color from settings.
pub fn build_css_variables(settings: &Value) -> String {
    let get = |key: &str, default: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };

    let font_primary = get("font_primary", "Inter");
    let font_heading = get("font_heading", "Inter");
    let sitewide = get("font_sitewide", "true") == "true";

    // Resolve per-element font: use specific if set, else fallback
    let resolve = |key: &str, fallback: &str| -> String {
        let v = get(key, "");
        if v.is_empty() {
            fallback.to_string()
        } else {
            v
        }
    };

    // Per-element fonts: use primary if sitewide, else use specific or fallback to primary
    let font_body = if sitewide {
        font_primary.clone()
    } else {
        resolve("font_body", &font_primary)
    };
    let font_headings = if sitewide {
        font_heading.clone()
    } else {
        resolve("font_headings", &font_heading)
    };
    let font_nav = if sitewide {
        font_primary.clone()
    } else {
        resolve("font_navigation", &font_primary)
    };
    let font_buttons = if sitewide {
        font_primary.clone()
    } else {
        resolve("font_buttons", &font_primary)
    };
    let font_captions = if sitewide {
        font_primary.clone()
    } else {
        resolve("font_captions", &font_primary)
    };

    // New per-element fonts (always resolve, fallback to primary/heading)
    let font_logo = resolve("font_logo", &font_primary);
    let font_subheading = resolve("font_subheading", &font_headings);
    let font_blockquote = resolve("font_blockquote", &font_body);
    let font_list = resolve("font_list", &font_body);
    let font_footer = resolve("font_footer", &font_primary);
    let font_lb_title = resolve("font_lightbox_title", &font_headings);
    let font_categories = resolve("font_categories", &font_primary);
    let font_tags = resolve("font_tags", &font_primary);

    let text_transform = get("font_text_transform", "none");
    let text_direction = get("font_text_direction", "ltr");
    let text_alignment = get("font_text_alignment", "left");

    format!(
        r#":root {{
    --font-primary: '{font_primary}', sans-serif;
    --font-heading: '{font_headings}', sans-serif;
    --font-body: '{font_body}', sans-serif;
    --font-nav: '{font_nav}', sans-serif;
    --font-buttons: '{font_buttons}', sans-serif;
    --font-captions: '{font_captions}', sans-serif;
    --font-logo: '{font_logo}', sans-serif;
    --font-subheading: '{font_subheading}', sans-serif;
    --font-blockquote: '{font_blockquote}', sans-serif;
    --font-list: '{font_list}', sans-serif;
    --font-footer: '{font_footer}', sans-serif;
    --font-lb-title: '{font_lb_title}', sans-serif;
    --font-categories: '{font_categories}', sans-serif;
    --font-tags: '{font_tags}', sans-serif;
    --font-size-body: {size_body};
    --font-size-h1: {size_h1};
    --font-size-h2: {size_h2};
    --font-size-h3: {size_h3};
    --font-size-h4: {size_h4};
    --font-size-h5: {size_h5};
    --font-size-h6: {size_h6};
    --font-size-logo: {size_logo};
    --font-size-nav: {size_nav};
    --font-size-blockquote: {size_blockquote};
    --font-size-list: {size_list};
    --font-size-footer: {size_footer};
    --font-size-lb-title: {size_lb_title};
    --font-size-categories: {size_categories};
    --font-size-tags: {size_tags};
    --line-height: {line_height};
    --text-transform: {text_transform};
    --text-direction: {text_direction};
    --text-alignment: {text_alignment};
    --sidebar-width: 250px;
    --sidebar-direction: {sidebar_direction};
    --content-margin-top: {margin_top};
    --content-margin-bottom: {margin_bottom};
    --content-margin-left: {margin_left};
    --content-margin-right: {margin_right};
    --content-max-width: {content_max_width};
    --grid-gap: 12px;
    --grid-columns: {grid_cols};
    --blog-grid-columns: {blog_grid_cols};
    --lightbox-border-color: {lb_border};
    --lightbox-title-color: {lb_title_color};
    --lightbox-tag-color: {lb_tag_color};
    --lightbox-nav-color: {lb_nav_color};
    --color-text: {color_text};
    --color-text-secondary: {color_text_secondary};
    --color-bg: {color_bg};
    --color-accent: {color_accent};
    --color-link: {color_link};
    --color-link-hover: {color_link_hover};
    --color-border: {color_border};
    --color-logo-text: {color_logo_text};
    --color-tagline: {color_tagline};
    --color-heading: {color_heading};
    --color-subheading: {color_subheading};
    --color-caption: {color_caption};
    --color-footer: {color_footer};
    --color-lightbox-categories: {color_lb_cats};
    --color-categories: {color_categories};
    --color-tags: {color_tags};
}}"#,
        color_text = get("site_text_color", "#111827"),
        color_text_secondary = get("site_text_secondary_color", "#6b7280"),
        color_bg = get("site_background_color", "#ffffff"),
        color_accent = get("site_accent_color", "#3b82f6"),
        color_link = get("color_link", "#3b82f6"),
        color_link_hover = get("color_link_hover", "#2563eb"),
        color_border = get("color_border", "#e5e7eb"),
        color_logo_text = get("color_logo_text", "#111827"),
        color_tagline = get("color_tagline", "#6b7280"),
        color_heading = get("color_heading", "#111827"),
        color_subheading = get("color_subheading", "#1f2937"),
        color_caption = get("color_caption", "#374151"),
        color_footer = get("color_footer", "#9ca3af"),
        color_lb_cats = get("color_lightbox_categories", "#AAAAAA"),
        color_categories = get("color_categories", "#6b7280"),
        color_tags = get("color_tags", "#6b7280"),
        font_primary = font_primary,
        font_headings = font_headings,
        font_body = font_body,
        font_nav = font_nav,
        font_buttons = font_buttons,
        font_captions = font_captions,
        font_logo = font_logo,
        font_subheading = font_subheading,
        font_blockquote = font_blockquote,
        font_list = font_list,
        font_footer = font_footer,
        font_lb_title = font_lb_title,
        font_categories = font_categories,
        font_tags = font_tags,
        size_body = get("font_size_body", "16px"),
        size_h1 = get("font_size_h1", "2.5rem"),
        size_h2 = get("font_size_h2", "2rem"),
        size_h3 = get("font_size_h3", "1.75rem"),
        size_h4 = get("font_size_h4", "1.5rem"),
        size_h5 = get("font_size_h5", "1.25rem"),
        size_h6 = get("font_size_h6", "1rem"),
        size_logo = get("font_size_logo", "1.5rem"),
        size_nav = get("font_size_nav", "14px"),
        size_blockquote = get("font_size_blockquote", "1.1rem"),
        size_list = get("font_size_list", "16px"),
        size_footer = get("font_size_footer", "12px"),
        size_lb_title = get("font_size_lightbox_title", "18px"),
        size_categories = get("font_size_categories", "13px"),
        size_tags = get("font_size_tags", "12px"),
        line_height = get("font_line_height", "1.6"),
        text_transform = text_transform,
        text_direction = text_direction,
        text_alignment = text_alignment,
        sidebar_direction = if get("layout_sidebar_position", "left") == "right" {
            "row-reverse"
        } else {
            "row"
        },
        margin_top = {
            let v = get("layout_margin_top", "0");
            if v == "0" {
                "0".to_string()
            } else {
                format!("{}px", v.trim_end_matches("px"))
            }
        },
        margin_bottom = {
            let v = get("layout_margin_bottom", "0");
            if v == "0" {
                "0".to_string()
            } else {
                format!("{}px", v.trim_end_matches("px"))
            }
        },
        margin_left = {
            let v = get("layout_margin_left", "0");
            if v == "0" {
                "0".to_string()
            } else {
                format!("{}px", v.trim_end_matches("px"))
            }
        },
        margin_right = {
            let v = get("layout_margin_right", "0");
            if v == "0" {
                "0".to_string()
            } else {
                format!("{}px", v.trim_end_matches("px"))
            }
        },
        content_max_width = if get("layout_content_boundary", "full") == "boxed" {
            "1200px"
        } else {
            "none"
        },
        grid_cols = get("portfolio_grid_columns", "3"),
        blog_grid_cols = get("blog_grid_columns", "3"),
        lb_border = get("portfolio_lightbox_border_color", "#D4A017"),
        lb_title_color = get("portfolio_lightbox_title_color", "#FFFFFF"),
        lb_tag_color = get("portfolio_lightbox_tag_color", "#AAAAAA"),
        lb_nav_color = get("portfolio_lightbox_nav_color", "#FFFFFF"),
    )
}

/// Build the font loading HTML tags (Google Fonts, Adobe Fonts, custom @font-face).
pub fn build_font_links(settings: &Value) -> String {
    let get = |key: &str| -> &str { settings.get(key).and_then(|v| v.as_str()).unwrap_or("") };

    let google_enabled = get("font_google_enabled") == "true";
    let adobe_enabled = get("font_adobe_enabled") == "true";
    let font_primary = get("font_primary");
    let font_heading = get("font_heading");
    let sitewide = get("font_sitewide") != "false";

    let mut html = String::new();

    // Collect all Google Font families that need loading
    if google_enabled {
        let system_fonts = ["system-ui", "Georgia, serif", "ui-monospace, monospace", ""];
        let mut families: Vec<String> = Vec::new();

        let mut maybe_add = |name: &str| {
            if !name.is_empty() && !system_fonts.contains(&name) && !name.starts_with("adobe-") {
                let family = name.replace(' ', "+");
                if !families.contains(&family) {
                    families.push(family);
                }
            }
        };

        maybe_add(font_primary);
        maybe_add(font_heading);

        if !sitewide {
            for key in &[
                "font_body",
                "font_headings",
                "font_navigation",
                "font_buttons",
                "font_captions",
            ] {
                maybe_add(get(key));
            }
        }

        // Always load per-element fonts if set (these are independent of sitewide toggle)
        for key in &[
            "font_logo",
            "font_subheading",
            "font_blockquote",
            "font_list",
            "font_footer",
            "font_lightbox_title",
            "font_categories",
            "font_tags",
        ] {
            maybe_add(get(key));
        }

        if !families.is_empty() {
            html.push_str(
                r#"    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
"#,
            );
            let params: Vec<String> = families
                .iter()
                .map(|f| format!("family={}:wght@300;400;500;600;700", f))
                .collect();
            html.push_str(&format!(
                r#"    <link href="https://fonts.googleapis.com/css2?{}&display=swap" rel="stylesheet">
"#,
                params.join("&")
            ));
        }
    }

    // Adobe Fonts
    if adobe_enabled {
        let project_id = get("font_adobe_project_id");
        if !project_id.is_empty() {
            html.push_str(&format!(
                r#"    <link rel="stylesheet" href="https://use.typekit.net/{}.css">
"#,
                html_escape(project_id)
            ));
        }
    }

    // Custom font @font-face
    let custom_name = get("font_custom_name");
    let custom_file = get("font_custom_filename");
    if !custom_name.is_empty() && !custom_file.is_empty() {
        let ext = custom_file.rsplit('.').next().unwrap_or("woff2");
        let format = match ext {
            "woff2" => "woff2",
            "woff" => "woff",
            "ttf" => "truetype",
            "otf" => "opentype",
            _ => "woff2",
        };
        html.push_str(&format!(
            r#"    <style>@font-face {{ font-family: '{}'; src: url('/uploads/fonts/{}') format('{}'); font-display: swap; }}</style>
"#,
            html_escape(custom_name),
            html_escape(custom_file),
            format,
        ));
    }

    html
}
