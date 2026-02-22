use crate::store::Store;
use chrono::{DateTime, Utc};

/// Generate RSS 2.0 XML feed for published blog posts
pub fn generate_feed(store: &dyn Store) -> String {
    let site_name = store.setting_get_or("site_name", "Velocty");
    let site_url = store.setting_get_or("site_url", "http://localhost:8000");
    let site_tagline = store.setting_get_or("site_caption", "");
    let tz_name = store.setting_get_or("timezone", "UTC");

    let blog_slug = store.setting_get_or("blog_slug", "blog");
    let feed_count = store
        .setting_get_or("rss_feed_count", "25")
        .parse::<i64>()
        .unwrap_or(25)
        .clamp(1, 100);

    let posts = store.post_list(Some("published"), feed_count, 0);

    // Build date in the configured timezone (RFC 2822 format required by RSS spec)
    let format_rfc2822 = |ndt: chrono::NaiveDateTime| -> String {
        let utc: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
        if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
            utc.with_timezone(&tz)
                .format("%a, %d %b %Y %H:%M:%S %z")
                .to_string()
        } else {
            utc.format("%a, %d %b %Y %H:%M:%S +0000").to_string()
        }
    };

    let last_build = posts
        .first()
        .and_then(|p| p.published_at)
        .map(|d| format!("    <lastBuildDate>{}</lastBuildDate>\n", format_rfc2822(d)))
        .unwrap_or_default();

    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
<channel>
    <title>{title}</title>
    <link>{url}</link>
    <description>{desc}</description>
    <atom:link href="{url}/feed" rel="self" type="application/rss+xml"/>
    <language>en</language>
{last_build}"#,
        title = xml_escape(&site_name),
        url = xml_escape(&site_url),
        desc = xml_escape(&site_tagline),
        last_build = last_build,
    );

    for post in &posts {
        let pub_date = post.published_at.map(&format_rfc2822).unwrap_or_default();

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
