use crate::db::DbPool;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::Setting;

/// Build meta tags HTML string for a page
pub fn build_meta(
    pool: &DbPool,
    title: Option<&str>,
    description: Option<&str>,
    path: &str,
) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let title_template = Setting::get_or(pool, "seo_title_template", "{{title}} — {{site_name}}");
    let default_desc = Setting::get_or(pool, "seo_default_description", "");
    let og_enabled = Setting::get_bool(pool, "seo_open_graph");
    let twitter_enabled = Setting::get_bool(pool, "seo_twitter_cards");
    let canonical_base = Setting::get_or(pool, "seo_canonical_base", &site_url);

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

/// Build JSON-LD structured data for a blog post
pub fn build_post_jsonld(pool: &DbPool, post: &Post) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");

    let published = post
        .published_at
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default();

    let modified = post.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string();

    format!(
        r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "BlogPosting",
    "headline": "{}",
    "description": "{}",
    "url": "{}/blog/{}",
    "datePublished": "{}",
    "dateModified": "{}",
    "publisher": {{
        "@type": "Organization",
        "name": "{}"
    }}
}}
</script>"#,
        json_escape(&post.title),
        json_escape(post.meta_description.as_deref().unwrap_or("")),
        site_url,
        post.slug,
        published,
        modified,
        json_escape(&site_name),
    )
}

/// Build JSON-LD structured data for a portfolio item
pub fn build_portfolio_jsonld(pool: &DbPool, item: &PortfolioItem) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");

    format!(
        r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "ImageObject",
    "name": "{}",
    "description": "{}",
    "contentUrl": "{}/uploads/{}",
    "url": "{}/portfolio/{}",
    "publisher": {{
        "@type": "Organization",
        "name": "{}"
    }}
}}
</script>"#,
        json_escape(&item.title),
        json_escape(item.meta_description.as_deref().unwrap_or("")),
        site_url,
        item.image_path,
        site_url,
        item.slug,
        json_escape(&site_name),
    )
}

/// Generate sitemap.xml
pub fn generate_sitemap(pool: &DbPool) -> String {
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");

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
        "  <url><loc>{}/blog</loc><changefreq>daily</changefreq><priority>0.8</priority></url>\n",
        site_url
    ));

    // Portfolio index
    xml.push_str(&format!(
        "  <url><loc>{}/portfolio</loc><changefreq>daily</changefreq><priority>0.8</priority></url>\n",
        site_url
    ));

    // Published posts
    let posts = Post::published(pool, 1000, 0);
    for post in &posts {
        let lastmod = post.updated_at.format("%Y-%m-%d").to_string();
        xml.push_str(&format!(
            "  <url><loc>{}/blog/{}</loc><lastmod>{}</lastmod><priority>0.6</priority></url>\n",
            site_url, post.slug, lastmod
        ));
    }

    // Published portfolio items
    let items = PortfolioItem::published(pool, 1000, 0);
    for item in &items {
        let lastmod = item.updated_at.format("%Y-%m-%d").to_string();
        xml.push_str(&format!(
            "  <url><loc>{}/portfolio/{}</loc><lastmod>{}</lastmod><priority>0.6</priority></url>\n",
            site_url, item.slug, lastmod
        ));
    }

    xml.push_str("</urlset>");
    xml
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
