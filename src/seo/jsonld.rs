use crate::db::DbPool;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::Setting;
use chrono::{DateTime, Utc};

use super::json_escape;

/// Format a NaiveDateTime as ISO 8601 with timezone offset for schema.org
fn format_iso8601(ndt: chrono::NaiveDateTime, tz_name: &str) -> String {
    let utc: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
    if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
        utc.with_timezone(&tz)
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    } else {
        utc.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    }
}

/// Build JSON-LD structured data for a blog post
pub fn build_post_jsonld(pool: &DbPool, post: &Post) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let blog_slug = Setting::get_or(pool, "blog_slug", "journal");
    let tz_name = Setting::get_or(pool, "timezone", "UTC");

    let published = post
        .published_at
        .map(|d| format_iso8601(d, &tz_name))
        .unwrap_or_default();

    let modified = format_iso8601(post.updated_at, &tz_name);

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
