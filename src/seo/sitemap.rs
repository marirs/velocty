use crate::store::Store;

/// Generate sitemap.xml content.
/// Returns None if seo_sitemap_enabled is false.
pub fn generate_sitemap(store: &dyn Store) -> Option<String> {
    if !store.setting_get_bool("seo_sitemap_enabled") {
        return None;
    }

    let site_url = store.setting_get_or("site_url", "http://localhost:8000");
    let blog_slug = store.setting_get_or("blog_slug", "journal");
    let portfolio_slug = store.setting_get_or("portfolio_slug", "portfolio");

    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );

    // Homepage
    xml.push_str(&format!(
        "  <url><loc>{}</loc><changefreq>daily</changefreq><priority>1.0</priority></url>\n",
        site_url
    ));

    // Blog index
    xml.push_str(&format!(
        "  <url><loc>{}/{}</loc><changefreq>daily</changefreq><priority>0.8</priority></url>\n",
        site_url, blog_slug
    ));

    // Portfolio index
    xml.push_str(&format!(
        "  <url><loc>{}/{}</loc><changefreq>daily</changefreq><priority>0.8</priority></url>\n",
        site_url, portfolio_slug
    ));

    // Published posts
    let posts = store.post_list(Some("published"), 1000, 0);
    for post in &posts {
        let lastmod = post.updated_at.format("%Y-%m-%d").to_string();
        xml.push_str(&format!(
            "  <url><loc>{}/{}/{}</loc><lastmod>{}</lastmod><priority>0.6</priority></url>\n",
            site_url, blog_slug, post.slug, lastmod
        ));
    }

    // Published portfolio items
    let items = store.portfolio_list(Some("published"), 1000, 0);
    for item in &items {
        let lastmod = item.updated_at.format("%Y-%m-%d").to_string();
        xml.push_str(&format!(
            "  <url><loc>{}/{}/{}</loc><lastmod>{}</lastmod><priority>0.6</priority></url>\n",
            site_url, portfolio_slug, item.slug, lastmod
        ));
    }

    xml.push_str("</urlset>");
    Some(xml)
}

/// Generate robots.txt content with dynamic sitemap URL.
pub fn generate_robots(store: &dyn Store) -> String {
    let mut content = store.setting_get_or("seo_robots_txt", "User-agent: *\nAllow: /");
    let site_url = store.setting_get_or("site_url", "http://localhost:8000");
    if store.setting_get_bool("seo_sitemap_enabled") {
        content.push_str(&format!("\nSitemap: {}/sitemap.xml", site_url));
    }
    content
}
