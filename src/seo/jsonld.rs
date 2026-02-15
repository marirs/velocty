use crate::db::DbPool;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::Setting;

use super::json_escape;

/// Build JSON-LD structured data for a blog post
pub fn build_post_jsonld(pool: &DbPool, post: &Post) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let blog_slug = Setting::get_or(pool, "blog_slug", "journal");

    let published = post
        .published_at
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default();

    let modified = post.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string();

    let mut ld = format!(
        r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "BlogPosting",
    "headline": "{}",
    "description": "{}",
    "url": "{}/{}/{}",
    "datePublished": "{}",
    "dateModified": "{}",
    "publisher": {{
        "@type": "Organization",
        "name": "{}"
    }}"#,
        json_escape(&post.title),
        json_escape(post.meta_description.as_deref().unwrap_or("")),
        site_url,
        blog_slug,
        post.slug,
        published,
        modified,
        json_escape(&site_name),
    );

    // Add featured image if present
    if let Some(ref img) = post.featured_image {
        if !img.is_empty() {
            ld.push_str(&format!(
                r#",
    "image": "{}/uploads/{}"
"#,
                site_url,
                json_escape(img)
            ));
        }
    }

    ld.push_str("\n}\n</script>");
    ld
}

/// Build JSON-LD structured data for a portfolio item
pub fn build_portfolio_jsonld(pool: &DbPool, item: &PortfolioItem) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let portfolio_slug = Setting::get_or(pool, "portfolio_slug", "portfolio");

    format!(
        r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "ImageObject",
    "name": "{}",
    "description": "{}",
    "contentUrl": "{}/uploads/{}",
    "url": "{}/{}/{}",
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
        portfolio_slug,
        item.slug,
        json_escape(&site_name),
    )
}
