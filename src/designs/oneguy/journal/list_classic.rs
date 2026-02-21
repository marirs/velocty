use serde_json::Value;

use crate::designs::common::{author_initials, build_classic_comments, name_hue};
use crate::render::{
    build_share_buttons, count_words_html, format_date, format_date_iso8601, html_escape,
};

/// Render the blog single page in the Classic style.
/// Layout: full-width featured image → title → content → meta → tags → written by → comments.
pub fn render_single(context: &Value) -> String {
    let post = match context.get("post") {
        Some(p) => p,
        None => return String::new(),
    };
    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let show_author = sg("blog_show_author", "true") == "true";
    let show_date = sg("blog_show_date", "true") == "true";
    let show_reading_time = sg("blog_show_reading_time", "true") == "true";

    let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let content = post
        .get("content_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let raw_date = post
        .get("published_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let date = format_date(raw_date, &settings);
    let featured = post
        .get("featured_image")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let author = post
        .get("author_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let word_count = count_words_html(content);
    let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;

    let blog_slug = sg("blog_slug", "journal");
    let post_slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let site_url = sg("site_url", "");
    let page_url = if !site_url.is_empty() {
        format!("{}/{}/{}", site_url, blog_slug, post_slug)
    } else {
        String::new()
    };
    let share_pos = sg("share_icons_position", "below_content");

    let mut html = String::from("<article class=\"blog-single-classic\">");

    // Full-width featured image
    if !featured.is_empty() {
        html.push_str(&format!(
            "<div class=\"bsc-hero\"><img src=\"/uploads/{}\" alt=\"{}\"></div>",
            featured,
            html_escape(title)
        ));
        if share_pos == "below_image" && !page_url.is_empty() {
            html.push_str("<div class=\"bsc-content-wrap\">");
            html.push_str(&build_share_buttons(&settings, &page_url, title));
            html.push_str("</div>");
        }
    }

    // Title
    html.push_str(&format!(
        "<div class=\"bsc-content-wrap\"><h1 class=\"bsc-title\">{}</h1>",
        html_escape(title)
    ));

    // Meta line (date / reading time) — below title, before content
    let mut meta_parts: Vec<String> = Vec::new();
    if show_date && !date.is_empty() {
        meta_parts.push(format!("<span class=\"bsc-date\">{}</span>", date));
    }
    if show_reading_time && word_count > 0 {
        meta_parts.push(format!(
            "<span class=\"bsc-reading-time\">{} min read</span>",
            reading_time
        ));
    }
    if !meta_parts.is_empty() {
        html.push_str(&format!(
            "<div class=\"bsc-meta\">{}</div>",
            meta_parts.join(" / ")
        ));
    }

    // Content
    html.push_str(&format!("<div class=\"bsc-content\">{}</div>", content));

    // Tags (plain text, not links)
    if let Some(Value::Array(tags)) = context.get("tags") {
        if !tags.is_empty() {
            html.push_str("<div class=\"bsc-tags\">");
            let tag_strs: Vec<String> = tags
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(|v| v.as_str())?;
                    Some(format!(
                        "<span class=\"bsc-tag\">#{}</span>",
                        html_escape(name)
                    ))
                })
                .collect();
            html.push_str(&tag_strs.join(" "));
            html.push_str("</div>");
        }
    }

    // Share buttons — below content
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // "Written by" section (only if show_author is enabled)
    if show_author && !author.is_empty() {
        let written_by_label = sg("blog_written_by_label", "By");
        let initials = author_initials(author);
        let hue = name_hue(author);
        html.push_str(&format!(
            "<div class=\"bsc-author-box\">\
             <div class=\"bsc-author-avatar\" style=\"background:hsl({},45%,55%)\">{}</div>\
             <div class=\"bsc-author-info\">{}<br><strong>{}</strong></div>\
             </div>",
            hue,
            html_escape(&initials),
            html_escape(&written_by_label),
            html_escape(author),
        ));
    }

    // Prev / Next navigation
    let mut nav_html = String::new();
    if let Some(prev) = context.get("prev_post") {
        let prev_title = prev.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let prev_slug = prev.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        nav_html.push_str(&format!(
            "<a href=\"/{}/{}\">&larr; {}</a>",
            blog_slug,
            prev_slug,
            html_escape(prev_title)
        ));
    } else {
        nav_html.push_str("<span></span>");
    }
    if let Some(next) = context.get("next_post") {
        let next_title = next.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let next_slug = next.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        nav_html.push_str(&format!(
            "<a href=\"/{}/{}\">{} &rarr;</a>",
            blog_slug,
            next_slug,
            html_escape(next_title)
        ));
    }
    if !nav_html.is_empty() {
        html.push_str(&format!("<nav class=\"post-nav\">{}</nav>", nav_html));
    }

    // Comments
    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_classic_comments(context, &settings, post_id));
    }

    html.push_str("</div>"); // close bsc-content-wrap
    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = sg("site_name", "Velocty");
        let site_url_ld = sg("site_url", "http://localhost:8000");
        let slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let headline = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let desc = post
            .get("meta_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let raw_pub = post
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let raw_mod = post
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let published = format_date_iso8601(raw_pub, &settings);
        let modified = format_date_iso8601(raw_mod, &settings);
        let image = post
            .get("featured_image")
            .and_then(|v| v.as_str())
            .unwrap_or("");
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
    "publisher": {{ "@type": "Organization", "name": "{}" }}"#,
            html_escape(headline),
            html_escape(desc),
            site_url_ld,
            blog_slug,
            slug,
            published,
            modified,
            html_escape(&site_name),
        );
        if !image.is_empty() {
            ld.push_str(&format!(
                ",\n    \"image\": \"{}/uploads/{}\"",
                site_url_ld, image
            ));
        }
        if !author.is_empty() {
            ld.push_str(&format!(
                ",\n    \"author\": {{ \"@type\": \"Person\", \"name\": \"{}\" }}",
                html_escape(author)
            ));
        }
        ld.push_str("\n}\n</script>");
        html.push_str(&ld);
    }

    html
}

/// CSS for the Classic single page design.
pub fn css() -> &'static str {
    r#"
/* Classic single page */
.blog-single-classic {
    max-width: 100%;
}
.bsc-hero {
    max-width: clamp(780px, 70%, 1200px);
    margin: 0 0 40px 24px;
    overflow: hidden;
}
.bsc-hero img {
    width: 100%;
    height: auto;
    display: block;
    object-fit: cover;
}
.bsc-content-wrap {
    max-width: clamp(780px, 70%, 1200px);
    margin: 0 0 0 24px;
    padding: 0 20px;
}
.bsc-title {
    font-size: 32px;
    font-weight: 700;
    line-height: 1.3;
    margin-bottom: 32px;
    color: var(--color-text);
}
.bsc-content {
    font-size: 16px;
    line-height: 1.8;
    color: var(--color-text);
    margin-bottom: 40px;
}
.bsc-content p {
    margin-bottom: 1.5em;
}
.bsc-content blockquote {
    border-left: 3px solid var(--color-text);
    margin: 1.5em 0;
    padding: 1em 1.5em;
    font-style: italic;
    color: var(--color-text-secondary);
}
.bsc-content img {
    max-width: 100%;
    height: auto;
}
.bsc-meta {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary);
    margin-bottom: 28px;
}
.bsc-tags {
    margin-bottom: 32px;
}
.bsc-tag {
    display: inline-block;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-secondary);
    margin-right: 12px;
}

/* Written by author box */
.bsc-author-box {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 24px 0;
    border-top: 1px solid rgba(0,0,0,0.08);
    margin-bottom: 40px;
}
.bsc-author-avatar {
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 700;
    font-size: 16px;
    flex-shrink: 0;
}
.bsc-author-info {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.5;
}
.bsc-author-info strong {
    color: var(--color-text);
    font-size: 15px;
}

/* Classic comments */
.bsc-comments {
    margin-bottom: 40px;
}
.bsc-comments h3 {
    font-size: 18px;
    font-weight: 700;
    margin-bottom: 24px;
    color: var(--color-text);
}
.bsc-comment {
    display: flex;
    gap: 14px;
    margin-bottom: 24px;
    padding-bottom: 24px;
    border-bottom: 1px solid rgba(0,0,0,0.06);
}
.bsc-comment:last-child {
    border-bottom: none;
}
.bsc-comment-avatar {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 700;
    font-size: 14px;
    flex-shrink: 0;
}
.bsc-comment-body {
    flex: 1;
    min-width: 0;
}
.bsc-comment-header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 6px;
}
.bsc-comment-name {
    font-size: 14px;
    color: var(--color-text);
}
.bsc-comment-date {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-secondary);
}
.bsc-comment-body p {
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-text);
    margin: 0 0 6px 0;
}
.bsc-comment .reply-btn {
    font-size: 12px;
    color: var(--color-accent);
    text-decoration: none;
}
.bsc-comment .reply-btn:hover {
    text-decoration: underline;
}

/* Classic comment form */
.bsc-comment-form {
    border-top: 1px solid rgba(0,0,0,0.08);
    padding-top: 32px;
    margin-bottom: 40px;
}
.bsc-comment-form h3 {
    font-size: 18px;
    font-weight: 700;
    margin-bottom: 20px;
    color: var(--color-text);
}
.bsc-comment-form textarea {
    width: 100%;
    min-height: 120px;
    padding: 12px;
    border: 1px solid rgba(0,0,0,0.15);
    font-size: 14px;
    font-family: inherit;
    line-height: 1.6;
    resize: vertical;
    margin-bottom: 12px;
    box-sizing: border-box;
}
.bsc-comment-form input[type="text"],
.bsc-comment-form input[type="email"] {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid rgba(0,0,0,0.15);
    font-size: 14px;
    font-family: inherit;
    margin-bottom: 12px;
    box-sizing: border-box;
}
.bsc-comment-form button[type="submit"] {
    padding: 10px 24px;
    background: var(--color-text);
    color: var(--color-bg);
    border: none;
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    cursor: pointer;
}
.bsc-comment-form button[type="submit"]:hover {
    opacity: 0.85;
}

/* Post navigation in classic */
.blog-single-classic .post-nav {
    display: flex;
    justify-content: space-between;
    padding: 20px 0;
    margin-bottom: 32px;
    font-size: 14px;
}
.blog-single-classic .post-nav a {
    color: var(--color-text);
    text-decoration: none;
}
.blog-single-classic .post-nav a:hover {
    text-decoration: underline;
}

@media (max-width: 768px) {
    .bsc-title { font-size: 24px; }
    .bsc-content { font-size: 15px; }
    .bsc-hero { margin-bottom: 24px; max-width: 100%; }
    .bsc-content-wrap { max-width: 100%; padding: 0 16px; }
}
"#
}

/// CSS for the Classic list page design (moved from render.rs).
pub fn list_css() -> &'static str {
    r#"
/* Blog Classic list style */
.blog-list.blog-classic {
    max-width: 1100px;
}
.blog-classic .blog-item {
    display: flex;
    gap: 40px;
    align-items: center;
    padding-bottom: 40px;
    margin-bottom: 40px;
    border-bottom: 1px solid rgba(0,0,0,0.08);
}
.blog-classic .blog-item:last-of-type {
    border-bottom: none;
    margin-bottom: 0;
}
.blog-classic .blog-thumb {
    flex: 0 0 50%;
    max-width: 50%;
}
.blog-classic .blog-thumb img {
    width: 100%;
    height: auto;
    display: block;
}
.blog-classic .blog-body {
    flex: 1;
    min-width: 0;
}
.blog-classic .blog-item h2 {
    font-size: 22px;
    font-weight: 700;
    line-height: 1.35;
    margin-bottom: 16px;
}
.blog-classic .blog-item h2 a {
    color: var(--color-text);
    text-decoration: none;
}
.blog-classic .blog-item h2 a:hover {
    text-decoration: underline;
}
.blog-classic .blog-item .blog-excerpt {
    font-size: 14px;
    line-height: 1.7;
    color: var(--color-text);
    margin-bottom: 20px;
}
.blog-classic .blog-meta {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary);
    margin-top: 0;
    margin-bottom: 0;
}
.blog-classic .blog-thumb-placeholder {
    width: 100%;
    height: auto;
    aspect-ratio: 4/3;
}
@media (max-width: 768px) {
    .blog-classic .blog-item {
        flex-direction: column;
        gap: 16px;
    }
    .blog-classic .blog-thumb {
        flex: none;
        max-width: 100%;
    }
}
"#
}
