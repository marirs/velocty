use crate::store::Store;

use super::html_escape;

/// Build meta tags HTML string for a page
pub fn build_meta(
    store: &dyn Store,
    title: Option<&str>,
    description: Option<&str>,
    path: &str,
) -> String {
    let site_name = store.setting_get_or("site_name", "Velocty");
    let site_url = store.setting_get_or("site_url", "http://localhost:8000");
    let title_template = store.setting_get_or("seo_title_template", "{{title}} — {{site_name}}");
    let default_desc = store.setting_get_or("seo_default_description", "");
    let og_enabled = store.setting_get_bool("seo_open_graph");
    let twitter_enabled = store.setting_get_bool("seo_twitter_cards");
    let canonical_base = store.setting_get_or("seo_canonical_base", &site_url);

    let page_title = match title {
        Some(t) => title_template
            .replace("{{title}}", t)
            .replace("{{site_name}}", &site_name),
        None => site_name.clone(),
    };

    let page_desc = description.unwrap_or(&default_desc);
    let canonical = format!("{}{}", canonical_base, path);

    let mut meta = String::new();

    // Basic meta
    meta.push_str(&format!(
        r#"<title>{}</title>
<meta name="description" content="{}">
<link rel="canonical" href="{}">"#,
        html_escape(&page_title),
        html_escape(page_desc),
        html_escape(&canonical),
    ));

    // Open Graph
    if og_enabled {
        meta.push_str(&format!(
            r#"
<meta property="og:title" content="{}">
<meta property="og:description" content="{}">
<meta property="og:url" content="{}">
<meta property="og:site_name" content="{}">
<meta property="og:type" content="website">"#,
            html_escape(&page_title),
            html_escape(page_desc),
            html_escape(&canonical),
            html_escape(&site_name),
        ));
    }

    // Twitter Cards
    if twitter_enabled {
        meta.push_str(&format!(
            r#"
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{}">
<meta name="twitter:description" content="{}">"#,
            html_escape(&page_title),
            html_escape(page_desc),
        ));
    }

    meta
}
