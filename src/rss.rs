use crate::db::DbPool;
use crate::models::post::Post;
use crate::models::settings::Setting;

/// Generate RSS 2.0 XML feed for published blog posts
pub fn generate_feed(pool: &DbPool) -> String {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    let site_tagline = Setting::get_or(pool, "site_caption", "");

    let blog_slug = Setting::get_or(pool, "blog_slug", "blog");
    let feed_count = Setting::get_or(pool, "rss_feed_count", "25")
        .parse::<i64>()
        .unwrap_or(25)
        .clamp(1, 100);

    let posts = Post::published(pool, feed_count, 0);

    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
<channel>
    <title>{title}</title>
    <link>{url}</link>
    <description>{desc}</description>
    <atom:link href="{url}/feed" rel="self" type="application/rss+xml"/>
    <language>en</language>
"#,
        title = xml_escape(&site_name),
        url = xml_escape(&site_url),
        desc = xml_escape(&site_tagline),
    );

    for post in &posts {
        let pub_date = post
            .published_at
            .map(|d| d.format("%a, %d %b %Y %H:%M:%S +0000").to_string())
            .unwrap_or_default();

        let excerpt = post.excerpt.as_deref().unwrap_or("");

        xml.push_str(&format!(
            r#"    <item>
        <title>{title}</title>
        <link>{url}/{blog_slug}/{slug}</link>
        <guid isPermaLink="true">{url}/{blog_slug}/{slug}</guid>
        <pubDate>{date}</pubDate>
        <description>{desc}</description>
    </item>
"#,
            title = xml_escape(&post.title),
            url = xml_escape(&site_url),
            blog_slug = &blog_slug,
            slug = &post.slug,
            date = pub_date,
            desc = xml_escape(excerpt),
        ));
    }

    xml.push_str("</channel>\n</rss>");
    xml
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
