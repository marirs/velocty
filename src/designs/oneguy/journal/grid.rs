use serde_json::Value;

use crate::designs::common::{author_initials, build_classic_comments, name_hue};
use crate::render::{
    build_pagination, build_share_buttons, count_words_html, format_date, format_date_iso8601,
    html_escape, slug_url, strip_html_to_text, truncate_words,
};

/// Render the blog list page in the Grid style.
/// Layout: CSS grid of cards (2 or 3 columns), each with featured image, title, excerpt, meta.
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
    let show_author = sg("blog_show_author", "true") == "true";
    let show_date = sg("blog_show_date", "true") == "true";
    let show_reading_time = sg("blog_show_reading_time", "true") == "true";
    let comments_on = sg("comments_on_blog", "false") == "true";
    let excerpt_words: usize = sg("blog_excerpt_words", "40").parse().unwrap_or(40);
    let columns: u8 = sg("blog_grid_columns", "2").parse().unwrap_or(2);

    let blog_label = sg("blog_label", "journal");
    let mut html = format!(
        "<div class=\"blog-list blog-grid\" style=\"--bgrid-cols:{}\">\n<h1>{}</h1>\n<div class=\"bgrid-cards\">",
        columns,
        html_escape(&blog_label)
    );

    for post in posts {
        let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let post_slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
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
        let excerpt = truncate_words(&excerpt_source, excerpt_words);
        let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;
        let comment_count: i64 = post
            .get("comment_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let post_url = slug_url(&blog_slug, post_slug);

        let thumb_html = if !thumb.is_empty() {
            format!(
                "<a href=\"{}\" class=\"bgrid-thumb\"><img src=\"/uploads/{}\" alt=\"{}\"></a>",
                post_url,
                thumb,
                html_escape(title)
            )
        } else {
            format!(
                "<a href=\"{}\" class=\"bgrid-thumb bgrid-thumb-placeholder\">\
                 <svg width=\"48\" height=\"48\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1\">\
                 <rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/>\
                 <circle cx=\"8.5\" cy=\"8.5\" r=\"1.5\"/>\
                 <path d=\"M21 15l-5-5L5 21\"/></svg></a>",
                post_url
            )
        };

        // Build meta line: DATE / AUTHOR / COMMENTS / READING TIME
        let mut meta_parts: Vec<String> = Vec::new();
        if show_date && !date.is_empty() {
            meta_parts.push(format!("<time>{}</time>", html_escape(&date)));
        }
        if show_author && !author.is_empty() {
            meta_parts.push(html_escape(author).to_string());
        }
        if comments_on {
            if comment_count == 0 {
                meta_parts.push("No Comments".to_string());
            } else if comment_count == 1 {
                meta_parts.push("1 Comment".to_string());
            } else {
                meta_parts.push(format!("{} Comments", comment_count));
            }
        } else {
            meta_parts.push("Comments Closed".to_string());
        }
        if show_reading_time && word_count > 0 {
            meta_parts.push(format!("{} min read", reading_time));
        }
        let meta_html = if !meta_parts.is_empty() {
            format!("<div class=\"bgrid-meta\">{}</div>", meta_parts.join(" ◆ "))
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<article class=\"bgrid-card\">\
             {thumb_html}\
             <div class=\"bgrid-body\">\
             <h2><a href=\"{post_url}\">{title}</a></h2>\
             <p class=\"bgrid-excerpt\">{excerpt}</p>\
             {meta_html}\
             </div>\
             </article>",
            thumb_html = thumb_html,
            post_url = post_url,
            title = html_escape(title),
            excerpt = html_escape(&excerpt),
            meta_html = meta_html,
        ));
    }

    html.push_str("</div></div>");

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

/// Render the blog single page in the Grid style.
/// Layout: two-column — left content card (#fafafa + shadow) with featured image, title, body;
/// right sidebar with date, author, comment count, categories, tags.
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
    let comment_count = if let Some(Value::Array(comments)) = context.get("comments") {
        comments.len()
    } else {
        0
    };

    // Structure:
    //   article.bgs-single (CSS grid: 3 rows x 2 cols)
    //     div.bgs-hero      → row 1, col 1 (inside card area)
    //     h1.bgs-title      → row 2, col 1
    //     div.bgs-body       → row 3, col 1 (content, author, nav, comments)
    //     aside.bgs-sidebar  → row 3, col 2 (aligns with content start)
    //   The card background spans rows 1-3 col 1 via a pseudo-element.

    let mut html = String::from("<article class=\"bgs-single\">");

    // Row 1: Featured image
    if !featured.is_empty() {
        html.push_str(&format!(
            "<div class=\"bgs-hero\"><img src=\"/uploads/{}\" alt=\"{}\"></div>",
            featured,
            html_escape(title)
        ));
    }

    // Row 2: Title
    html.push_str(&format!(
        "<h1 class=\"bgs-title\">{}</h1>",
        html_escape(title)
    ));

    // Share buttons below title
    if share_pos == "below_image" && !page_url.is_empty() {
        html.push_str("<div class=\"bgs-share-wrap\">");
        html.push_str(&build_share_buttons(&settings, &page_url, title));
        html.push_str("</div>");
    }

    // Row 3 col 1: Content body
    html.push_str("<div class=\"bgs-body\">");
    html.push_str(&format!("<div class=\"bgs-content\">{}</div>", content));

    // Share buttons below content
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // "Written by" section
    if show_author && !author.is_empty() {
        let written_by_label = sg("blog_written_by_label", "By");
        let initials = author_initials(author);
        let hue = name_hue(author);
        html.push_str(&format!(
            "<div class=\"bgs-author-box\">\
             <div class=\"bgs-author-avatar\" style=\"background:hsl({},45%,55%)\">{}</div>\
             <div class=\"bgs-author-info\">{}<br><strong>{}</strong></div>\
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
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_classic_comments(context, &settings, post_id));
    }

    html.push_str("</div>"); // close bgs-body

    // Row 3 col 2: Sidebar meta (no background)
    html.push_str("<aside class=\"bgs-sidebar\">");

    // Date
    if show_date && !date.is_empty() {
        html.push_str(&format!(
            "<div class=\"bgs-side-item\">{}</div>",
            html_escape(&date)
        ));
    }

    // Author
    if show_author && !author.is_empty() {
        html.push_str(&format!(
            "<div class=\"bgs-side-item\">{}</div>",
            html_escape(author)
        ));
    }

    // Comment count
    if comments_on {
        let label = match comment_count {
            0 => "No Comments".to_string(),
            1 => "1 Comment".to_string(),
            n => format!("{} Comments", n),
        };
        html.push_str(&format!("<div class=\"bgs-side-item\">{}</div>", label));
    }

    // Reading time
    if show_reading_time && word_count > 0 {
        html.push_str(&format!(
            "<div class=\"bgs-side-item\">{} Min Read</div>",
            reading_time
        ));
    }

    // Categories
    if let Some(Value::Array(cats)) = context.get("categories") {
        if !cats.is_empty() {
            html.push_str("<div class=\"bgs-side-group\"><h4>Categories</h4>");
            for cat in cats {
                let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/category/{}\" class=\"bgs-side-link\">{}</a>",
                    blog_slug,
                    slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    // Tags
    if let Some(Value::Array(tags)) = context.get("tags") {
        if !tags.is_empty() {
            html.push_str("<div class=\"bgs-side-group\"><h4>Tags</h4>");
            for tag in tags {
                let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let slug = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    "<a href=\"/{}/tag/{}\" class=\"bgs-side-link\">#{}</a>",
                    blog_slug,
                    slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</aside>"); // close bgs-sidebar
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

/// CSS for the Grid single page.
pub fn single_css() -> &'static str {
    r#"
/* Grid single — CSS grid layout
   Row 1: image (col 1 only)
   Row 2: title (col 1 only)
   Row 3: share (col 1 only)
   Row 4: body (col 1) + sidebar (col 2)
   Card background covers col 1 rows 1-4 via pseudo-element */
.bgs-single {
    max-width: clamp(800px, 100%, 1100px);
    padding: 40px 0;
    display: grid;
    grid-template-columns: 1fr 200px;
    grid-template-rows: auto auto auto 1fr;
    column-gap: 60px;
    position: relative;
}
/* Card background on left column only */
.bgs-single::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: calc(100% - 260px);
    height: 100%;
    background: #fafafa;
    box-shadow: 0 1px 4px rgba(0,0,0,.06);
    border-radius: 4px;
    z-index: 0;
}
.bgs-single > * {
    position: relative;
    z-index: 1;
}

/* Row 1: image */
.bgs-hero {
    grid-column: 1;
    grid-row: 1;
    padding: 40px 40px 0 40px;
}
.bgs-hero img {
    width: 100%;
    height: auto;
    display: block;
}

/* Row 2: title */
.bgs-title {
    grid-column: 1;
    grid-row: 2;
    font-size: 24px;
    font-weight: 700;
    line-height: 1.35;
    margin: 0;
    padding: 32px 40px 8px;
    color: var(--color-text);
    border: none;
}

/* Row 3: share buttons */
.bgs-share-wrap {
    grid-column: 1;
    grid-row: 3;
    padding: 0 40px 12px;
}
.bgs-single .share-icons {
    border-top: none;
    padding-top: 0;
}

/* Row 4 col 1: content body */
.bgs-body {
    grid-column: 1;
    grid-row: 4;
    padding: 0 40px 40px;
}
.bgs-content {
    font-size: 15px;
    line-height: 1.85;
    color: var(--color-text);
    margin-bottom: 36px;
}
.bgs-content p {
    margin-bottom: 1.5em;
}
.bgs-content blockquote {
    border-left: 3px solid var(--color-text);
    margin: 1.5em 0;
    padding: 1em 1.5em;
    font-style: italic;
    color: var(--color-text-secondary);
}
.bgs-content img {
    max-width: 100%;
    height: auto;
}

/* Written by author box */
.bgs-author-box {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 24px 0;
    border-top: 1px solid rgba(0,0,0,0.08);
    margin-bottom: 32px;
}
.bgs-author-avatar {
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
.bgs-author-info {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.5;
}
.bgs-author-info strong {
    color: var(--color-text);
    font-size: 15px;
}

/* Post navigation */
.bgs-single .post-nav {
    display: flex;
    justify-content: space-between;
    padding: 20px 0;
    margin-bottom: 32px;
    font-size: 14px;
}
.bgs-single .post-nav a {
    color: var(--color-text);
    text-decoration: none;
}
.bgs-single .post-nav a:hover {
    text-decoration: underline;
}

/* Row 4 col 2: sidebar meta (no background) */
.bgs-sidebar {
    grid-column: 2;
    grid-row: 4;
    align-self: start;
}
.bgs-side-item {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text);
    margin-bottom: 8px;
    line-height: 1.6;
}
.bgs-side-group {
    margin-top: 28px;
}
.bgs-side-group h4 {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text);
    margin: 0 0 10px 0;
}
.bgs-side-link {
    display: block;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text);
    text-decoration: none;
    margin-bottom: 4px;
    line-height: 1.6;
}
.bgs-side-link:hover {
    color: var(--color-accent);
}

@media (max-width: 900px) {
    .bgs-single {
        max-width: 100%;
        display: block;
    }
    .bgs-single::before {
        width: 100%;
    }
    .bgs-hero { padding: 20px 20px 0; }
    .bgs-title { padding: 24px 20px 20px; font-size: 20px; }
    .bgs-share-wrap { padding: 0 20px 12px; }
    .bgs-body { padding: 0 20px 20px; }
    .bgs-content { font-size: 14px; }
    .bgs-sidebar {
        padding: 24px 20px;
        display: flex;
        flex-wrap: wrap;
        gap: 16px 32px;
    }
    .bgs-side-group {
        margin-top: 0;
    }
}
"#
}

/// CSS for the Grid blog list style.
pub fn css() -> &'static str {
    r#"
/* Blog Grid style */
.blog-list.blog-grid {
    width: 100%;
    max-width: 100%;
    padding: 40px 0;
}
.blog-list.blog-grid h1 {
    font-size: 18px;
    font-weight: 400;
    margin-bottom: 32px;
    color: var(--color-text);
}
.bgrid-cards {
    display: grid;
    grid-template-columns: repeat(var(--bgrid-cols, 2), 1fr);
    gap: 60px 50px;
}
.bgrid-card {
    display: flex;
    flex-direction: column;
    background: #fafafa;
    box-shadow: 0 1px 4px rgba(0,0,0,.06);
    border-radius: 4px;
    overflow: hidden;
}
.bgrid-thumb {
    display: block;
    width: 100%;
    overflow: hidden;
    text-decoration: none;
}
.bgrid-thumb img {
    width: 100%;
    height: auto;
    display: block;
}
.bgrid-thumb-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 16/10;
    background: #f5f5f5;
    color: #ccc;
}
.bgrid-body {
    padding: 20px 20px 20px 20px;
}
.bgrid-body h2 {
    font-size: 18px;
    font-weight: 700;
    line-height: 1.35;
    margin: 0 0 18px 0;
}
.bgrid-body h2 a {
    color: #111;
    text-decoration: none;
}
.bgrid-body h2 a:hover {
    text-decoration: underline;
}
.bgrid-excerpt {
    font-size: 14px;
    line-height: 1.75;
    color: #333;
    margin: 0 0 22px 0;
}
.bgrid-meta {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #999;
    line-height: 1.6;
}
@media (max-width: 768px) {
    .blog-list.blog-grid { padding: 20px 0; }
    .bgrid-cards {
        grid-template-columns: 1fr;
        gap: 40px;
    }
    .bgrid-body h2 { font-size: 16px; }
    .bgrid-excerpt { font-size: 13px; }
}
"#
}
