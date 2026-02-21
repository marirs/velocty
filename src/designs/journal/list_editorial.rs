use serde_json::Value;

use crate::designs::journal::list_classic::build_classic_comments;
use crate::render::{
    build_pagination, build_share_buttons, count_words_html, format_date, format_date_iso8601,
    html_escape, strip_html_to_text, truncate_words,
};

/// Render the blog list page in the Editorial style.
/// Layout: large featured image on one side, white boxed content on the other,
/// alternating left/right per post. Content shows date, category, reading time,
/// title, and excerpt.
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
    let show_reading_time = sg("blog_show_reading_time", "true") == "true";
    let excerpt_words: usize = sg("blog_excerpt_words", "40").parse().unwrap_or(40);

    let blog_label = sg("blog_label", "journal");
    let mut html = format!(
        "<div class=\"blog-list blog-editorial-list\">\n<h1>{}</h1>",
        html_escape(&blog_label)
    );

    for (i, post) in posts.iter().enumerate() {
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
        let word_count = count_words_html(content_html);
        let excerpt = truncate_words(&excerpt_source, excerpt_words);
        let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;

        // Categories
        let categories = post
            .get("categories")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let cat_names: Vec<String> = categories
            .iter()
            .filter_map(|c| {
                c.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        // Image HTML
        let image_html = if !thumb.is_empty() {
            format!(
                "<div class=\"bed-image\"><a href=\"/{}/{}\"><img src=\"/uploads/{}\" alt=\"{}\"></a></div>",
                blog_slug, slug, thumb, html_escape(title)
            )
        } else {
            format!(
                "<div class=\"bed-image bed-image-placeholder\"><a href=\"/{}/{}\">\
                 <svg width=\"48\" height=\"48\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1\">\
                 <rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/>\
                 <circle cx=\"8.5\" cy=\"8.5\" r=\"1.5\"/>\
                 <path d=\"M21 15l-5-5L5 21\"/></svg></a></div>",
                blog_slug, slug
            )
        };

        // Meta lines: Date / Author + Reading Time / Posted in Category
        let author_name = post
            .get("author_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut meta_lines: Vec<String> = Vec::new();
        if show_date && !date.is_empty() {
            meta_lines.push(format!(
                "<span class=\"bed-date\">{}</span>",
                html_escape(&date)
            ));
        }
        {
            let mut author_rt: Vec<String> = Vec::new();
            if !author_name.is_empty() {
                author_rt.push(html_escape(author_name));
            }
            if show_reading_time && word_count > 0 {
                author_rt.push(format!("{} min read", reading_time));
            }
            if !author_rt.is_empty() {
                meta_lines.push(format!(
                    "<span class=\"bed-author\">{}</span>",
                    author_rt.join(" / ")
                ));
            }
        }
        if !cat_names.is_empty() {
            let cat_str = cat_names
                .iter()
                .map(|c| html_escape(c))
                .collect::<Vec<_>>()
                .join(", ");
            meta_lines.push(format!(
                "<span class=\"bed-category\">Posted in {}</span>",
                cat_str
            ));
        }

        let meta_html = if !meta_lines.is_empty() {
            format!("<div class=\"bed-meta\">{}</div>", meta_lines.join("\n"))
        } else {
            String::new()
        };

        // Alternating: even index = image-left, odd = image-right
        let row_class = if i % 2 == 0 {
            "bed-row"
        } else {
            "bed-row bed-row-reverse"
        };

        html.push_str(&format!(
            "<article class=\"{row_class}\">\
             {image_html}\
             <div class=\"bed-content\">\
             {meta_html}\
             <h2><a href=\"/{blog_slug}/{slug}\">{title}</a></h2>\
             <p class=\"bed-excerpt\">{excerpt}</p>\
             </div>\
             </article>",
            row_class = row_class,
            image_html = image_html,
            meta_html = meta_html,
            blog_slug = blog_slug,
            slug = slug,
            title = html_escape(title),
            excerpt = html_escape(&excerpt),
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

/// CSS for the editorial list design.
pub fn list_css() -> &'static str {
    r#"
/* ── Editorial List ── */
.blog-editorial-list {
    max-width: clamp(780px, 70%, 1200px);
    margin-left: 24px;
    padding: 20px 0;
}
.blog-editorial-list h1 {
    font-size: var(--font-size-h1);
    font-weight: 700;
    margin-bottom: 40px;
}
.bed-row {
    display: flex;
    gap: 0;
    margin-bottom: 56px;
    align-items: stretch;
    background: #fff;
    padding: 15px;
    box-sizing: border-box;
    box-shadow: 0 2px 12px rgba(0,0,0,0.08);
}
.bed-row-reverse {
    flex-direction: row-reverse;
}
.bed-image {
    flex: 0 0 58%;
    max-width: 58%;
    overflow: hidden;
}
.bed-image img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
}
.bed-image-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0,0,0,0.04);
    min-height: 320px;
    color: rgba(0,0,0,0.25);
}
.bed-content {
    flex: 1;
    background: #fff;
    padding: 32px 36px;
    display: flex;
    flex-direction: column;
    justify-content: flex-start;
}
.bed-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 36px;
}
.bed-meta span {
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary);
}
.bed-content h2 {
    font-size: 22px;
    font-weight: 700;
    line-height: 1.3;
    margin-bottom: 14px;
}
.bed-content h2 a {
    color: var(--color-text);
    text-decoration: none;
}
.bed-content h2 a:hover {
    opacity: 0.7;
}
.bed-excerpt {
    font-size: 14px;
    line-height: 1.7;
    color: var(--color-text-secondary);
    margin: 0;
}

@media (max-width: 768px) {
    .blog-editorial-list {
        max-width: 100%;
        margin-left: 0;
        padding: 16px;
    }
    .bed-row,
    .bed-row-reverse {
        flex-direction: column;
    }
    .bed-image {
        flex: none;
        max-width: 100%;
    }
    .bed-image img {
        height: auto;
    }
    .bed-content {
        padding: 20px 16px;
    }
}
"#
}

/// Render the blog single page in the Editorial style.
/// Layout: white card with 15px padding + shadow, full-width image top,
/// 2-column below: left = title + content + comments, right = sidebar meta.
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

    let mut html = String::from("<article class=\"bes-article\">");

    // White card wrapper
    html.push_str("<div class=\"bes-card\">");

    // Full-width featured image inside card
    if !featured.is_empty() {
        html.push_str(&format!(
            "<div class=\"bes-hero\"><img src=\"/uploads/{}\" alt=\"{}\"></div>",
            featured,
            html_escape(title)
        ));
    }

    // Share buttons below image
    if share_pos == "below_image" && !page_url.is_empty() {
        html.push_str("<div class=\"bes-share-row\">");
        html.push_str(&build_share_buttons(&settings, &page_url, title));
        html.push_str("</div>");
    }

    // 2-column layout below image
    html.push_str("<div class=\"bes-body\">");

    // ── Left column: title + content + comments ──
    html.push_str("<div class=\"bes-main\">");
    html.push_str(&format!(
        "<h1 class=\"bes-title\">{}</h1>",
        html_escape(title)
    ));
    html.push_str(&format!("<div class=\"bes-content\">{}</div>", content));

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
    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_classic_comments(context, &settings, post_id));
    }

    html.push_str("</div>"); // close bes-main

    // ── Right column: sidebar meta ──
    html.push_str("<aside class=\"bes-sidebar\">");

    // Date
    if show_date && !date.is_empty() {
        html.push_str(&format!(
            "<div class=\"bes-sidebar-item\"><span class=\"bes-sidebar-label\">{}</span></div>",
            html_escape(&date)
        ));
    }

    // Author
    if show_author && !author.is_empty() {
        html.push_str(&format!(
            "<div class=\"bes-sidebar-item\"><span class=\"bes-sidebar-label\">{}</span></div>",
            html_escape(author)
        ));
    }

    // Reading time
    if show_reading_time && word_count > 0 {
        html.push_str(&format!(
            "<div class=\"bes-sidebar-item\"><span class=\"bes-sidebar-label\">{} min read</span></div>",
            reading_time
        ));
    }

    // Comment count
    if comments_on {
        let comment_count = context
            .get("comments")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        html.push_str(&format!(
            "<div class=\"bes-sidebar-item\"><span class=\"bes-sidebar-label\">{} Comment{}</span></div>",
            comment_count,
            if comment_count == 1 { "" } else { "s" }
        ));
    }

    // Categories
    if let Some(Value::Array(cats)) = context.get("categories") {
        if !cats.is_empty() {
            html.push_str("<div class=\"bes-sidebar-section\"><h4>Categories</h4>");
            for cat in cats {
                let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let cat_slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/category/{}\" class=\"bes-sidebar-link\">{}</a>",
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
            html.push_str("<div class=\"bes-sidebar-section\"><h4>Tags</h4>");
            for tag in tags {
                let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let tag_slug = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/tag/{}\" class=\"bes-sidebar-link\">#{}</a>",
                    blog_slug,
                    tag_slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</aside>"); // close bes-sidebar
    html.push_str("</div>"); // close bes-body
    html.push_str("</div>"); // close bes-card
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

/// CSS for the editorial single page design.
pub fn single_css() -> &'static str {
    r#"
/* ── Editorial Single ── */
.bes-article {
    max-width: clamp(780px, 70%, 1200px);
    margin-left: 24px;
    padding: 20px 0;
}
.bes-card {
    background: #fff;
    padding: 15px;
    box-shadow: 0 2px 12px rgba(0,0,0,0.08);
}
.bes-hero {
    width: 100%;
    overflow: hidden;
}
.bes-hero img {
    width: 100%;
    height: auto;
    display: block;
    object-fit: cover;
}
.bes-share-row {
    padding: 12px 0;
}
.bes-body {
    display: flex;
    gap: 60px;
    padding: 32px 20px 20px;
}
.bes-main {
    flex: 1;
    min-width: 0;
}
.bes-title {
    font-size: 28px;
    font-weight: 700;
    line-height: 1.3;
    margin-bottom: 24px;
}
.bes-content {
    font-size: 16px;
    line-height: 1.8;
    color: var(--color-text);
}
.bes-content p {
    margin-bottom: 1.2em;
}
.bes-content blockquote {
    border-left: 3px solid var(--color-text);
    margin: 1.5em 0;
    padding: 0.5em 0 0.5em 1.5em;
    font-style: italic;
    color: var(--color-text-secondary);
}
.bes-content img {
    max-width: 100%;
    height: auto;
}
.bes-sidebar {
    flex: 0 0 220px;
    max-width: 220px;
    padding-top: 4px;
}
.bes-sidebar-item {
    margin-bottom: 4px;
}
.bes-sidebar-label {
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-secondary);
}
.bes-sidebar-section {
    margin-top: 36px;
}
.bes-sidebar-section h4 {
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 700;
    color: var(--color-text);
    margin-bottom: 8px;
}
.bes-sidebar-link {
    display: block;
    font-family: var(--font-captions);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text-secondary);
    text-decoration: none;
    margin-bottom: 4px;
}
.bes-sidebar-link:hover {
    color: var(--color-text);
}

@media (max-width: 768px) {
    .bes-article {
        max-width: 100%;
        margin-left: 0;
        padding: 16px;
    }
    .bes-body {
        flex-direction: column;
        gap: 24px;
        padding: 20px 12px;
    }
    .bes-sidebar {
        flex: none;
        max-width: 100%;
    }
}
"#
}
