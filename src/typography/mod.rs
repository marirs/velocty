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

    // Per-element fonts: use primary if sitewide, else use specific or fallback to primary
    let font_body = if sitewide { font_primary.clone() } else {
        let v = get("font_body", ""); if v.is_empty() { font_primary.clone() } else { v }
    };
    let font_headings = if sitewide { font_heading.clone() } else {
        let v = get("font_headings", ""); if v.is_empty() { font_heading.clone() } else { v }
    };
    let font_nav = if sitewide { font_primary.clone() } else {
        let v = get("font_navigation", ""); if v.is_empty() { font_primary.clone() } else { v }
    };
    let font_buttons = if sitewide { font_primary.clone() } else {
        let v = get("font_buttons", ""); if v.is_empty() { font_primary.clone() } else { v }
    };
    let font_captions = if sitewide { font_primary.clone() } else {
        let v = get("font_captions", ""); if v.is_empty() { font_primary.clone() } else { v }
    };

    let text_transform = get("font_text_transform", "none");

    format!(
        r#":root {{
    --font-primary: '{font_primary}', sans-serif;
    --font-heading: '{font_headings}', sans-serif;
    --font-body: '{font_body}', sans-serif;
    --font-nav: '{font_nav}', sans-serif;
    --font-buttons: '{font_buttons}', sans-serif;
    --font-captions: '{font_captions}', sans-serif;
    --font-size-body: {size_body};
    --font-size-h1: {size_h1};
    --font-size-h2: {size_h2};
    --font-size-h3: {size_h3};
    --font-size-h4: {size_h4};
    --font-size-h5: {size_h5};
    --font-size-h6: {size_h6};
    --text-transform: {text_transform};
    --sidebar-width: 250px;
    --grid-gap: 8px;
    --grid-columns: {grid_cols};
    --lightbox-border-color: {lb_border};
    --color-text: #111827;
    --color-text-secondary: #6b7280;
    --color-bg: #ffffff;
    --color-accent: #3b82f6;
}}"#,
        font_primary = font_primary,
        font_headings = font_headings,
        font_body = font_body,
        font_nav = font_nav,
        font_buttons = font_buttons,
        font_captions = font_captions,
        size_body = get("font_size_body", "16px"),
        size_h1 = get("font_size_h1", "2.5rem"),
        size_h2 = get("font_size_h2", "2rem"),
        size_h3 = get("font_size_h3", "1.75rem"),
        size_h4 = get("font_size_h4", "1.5rem"),
        size_h5 = get("font_size_h5", "1.25rem"),
        size_h6 = get("font_size_h6", "1rem"),
        text_transform = text_transform,
        grid_cols = get("portfolio_grid_columns", "3"),
        lb_border = get("portfolio_lightbox_border_color", "#D4A017"),
    )
}

/// Build the font loading HTML tags (Google Fonts, Adobe Fonts, custom @font-face).
pub fn build_font_links(settings: &Value) -> String {
    let get = |key: &str| -> &str {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("")
    };

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
            for key in &["font_body", "font_headings", "font_navigation", "font_buttons", "font_captions"] {
                maybe_add(get(key));
            }
        }

        if !families.is_empty() {
            html.push_str(r#"    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
"#);
            let params: Vec<String> = families.iter().map(|f| format!("family={}:wght@300;400;500;600;700", f)).collect();
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
