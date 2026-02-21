use serde_json::Value;

use crate::designs::common::build_classic_comments;
use crate::render::{
    build_pagination, build_share_buttons, count_words_html, format_date, format_date_iso8601,
    html_escape, strip_html_to_text, truncate_words,
};

/// Render the blog list page in the Wide style.
/// Layout: full-width featured image, below it 2-col (title left / excerpt right),
/// thin line, meta line (date · author · comment count).
pub fn render_list(context: &Value) -> String {
    let posts = match context.get("posts") {
        Some(Value::Array(p)) => p,
        _ => return "<p>No posts yet.</p>".to_string(),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let blog_slug = sg("blog_slug", "journal");
    let show_date = sg("blog_show_date", "true") == "true";
    let show_author = sg("blog_show_author", "true") == "true";
    let show_reading_time = sg("blog_show_reading_time", "true") == "true";
    let comments_on_blog = sg("comments_on_blog", "true") == "true";
    let excerpt_words: usize = sg("blog_excerpt_words", "40").parse().unwrap_or(40);

    let mut html = String::from("<div class=\"blog-list blog-wide-list\">");

    for post in posts.iter() {
        let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let raw_excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
        let content_html = post
            .get("content_html")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let excerpt_source = if raw_excerpt.is_empty() {
            strip_html_to_text(content_html)
        } else {
            raw_excerpt.to_string()
        };
        let raw_date = post
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let date = format_date(raw_date, &settings);
        let thumb = post
            .get("featured_image")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let author = post
            .get("author_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let word_count = count_words_html(content_html);
        let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;
        let excerpt = truncate_words(&excerpt_source, excerpt_words);

        // Comment count per post
        let comment_count = post
            .get("comment_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // Image HTML
        let image_html = if !thumb.is_empty() {
            format!(
                "<div class=\"bwd-image\"><a href=\"/{}/{}\"><img src=\"/uploads/{}\" alt=\"{}\"></a></div>",
                blog_slug, slug, thumb, html_escape(title)
            )
        } else {
            String::new()
        };

        // Meta line: date · author · comment count
        let mut meta_parts: Vec<String> = Vec::new();
        if show_date && !date.is_empty() {
            meta_parts.push(html_escape(&date).to_uppercase());
        }
        if show_author && !author.is_empty() {
            meta_parts.push(html_escape(author).to_uppercase());
        }
        if show_reading_time && word_count > 0 {
            meta_parts.push(format!("{} MIN READ", reading_time));
        }
        if comments_on_blog {
            meta_parts.push(format!(
                "{} COMMENT{}",
                comment_count,
                if comment_count == 1 { "" } else { "S" }
            ));
        } else {
            meta_parts.push("COMMENTS CLOSED".to_string());
        }
        let meta_html = if !meta_parts.is_empty() {
            format!(
                "<div class=\"bwd-meta\"><span>{}</span></div>",
                meta_parts.join(" &nbsp;&middot;&nbsp; ")
            )
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<article class=\"bwd-card\">\
             {image_html}\
             <div class=\"bwd-body\">\
             <div class=\"bwd-cols\">\
             <div class=\"bwd-col-title\">\
             <h2><a href=\"/{blog_slug}/{slug}\">{title}</a></h2>\
             <span class=\"bwd-title-rule\"></span>\
             </div>\
             <div class=\"bwd-col-excerpt\"><p>{excerpt}</p></div>\
             </div>\
             {meta_html}\
             </div>\
             </article>",
            image_html = image_html,
            blog_slug = blog_slug,
            slug = slug,
            title = html_escape(title),
            excerpt = html_escape(&excerpt),
            meta_html = meta_html,
        ));
    }

    html.push_str("</div>");

    // Pagination
    let current_page = context
        .get("current_page")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let total_pages = context
        .get("total_pages")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let pagination_type = sg("blog_pagination_type", "classic");

    if total_pages > 1 {
        match pagination_type.as_str() {
            "load_more" => {
                html.push_str(&format!(
                    "<div class=\"pagination\" style=\"justify-content:center\">\
                     <button id=\"load-more-btn\" data-page=\"{}\" data-total=\"{}\" \
                     style=\"padding:10px 28px;border:1px solid #ddd;border-radius:4px;background:transparent;cursor:pointer;font-size:14px\">\
                     Load More</button></div>",
                    current_page + 1, total_pages
                ));
            }
            "infinite" => {
                html.push_str(&format!(
                    "<div id=\"infinite-sentinel\" data-page=\"{}\" data-total=\"{}\" \
                     style=\"height:1px\"></div>",
                    current_page + 1,
                    total_pages
                ));
            }
            _ => {
                html.push_str(&build_pagination(current_page, total_pages));
            }
        }
    }

    html
}

/// Render the blog single page in the Wide style.
/// Layout: full-width image top, below it 2-col:
///   left sidebar (~25%): title, date, author, comment count, categories, tags (bottom-aligned)
///   right (~70%): content, written by, comments
/// Light grey card #FAFAFA, no border/shadow.
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

    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut html = String::from("<article class=\"bws-article\">");
    html.push_str("<div class=\"bws-card\">");

    // Full-width featured image
    if !featured.is_empty() {
        html.push_str(&format!(
            "<div class=\"bws-hero\"><img src=\"/uploads/{}\" alt=\"{}\"></div>",
            featured,
            html_escape(title)
        ));
    }

    // Share buttons below image
    if share_pos == "below_image" && !page_url.is_empty() {
        html.push_str("<div class=\"bws-share-row\">");
        html.push_str(&build_share_buttons(&settings, &page_url, title));
        html.push_str("</div>");
    }

    // 2-column layout
    html.push_str("<div class=\"bws-body\">");

    // ── Left sidebar (bottom-aligned) ──
    html.push_str("<div class=\"bws-sidebar\">");

    // Title in sidebar
    html.push_str(&format!(
        "<h1 class=\"bws-title\">{}</h1>\
         <span class=\"bws-title-rule\"></span>",
        html_escape(title)
    ));

    // Meta items
    html.push_str("<div class=\"bws-sidebar-meta\">");

    if show_date && !date.is_empty() {
        html.push_str(&format!(
            "<span class=\"bws-meta-item\">{}</span>",
            html_escape(&date)
        ));
    }
    if show_author && !author.is_empty() {
        html.push_str(&format!(
            "<span class=\"bws-meta-item\">{}</span>",
            html_escape(author)
        ));
    }
    if show_reading_time && word_count > 0 {
        html.push_str(&format!(
            "<span class=\"bws-meta-item\">{} min read</span>",
            reading_time
        ));
    }
    if comments_on {
        let comment_count = context
            .get("comments")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        html.push_str(&format!(
            "<span class=\"bws-meta-item\">{} Comment{}</span>",
            comment_count,
            if comment_count == 1 { "" } else { "s" }
        ));
    } else {
        html.push_str("<span class=\"bws-meta-item\">Comments Closed</span>");
    }

    // Categories
    if let Some(Value::Array(cats)) = context.get("categories") {
        if !cats.is_empty() {
            html.push_str("<div class=\"bws-sidebar-section\"><h4>Categories</h4>");
            for cat in cats {
                let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let cat_slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/category/{}\" class=\"bws-sidebar-link\">{}</a>",
                    blog_slug,
                    cat_slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    // Tags
    if let Some(Value::Array(tags)) = context.get("tags") {
        if !tags.is_empty() {
            html.push_str("<div class=\"bws-sidebar-section\"><h4>Tags</h4>");
            for tag in tags {
                let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let tag_slug = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/tag/{}\" class=\"bws-sidebar-link\">#{}</a>",
                    blog_slug,
                    tag_slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</div>"); // close bws-sidebar-meta
    html.push_str("</div>"); // close bws-sidebar

    // ── Right column: content + written by + comments ──
    html.push_str("<div class=\"bws-main\">");
    html.push_str(&format!("<div class=\"bws-content\">{}</div>", content));

    // Written by section
    if show_author && !author.is_empty() {
        let written_by_label = sg("blog_written_by_label", "By");
        html.push_str(&format!(
            "<div class=\"bws-written-by\"><strong>{} {}</strong></div>",
            html_escape(&written_by_label),
            html_escape(author)
        ));
    }

    // Share buttons below content
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
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
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_classic_comments(context, &settings, post_id));
    }

    html.push_str("</div>"); // close bws-main
    html.push_str("</div>"); // close bws-body
    html.push_str("</div>"); // close bws-card
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

/// CSS for the wide list design.
pub fn list_css() -> &'static str {
    r#"
/* ── Wide List ── */
.blog-list.blog-wide-list {
    max-width: 1400px;
    margin: 0 auto;
    padding: 40px 20px;
}
.blog-wide-list h1 {
    font-size: var(--font-size-h1);
    font-weight: 700;
    margin-bottom: 48px;
    text-align: center;
}
.bwd-card {
    background: #FAFAFA;
    margin-bottom: 64px;
}
.bwd-image {
    width: 100%;
    overflow: hidden;
}
.bwd-image img {
    width: 100%;
    height: auto;
    display: block;
    object-fit: cover;
}
.bwd-body {
    padding: 48px 56px 40px;
}
.bwd-cols {
    display: flex;
    gap: 48px;
    align-items: flex-start;
}
.bwd-col-title {
    flex: 0 0 32%;
    max-width: 32%;
}
.bwd-col-title h2 {
    font-size: 22px;
    font-weight: 700;
    line-height: 1.3;
    margin: 0 0 20px;
}
.bwd-col-title h2 a {
    color: var(--color-text);
    text-decoration: none;
}
.bwd-col-title h2 a:hover {
    opacity: 0.7;
}
.bwd-title-rule {
    display: block;
    width: 40px;
    height: 1px;
    background: var(--color-text);
    margin-top: 0;
}
.bwd-col-excerpt {
    flex: 1;
}
.bwd-col-excerpt p {
    font-size: 13px;
    line-height: 1.8;
    color: var(--color-text-secondary);
    margin: 0;
}
.bwd-meta {
    margin-top: 32px;
    margin-left: calc(32% + 48px);
}
.bwd-meta span {
    font-family: var(--font-captions);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--color-text-secondary);
}

@media (max-width: 768px) {
    .blog-wide-list {
        max-width: 100%;
        padding: 16px;
    }
    .bwd-cols {
        flex-direction: column;
        gap: 16px;
    }
    .bwd-col-title {
        flex: none;
        max-width: 100%;
    }
    .bwd-body {
        padding: 24px 20px;
    }
    .bwd-meta {
        margin-left: 0;
    }
}
"#
}

/// CSS for the wide single page design.
pub fn single_css() -> &'static str {
    r#"
/* ── Wide Single ── */
.bws-article {
    max-width: 1400px;
    margin: 0 auto;
    padding: 40px 20px;
}
.bws-card {
    background: #FAFAFA;
}
.bws-hero {
    width: 100%;
    overflow: hidden;
}
.bws-hero img {
    width: 100%;
    height: auto;
    display: block;
    object-fit: cover;
}
.bws-share-row {
    padding: 12px 56px;
}
.bws-share-row .share-icons {
    border-top: none;
    margin: 0;
    padding: 8px 0;
}
.bws-article .share-icons {
    border-top: none;
}
.bws-body {
    display: flex;
    gap: 48px;
    padding: 48px 56px 40px;
}
.bws-sidebar {
    flex: 0 0 28%;
    max-width: 28%;
}
.bws-title {
    font-size: 26px;
    font-weight: 700;
    line-height: 1.3;
    margin-bottom: 20px;
}
.bws-title-rule {
    display: block;
    width: 40px;
    height: 1px;
    background: var(--color-text);
}
.bws-sidebar-meta {
    margin-top: 36px;
    display: flex;
    flex-direction: column;
    gap: 8px;
}
.bws-meta-item {
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary);
}
.bws-sidebar-section {
    margin-top: 20px;
}
.bws-sidebar-section h4 {
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 700;
    color: var(--color-text);
    margin-bottom: 6px;
}
.bws-sidebar-link {
    display: block;
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text-secondary);
    text-decoration: none;
    margin-bottom: 3px;
}
.bws-sidebar-link:hover {
    color: var(--color-text);
}
.bws-main {
    flex: 1;
    min-width: 0;
}
.bws-content {
    font-size: 16px;
    line-height: 1.8;
    color: var(--color-text);
}
.bws-content p {
    margin-bottom: 1.2em;
}
.bws-content blockquote {
    border-left: 3px solid var(--color-text);
    margin: 1.5em 0;
    padding: 0.5em 0 0.5em 1.5em;
    font-style: italic;
    color: var(--color-text-secondary);
}
.bws-content img {
    max-width: 100%;
    height: auto;
}
.bws-written-by {
    margin-top: 32px;
    font-size: 14px;
    color: var(--color-text);
}

@media (max-width: 768px) {
    .bws-article {
        max-width: 100%;
        margin-left: 0;
        padding: 16px;
    }
    .bws-body {
        flex-direction: column;
        gap: 24px;
        padding: 20px 16px;
    }
    .bws-sidebar {
        flex: none;
        max-width: 100%;
    }
}
"#
}
