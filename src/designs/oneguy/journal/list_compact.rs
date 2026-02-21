use serde_json::Value;

use crate::render::{
    build_pagination, count_words_html, format_date, html_escape, strip_html_to_text,
    truncate_words,
};

/// Render the blog list page in the Compact style.
/// Layout: small square thumbnail left, title + date + excerpt right, separator lines.
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
        "<div class=\"blog-list blog-compact\">\n<h1>{}</h1>",
        html_escape(&blog_label)
    );

    for post in posts {
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

        let thumb_html = if !thumb.is_empty() {
            format!(
                "<div class=\"bcl-thumb\"><a href=\"/{}/{}\"><img src=\"/uploads/{}\" alt=\"{}\"></a></div>",
                blog_slug, slug, thumb, html_escape(title)
            )
        } else {
            "<div class=\"bcl-thumb bcl-thumb-placeholder\"><svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1\"><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/><circle cx=\"8.5\" cy=\"8.5\" r=\"1.5\"/><path d=\"M21 15l-5-5L5 21\"/></svg></div>".to_string()
        };

        // Build meta line
        let mut meta_parts: Vec<String> = Vec::new();
        if show_date && !date.is_empty() {
            meta_parts.push(date);
        }
        if show_reading_time && word_count > 0 {
            meta_parts.push(format!("{} min read", reading_time));
        }
        let meta_html = if !meta_parts.is_empty() {
            format!(
                "<div class=\"bcl-meta\">{}</div>",
                meta_parts.join(" &middot; ")
            )
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<article class=\"bcl-item\">\
             {thumb_html}\
             <div class=\"bcl-body\">\
             <h2><a href=\"/{blog_slug}/{slug}\">{title}</a></h2>\
             {meta_html}\
             <div class=\"bcl-excerpt\">{excerpt}</div>\
             </div>\
             </article>",
            thumb_html = thumb_html,
            blog_slug = blog_slug,
            slug = slug,
            title = html_escape(title),
            meta_html = meta_html,
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

/// CSS for the Compact list page design.
pub fn list_css() -> &'static str {
    r#"
/* Blog Compact list style */
.blog-list.blog-compact {
    max-width: clamp(780px, 70%, 1200px);
    margin-left: 24px;
}
.blog-compact h1 {
    font-size: 18px;
    font-weight: 400;
    margin-bottom: 24px;
    color: var(--color-text);
}
.bcl-item {
    display: flex;
    gap: 24px;
    padding: 24px 0;
    border-bottom: 1px solid rgba(0,0,0,0.08);
}
.bcl-item:first-of-type {
    border-top: 1px solid rgba(0,0,0,0.08);
}
.bcl-thumb {
    flex: 0 0 80px;
    width: 80px;
    height: 80px;
    overflow: hidden;
}
.bcl-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
}
.bcl-thumb-placeholder {
    background: #f0f0f0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #ccc;
}
.bcl-body {
    flex: 1;
    min-width: 0;
}
.bcl-body h2 {
    font-size: 17px;
    font-weight: 700;
    line-height: 1.35;
    margin: 0 0 4px 0;
}
.bcl-body h2 a {
    color: var(--color-text);
    text-decoration: none;
}
.bcl-body h2 a:hover {
    text-decoration: underline;
}
.bcl-meta {
    font-size: 12px;
    color: var(--color-text-secondary);
    margin-bottom: 6px;
}
.bcl-excerpt {
    font-size: 13px;
    line-height: 1.6;
    color: var(--color-text);
}
@media (max-width: 768px) {
    .blog-list.blog-compact { max-width: 100%; margin-left: 0; padding: 0 16px; }
    .bcl-thumb { flex: 0 0 60px; width: 60px; height: 60px; }
    .bcl-body h2 { font-size: 15px; }
    .bcl-excerpt { font-size: 12px; }
}
"#
}
