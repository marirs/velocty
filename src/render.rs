use serde_json::Value;

use crate::db::DbPool;
use crate::models::design::{Design, DesignTemplate};
use crate::models::settings::Setting;
use crate::seo;
use crate::typography;

/// Renders a full page by merging the active design template with content data.
/// Phase 3: checks for a custom design template in the DB first.
/// Falls back to the hardcoded default templates if none found.
pub fn render_page(pool: &DbPool, template_type: &str, context: &Value) -> String {
    // Phase 3: Try to load from active design's custom template
    if let Some(design) = Design::active(pool) {
        if let Some(tmpl) = DesignTemplate::get(pool, design.id, template_type) {
            if !tmpl.layout_html.trim().is_empty() {
                return render_from_design(pool, &tmpl, context);
            }
        }
    }

    // Fallback: hardcoded default rendering (Phase 1)
    render_page_default(pool, template_type, context)
}

/// Render a page from a custom design template stored in the DB.
/// Replaces {{placeholder}} tags with real content generated from the context.
fn render_from_design(pool: &DbPool, tmpl: &DesignTemplate, context: &Value) -> String {
    let settings = context.get("settings").cloned().unwrap_or_default();
    let css_vars = typography::build_css_variables(&settings);
    let font_links = typography::build_font_links(&settings);

    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
    };

    let site_name = sg("site_name", "Velocty");
    let site_tagline = sg("site_caption", "");

    // Build all placeholder replacements
    let mut html = tmpl.layout_html.clone();

    // ── Global placeholders ──
    html = html.replace("{{site_title}}", &html_escape(&site_name));
    html = html.replace("{{site_tagline}}", &html_escape(&site_tagline));
    html = html.replace("{{site_logo}}", &build_logo_html(&settings));
    html = html.replace("{{navigation}}", &build_categories_sidebar(context));
    html = html.replace("{{footer}}", &format!("<p>&copy; {} {}</p>", chrono::Utc::now().format("%Y"), html_escape(&site_name)));
    html = html.replace("{{social_links}}", &build_social_links(&settings));
    html = html.replace("{{current_year}}", &chrono::Utc::now().format("%Y").to_string());
    html = html.replace("{{category_filter}}", &build_categories_sidebar(context));

    // ── Portfolio placeholders ──
    html = html.replace("{{portfolio_grid}}", &render_portfolio_grid(context));

    if let Some(item) = context.get("item") {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let image = item.get("image_path").and_then(|v| v.as_str()).unwrap_or("");
        let desc = item.get("description_html").and_then(|v| v.as_str()).unwrap_or("");
        let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);
        let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let portfolio_slug = sg("portfolio_slug", "portfolio");

        html = html.replace("{{portfolio_title}}", &format!("<h1>{}</h1>", html_escape(title)));
        html = html.replace("{{portfolio_image}}", &format!(r#"<div class="portfolio-image"><img src="/uploads/{}" alt="{}"></div>"#, image, html_escape(title)));
        html = html.replace("{{portfolio_description}}", desc);
        html = html.replace("{{portfolio_likes}}", &format!(
            r#"<span class="like-btn" data-id="{}">♥ <span class="like-count">{}</span></span>"#,
            item_id, format_likes(likes)
        ));

        // Portfolio meta (categories + tags)
        let mut meta_html = String::new();
        if let Some(Value::Array(cats)) = context.get("categories") {
            if !cats.is_empty() {
                meta_html.push_str(r#"<div class="portfolio-categories">"#);
                for cat in cats {
                    let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                    meta_html.push_str(&format!("<a href=\"{}\">{}</a> ", slug_url(&portfolio_slug, &format!("category/{}", slug)), html_escape(name)));
                }
                meta_html.push_str("</div>");
            }
        }
        if let Some(Value::Array(tags)) = context.get("tags") {
            if !tags.is_empty() {
                meta_html.push_str(r#"<div class="portfolio-tags">"#);
                for tag in tags {
                    let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let tslug = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                    meta_html.push_str(&format!("<a href=\"{}\">{}</a> ", slug_url(&portfolio_slug, &format!("tag/{}", tslug)), html_escape(name)));
                }
                meta_html.push_str("</div>");
            }
        }
        html = html.replace("{{portfolio_meta}}", &meta_html);
        html = html.replace("{{portfolio_categories}}", &meta_html);
        html = html.replace("{{portfolio_tags}}", "");

        // Commerce buy button
        let commerce_enabled = context.get("commerce_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        if commerce_enabled {
            let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let purchase_note = item.get("purchase_note").and_then(|v| v.as_str()).unwrap_or("");
            let payment_provider = item.get("payment_provider").and_then(|v| v.as_str()).unwrap_or("");
            html = html.replace("{{portfolio_buy_button}}", &build_commerce_html(price, purchase_note, item_id, &settings, payment_provider));
        } else {
            html = html.replace("{{portfolio_buy_button}}", "");
        }

        // Comments on portfolio
        let comments_on = context.get("comments_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        if comments_on {
            html = html.replace("{{post_comments}}", &build_comments_section(context, &settings, item_id, "portfolio"));
        } else {
            html = html.replace("{{post_comments}}", "");
        }
    }

    // ── Blog placeholders ──
    html = html.replace("{{blog_list}}", &render_blog_list(context));
    html = html.replace("{{blog_pagination}}", ""); // pagination is included in blog_list

    if let Some(post) = context.get("post") {
        let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let content = post.get("content_html").and_then(|v| v.as_str()).unwrap_or("");
        let raw_date = post.get("published_at").and_then(|v| v.as_str()).unwrap_or("");
        let date = format_date(raw_date, &settings);
        let featured = post.get("featured_image").and_then(|v| v.as_str()).unwrap_or("");
        let author = post.get("author_name").and_then(|v| v.as_str()).unwrap_or("");
        let excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

        html = html.replace("{{post_title}}", &format!("<h1>{}</h1>", html_escape(title)));
        html = html.replace("{{post_content}}", content);
        html = html.replace("{{post_date}}", &date);
        html = html.replace("{{post_author}}", &html_escape(author));
        html = html.replace("{{post_excerpt}}", &html_escape(excerpt));
        html = html.replace("{{post_meta}}", &format!(
            r#"<div class="blog-meta"><time>{}</time>{}</div>"#,
            date,
            if !author.is_empty() { format!(" · <span>{}</span>", html_escape(author)) } else { String::new() }
        ));

        if !featured.is_empty() {
            html = html.replace("{{post_featured_image}}", &format!(
                r#"<div class="featured-image"><img src="/uploads/{}" alt="{}"></div>"#,
                featured, html_escape(title)
            ));
        } else {
            html = html.replace("{{post_featured_image}}", "");
        }

        // Post tags
        if let Some(Value::Array(tags)) = context.get("tags") {
            let blog_slug = sg("blog_slug", "journal");
            let tag_html: String = tags.iter().filter_map(|t| {
                let name = t.get("name").and_then(|v| v.as_str())?;
                let tslug = t.get("slug").and_then(|v| v.as_str())?;
                Some(format!("<a href=\"/{}/tag/{}\">{}</a>", blog_slug, tslug, html_escape(name)))
            }).collect::<Vec<_>>().join(" · ");
            html = html.replace("{{post_tags}}", &format!(r#"<div class="post-tags">{}</div>"#, tag_html));
        } else {
            html = html.replace("{{post_tags}}", "");
        }

        // Post navigation (prev/next)
        let mut nav_html = String::new();
        if let Some(prev) = context.get("prev_post") {
            let blog_slug = sg("blog_slug", "journal");
            let pslug = prev.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let ptitle = prev.get("title").and_then(|v| v.as_str()).unwrap_or("Previous");
            nav_html.push_str(&format!(r#"<a href="/{}/{}" class="nav-prev">&larr; {}</a>"#, blog_slug, pslug, html_escape(ptitle)));
        }
        if let Some(next) = context.get("next_post") {
            let blog_slug = sg("blog_slug", "journal");
            let nslug = next.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let ntitle = next.get("title").and_then(|v| v.as_str()).unwrap_or("Next");
            nav_html.push_str(&format!(r#"<a href="/{}/{}" class="nav-next">{} &rarr;</a>"#, blog_slug, nslug, html_escape(ntitle)));
        }
        html = html.replace("{{post_navigation}}", &format!(r#"<nav class="post-nav">{}</nav>"#, nav_html));

        // Comments on blog post
        let comments_on = context.get("comments_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        if comments_on {
            html = html.replace("{{post_comments}}", &build_comments_section(context, &settings, post_id, "post"));
        } else {
            html = html.replace("{{post_comments}}", "");
        }
    }

    // ── Clean up any remaining unreplaced placeholders ──
    let html = strip_unreplaced_placeholders(&html);

    // ── Strip GrapesJS placeholder wrapper divs for clean output ──
    // The data-placeholder divs are visual aids in the editor; in production
    // we've already replaced the content above, so the wrapper styling is kept
    // but the label badge is removed.

    // ── SEO meta tags ──
    let seo_meta = context.get("seo").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let webmaster_meta = seo::build_webmaster_meta(&settings);
    let favicon_link = build_favicon_link(&settings);
    let analytics_scripts = seo::build_analytics_scripts(&settings);
    let cookie_consent = build_cookie_consent_banner(&settings);
    let back_to_top = build_back_to_top(&settings);

    let click_mode = sg("portfolio_click_mode", "lightbox");
    let show_likes = sg("portfolio_enable_likes", "true");
    let show_cats = sg("portfolio_show_categories", "true");
    let show_tags = sg("portfolio_show_tags", "true");
    let fade_anim = sg("portfolio_fade_animation", "true");
    let display_type = sg("portfolio_display_type", "masonry");
    let pagination_type = sg("portfolio_pagination_type", "classic");
    let lb_show_title = sg("portfolio_lightbox_show_title", "true");
    let lb_show_tags = sg("portfolio_lightbox_show_tags", "true");
    let lb_nav = sg("portfolio_lightbox_nav", "true");
    let lb_keyboard = sg("portfolio_lightbox_keyboard", "true");

    let image_protection_js = if sg("portfolio_image_protection", "false") == "true" {
        IMAGE_PROTECTION_JS
    } else {
        ""
    };

    // Wrap in full HTML document
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {seo_meta}
    {webmaster_meta}
    {favicon_link}
{font_links}    <style>
        {css_vars}
        {design_css}
    </style>
</head>
<body data-click-mode="{click_mode}" data-show-likes="{show_likes}" data-show-categories="{show_cats}" data-show-tags="{show_tags}" data-fade-animation="{fade_anim}" data-display-type="{display_type}" data-pagination-type="{pagination_type}" data-lb-show-title="{lb_show_title}" data-lb-show-tags="{lb_show_tags}" data-lb-nav="{lb_nav}" data-lb-keyboard="{lb_keyboard}">
    {body_html}
    {back_to_top}
    <script>{lightbox_js}</script>
    {image_protection_js}
    {analytics_scripts}
    {cookie_consent}
</body>
</html>"#,
        seo_meta = seo_meta,
        webmaster_meta = webmaster_meta,
        favicon_link = favicon_link,
        font_links = font_links,
        css_vars = css_vars,
        design_css = tmpl.style_css,
        body_html = html,
        back_to_top = back_to_top,
        lightbox_js = LIGHTBOX_JS,
        image_protection_js = image_protection_js,
        analytics_scripts = analytics_scripts,
        cookie_consent = cookie_consent,
        click_mode = click_mode,
        show_likes = show_likes,
        show_cats = show_cats,
        show_tags = show_tags,
        fade_anim = fade_anim,
        display_type = display_type,
        pagination_type = pagination_type,
        lb_show_title = lb_show_title,
        lb_show_tags = lb_show_tags,
        lb_nav = lb_nav,
        lb_keyboard = lb_keyboard,
    )
}

/// Phase 1 fallback: hardcoded default rendering.
fn render_page_default(pool: &DbPool, template_type: &str, context: &Value) -> String {
    let settings = context.get("settings").cloned().unwrap_or_default();

    // Build CSS variables from settings
    let css_vars = typography::build_css_variables(&settings);

    // Get the page-specific HTML
    let body_html = match template_type {
        "homepage" | "portfolio_grid" => render_portfolio_grid(context),
        "portfolio_single" => render_portfolio_single(context),
        "blog_list" => render_blog_list(context),
        "blog_single" => render_blog_single(context),
        "archives" => render_archives(context),
        "404" => render_404(context),
        _ => render_404(context),
    };

    // Get SEO meta tags
    let seo_meta = context
        .get("seo")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let site_name = settings
        .get("site_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Velocty");

    let site_tagline = settings
        .get("site_caption")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Settings helper
    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
    };

    // Build the sidebar categories (gated on portfolio_show_categories)
    let categories_html = if sg("portfolio_show_categories", "true") == "true" {
        build_categories_sidebar(context)
    } else {
        String::new()
    };

    // Build social links — position controlled by setting
    let social_pos = sg("social_icons_position", "sidebar");
    let social_full = build_social_links(&settings);
    let social_sidebar = if social_pos == "sidebar" || social_pos == "both" { social_full.clone() } else { String::new() };
    let social_footer = if social_pos == "footer" || social_pos == "both" { social_full.clone() } else { String::new() };

    // Build font loading tags
    let font_links = typography::build_font_links(&settings);
    let click_mode = sg("portfolio_click_mode", "lightbox");
    let show_likes = sg("portfolio_enable_likes", "true");
    let show_cats = sg("portfolio_show_categories", "true");
    let show_tags = sg("portfolio_show_tags", "true");
    let fade_anim = sg("portfolio_fade_animation", "true");
    let display_type = sg("portfolio_display_type", "masonry");
    let pagination_type = sg("portfolio_pagination_type", "classic");
    let lb_show_title = sg("portfolio_lightbox_show_title", "true");
    let lb_show_tags = sg("portfolio_lightbox_show_tags", "true");
    let lb_nav = sg("portfolio_lightbox_nav", "true");
    let lb_keyboard = sg("portfolio_lightbox_keyboard", "true");

    // Build additional nav links (journal, contact)
    let blog_slug = sg("blog_slug", "journal");
    let blog_label = sg("blog_label", "journal");
    let blog_enabled = sg("journal_enabled", "true") != "false";
    let contact_label = sg("contact_label", "catch up");
    let contact_enabled = sg("contact_page_enabled", "false") == "true";
    let copyright_text = sg("copyright_text", "");

    let portfolio_slug = sg("portfolio_slug", "portfolio");
    let portfolio_label = sg("portfolio_label", "experiences");
    let portfolio_enabled = sg("portfolio_enabled", "false") == "true";

    let mut nav_links = String::new();
    if portfolio_enabled {
        nav_links.push_str(&format!(
            "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
            slug_url(&portfolio_slug, ""), html_escape(&portfolio_label)
        ));
    }
    if blog_enabled {
        nav_links.push_str(&format!(
            "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
            slug_url(&blog_slug, ""), html_escape(&blog_label)
        ));
    }
    if contact_enabled {
        nav_links.push_str(&format!(
            "<a href=\"/contact\" class=\"nav-link\">{}</a>\n",
            html_escape(&contact_label)
        ));
    }

    // Build copyright / footer text — only show if user has set it
    let footer_copyright = if !copyright_text.is_empty() {
        format!("<div class=\"footer-text\">{}</div>", html_escape(&copyright_text))
    } else {
        String::new()
    };

    // Full page shell — the default "Sidebar Portfolio" design
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {seo_meta}
    {webmaster_meta}
    {favicon_link}
{font_links}    <style>
        {css_vars}
        {base_css}
    </style>
</head>
<body data-click-mode="{click_mode}" data-show-likes="{show_likes}" data-show-categories="{show_cats}" data-show-tags="{show_tags}" data-fade-animation="{fade_anim}" data-display-type="{display_type}" data-pagination-type="{pagination_type}" data-lb-show-title="{lb_show_title}" data-lb-show-tags="{lb_show_tags}" data-lb-nav="{lb_nav}" data-lb-keyboard="{lb_keyboard}">
    <div class="mobile-header">
        {logo_html_mobile}
        <button class="mobile-menu-btn" onclick="document.querySelector('.sidebar').classList.toggle('mobile-open')">&#9776;</button>
    </div>
    <div class="site-wrapper">
        <aside class="sidebar">
            <div class="sidebar-top">
                <div class="site-logo">
                    {logo_html}
                    <h1 class="site-name"><a href="/">{site_name}</a></h1>
                    <p class="site-tagline">{tagline}</p>
                </div>
                <nav class="category-nav">
                    {categories_html}
                    {nav_links}
                </nav>
            </div>
            <div class="sidebar-bottom">
                {social_sidebar}
                {footer_legal_links}
                {footer_copyright}
            </div>
        </aside>
        <main class="content">
            {body_html}
        </main>
        <footer class="site-footer">
            {social_footer}
        </footer>
    </div>
    {back_to_top}
    <script>{lightbox_js}</script>
    {image_protection_js}
    {analytics_scripts}
    {cookie_consent}
</body>
</html>"#,
        seo_meta = seo_meta,
        webmaster_meta = seo::build_webmaster_meta(&settings),
        favicon_link = build_favicon_link(&settings),
        font_links = font_links,
        css_vars = css_vars,
        base_css = DEFAULT_CSS,
        logo_html = build_logo_html(&settings),
        logo_html_mobile = build_logo_html(&settings),
        site_name = html_escape(site_name),
        tagline = html_escape(site_tagline),
        categories_html = categories_html,
        nav_links = nav_links,
        social_sidebar = social_sidebar,
        social_footer = social_footer,
        footer_legal_links = build_footer_legal_links(&settings),
        footer_copyright = footer_copyright,
        body_html = body_html,
        back_to_top = build_back_to_top(&settings),
        lightbox_js = LIGHTBOX_JS,
        image_protection_js = if settings
            .get("portfolio_image_protection")
            .and_then(|v| v.as_str())
            .unwrap_or("false") == "true"
        {
            IMAGE_PROTECTION_JS
        } else {
            ""
        },
        analytics_scripts = seo::build_analytics_scripts(&settings),
        cookie_consent = build_cookie_consent_banner(&settings),
        click_mode = click_mode,
        show_likes = show_likes,
        show_cats = show_cats,
        show_tags = show_tags,
        fade_anim = fade_anim,
        display_type = display_type,
        pagination_type = pagination_type,
        lb_show_title = lb_show_title,
        lb_show_tags = lb_show_tags,
        lb_nav = lb_nav,
        lb_keyboard = lb_keyboard,
    )
}

/// Renders a legal page (Privacy Policy, Terms of Use) using the same site shell.
pub fn render_legal_page(
    pool: &DbPool,
    settings: &std::collections::HashMap<String, String>,
    title: &str,
    html_body: &str,
) -> String {
    let settings_json = serde_json::to_value(settings).unwrap_or_default();
    let css_vars = typography::build_css_variables(&settings_json);
    let social_html = build_social_links(&settings_json);

    let site_name = settings.get("site_name").map(|s| s.as_str()).unwrap_or("Velocty");
    let site_tagline = settings.get("site_caption").map(|s| s.as_str()).unwrap_or("");

    let show_cats = settings.get("portfolio_show_categories").map(|s| s.as_str()).unwrap_or("true") == "true";
    let categories_html = if show_cats {
        let categories = crate::models::category::Category::list(pool, Some("portfolio"));
        let cats_json = serde_json::to_value(&categories).unwrap_or_default();
        let ctx = serde_json::json!({ "categories": cats_json });
        build_categories_sidebar(&ctx)
    } else {
        String::new()
    };

    let analytics_scripts = seo::build_analytics_scripts(&settings_json);
    let font_links = typography::build_font_links(&settings_json);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} — {site_name}</title>
    {favicon_link}
{font_links}    <style>
        {css_vars}
        {base_css}
        .legal-content {{
            max-width: 780px;
            padding: 40px 24px;
            line-height: 1.8;
            color: var(--color-text);
        }}
        .legal-content h1 {{ font-size: 2rem; font-weight: 700; margin-bottom: 8px; }}
        .legal-content h2 {{ font-size: 1.35rem; font-weight: 600; margin-top: 2em; margin-bottom: 0.5em; border-bottom: 1px solid #e5e7eb; padding-bottom: 6px; }}
        .legal-content h3 {{ font-size: 1.1rem; font-weight: 600; margin-top: 1.5em; margin-bottom: 0.4em; }}
        .legal-content p {{ margin-bottom: 1em; }}
        .legal-content ul, .legal-content ol {{ margin-bottom: 1em; padding-left: 1.5em; }}
        .legal-content li {{ margin-bottom: 0.3em; }}
        .legal-content strong {{ font-weight: 600; }}
        .legal-content code {{ background: #f3f4f6; padding: 2px 6px; border-radius: 3px; font-size: 0.9em; }}
        .legal-content a {{ color: var(--color-accent); text-decoration: underline; }}
    </style>
</head>
<body>
    <div class="site-wrapper">
        <aside class="sidebar">
            <div class="sidebar-top">
                <div class="site-logo">
                    {logo_html}
                    <h1 class="site-name">{site_name}</h1>
                    <p class="site-tagline">{tagline}</p>
                </div>
                <nav class="category-nav">
                    {categories_html}
                </nav>
                <a href="/archives" class="archives-link">archives</a>
            </div>
            <div class="sidebar-bottom">
                {social_html}
                {footer_legal_links}
                <div class="footer-text">
                    <p>&copy; {year} {site_name}</p>
                </div>
            </div>
        </aside>
        <main class="content">
            <div class="legal-content">
                {body}
            </div>
        </main>
    </div>
    {back_to_top}
    {analytics_scripts}
    {cookie_consent}
</body>
</html>"#,
        title = html_escape(title),
        site_name = html_escape(site_name),
        tagline = html_escape(site_tagline),
        favicon_link = build_favicon_link(&settings_json),
        logo_html = build_logo_html(&settings_json),
        font_links = font_links,
        css_vars = css_vars,
        base_css = DEFAULT_CSS,
        categories_html = categories_html,
        social_html = social_html,
        footer_legal_links = build_footer_legal_links(&settings_json),
        year = chrono::Utc::now().format("%Y"),
        body = html_body,
        back_to_top = build_back_to_top(&settings_json),
        analytics_scripts = analytics_scripts,
        cookie_consent = build_cookie_consent_banner(&settings_json),
    )
}

/// Reusable comment display + form for blog and portfolio pages.
/// Renders approved comments (threaded) and the submission form with captcha.
fn build_comments_section(context: &Value, settings: &Value, content_id: i64, content_type: &str) -> String {
    let mut html = String::new();

    // Render existing comments (threaded)
    if let Some(Value::Array(comments)) = context.get("comments") {
        // Separate top-level and replies
        let top: Vec<&Value> = comments.iter().filter(|c| c.get("parent_id").and_then(|v| v.as_i64()).is_none()).collect();
        let replies: Vec<&Value> = comments.iter().filter(|c| c.get("parent_id").and_then(|v| v.as_i64()).is_some()).collect();

        if !comments.is_empty() {
            html.push_str(&format!(
                r#"<section class="comments"><h3>Comments ({})</h3>"#,
                comments.len()
            ));
            for comment in &top {
                render_comment(&mut html, comment, &replies, 0);
            }
            html.push_str("</section>");
        }
    }

    // Comment form
    let sg = |key: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };
    let require_name = sg("comments_require_name") != "false";
    let require_email = sg("comments_require_email") == "true";
    let name_req = if require_name { " required" } else { "" };
    let email_req = if require_email { " required" } else { "" };

    let (captcha_provider, captcha_site_key): (String, String) = if sg("security_recaptcha_enabled") == "true" {
        ("recaptcha".into(), sg("security_recaptcha_site_key"))
    } else if sg("security_turnstile_enabled") == "true" {
        ("turnstile".into(), sg("security_turnstile_site_key"))
    } else if sg("security_hcaptcha_enabled") == "true" {
        ("hcaptcha".into(), sg("security_hcaptcha_site_key"))
    } else {
        (String::new(), String::new())
    };
    let mut captcha_html = String::new();
    let mut captcha_script = String::new();
    let mut captcha_get_token_js = "null".to_string();

    if !captcha_provider.is_empty() && !captcha_site_key.is_empty() {
        match captcha_provider.as_str() {
            "recaptcha" => {
                let version = settings.get("security_recaptcha_version").and_then(|v| v.as_str()).unwrap_or("v3");
                if version == "v3" {
                    captcha_script = format!(r#"<script src="https://www.google.com/recaptcha/api.js?render={}"></script>"#, captcha_site_key);
                    captcha_get_token_js = format!("function(){{return grecaptcha.execute('{}',{{action:'comment'}})}}", captcha_site_key);
                } else {
                    captcha_script = "https://www.google.com/recaptcha/api.js".to_string();
                    captcha_html = format!(r#"<div class="g-recaptcha" data-sitekey="{}"></div>"#, captcha_site_key);
                    captcha_get_token_js = "function(){return Promise.resolve(grecaptcha.getResponse())}".to_string();
                }
            }
            "turnstile" => {
                captcha_script = "https://challenges.cloudflare.com/turnstile/v0/api.js".to_string();
                captcha_html = format!(r#"<div class="cf-turnstile" data-sitekey="{}"></div>"#, captcha_site_key);
                captcha_get_token_js = "function(){return Promise.resolve(document.querySelector('[name=cf-turnstile-response]').value)}".to_string();
            }
            "hcaptcha" => {
                captcha_script = "https://js.hcaptcha.com/1/api.js".to_string();
                captcha_html = format!(r#"<div class="h-captcha" data-sitekey="{}"></div>"#, captcha_site_key);
                captcha_get_token_js = "function(){return Promise.resolve(hcaptcha.getResponse())}".to_string();
            }
            _ => {}
        }
        if captcha_script.starts_with("https://") {
            captcha_script = format!(r#"<script src="{}"></script>"#, captcha_script);
        }
    }

    html.push_str(&format!(
        "<section class=\"comment-form\">\
\n    <h3>Leave a Comment</h3>\
\n    {captcha_script}\
\n    <form id=\"comment-form\" data-post-id=\"{content_id}\" data-content-type=\"{content_type}\">\
\n        <input type=\"hidden\" name=\"parent_id\" value=\"\">\
\n        <div id=\"reply-indicator\" style=\"display:none;margin-bottom:8px;font-size:13px;color:var(--color-text-secondary)\">\
\n            Replying to <strong id=\"reply-to-name\"></strong> <a href=\"#\" id=\"cancel-reply\" style=\"margin-left:8px\">Cancel</a>\
\n        </div>\
\n        <input type=\"text\" name=\"author_name\" placeholder=\"Name\"{name_req}>\
\n        <input type=\"email\" name=\"author_email\" placeholder=\"Email\"{email_req}>\
\n        <textarea name=\"body\" placeholder=\"Your comment...\" required></textarea>\
\n        <div style=\"display:none\"><input type=\"text\" name=\"honeypot\"></div>\
\n        {captcha_html}\
\n        <button type=\"submit\">Submit</button>\
\n        <p id=\"comment-msg\" style=\"margin-top:8px;font-size:13px;display:none\"></p>\
\n    </form>\
\n</section>\
\n<script>\
\n(function(){{\
\nvar f=document.getElementById('comment-form');\
\nif(!f)return;\
\nvar getToken={captcha_get_token_js};\
\ndocument.querySelectorAll('.reply-btn').forEach(function(btn){{\
\n    btn.addEventListener('click',function(e){{\
\n        e.preventDefault();\
\n        f.querySelector('[name=parent_id]').value=this.dataset.id;\
\n        document.getElementById('reply-to-name').textContent=this.dataset.name;\
\n        document.getElementById('reply-indicator').style.display='';\
\n        f.querySelector('[name=body]').focus();\
\n    }});\
\n}});\
\nvar cancelReply=document.getElementById('cancel-reply');\
\nif(cancelReply)cancelReply.addEventListener('click',function(e){{\
\n    e.preventDefault();\
\n    f.querySelector('[name=parent_id]').value='';\
\n    document.getElementById('reply-indicator').style.display='none';\
\n}});\
\nf.addEventListener('submit',function(e){{\
\n    e.preventDefault();\
\n    var btn=f.querySelector('button[type=submit]');\
\n    btn.disabled=true;btn.textContent='Submitting...';\
\n    var msg=document.getElementById('comment-msg');\
\n    msg.style.display='none';\
\n    var parentVal=f.querySelector('[name=parent_id]').value;\
\n    var data={{\
\n        post_id:parseInt(f.dataset.postId),\
\n        content_type:f.dataset.contentType||'post',\
\n        author_name:f.querySelector('[name=author_name]').value,\
\n        author_email:f.querySelector('[name=author_email]').value||null,\
\n        body:f.querySelector('[name=body]').value,\
\n        honeypot:f.querySelector('[name=honeypot]').value||null,\
\n        parent_id:parentVal?parseInt(parentVal):null\
\n    }};\
\n    var go=function(token){{\
\n        if(token)data.captcha_token=token;\
\n        fetch('/api/comment',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(data)}})\
\n        .then(function(r){{return r.json()}})\
\n        .then(function(j){{\
\n            msg.style.display='';\
\n            if(j.success){{msg.style.color='green';msg.textContent=j.message||'Comment submitted';f.reset();\
\n                f.querySelector('[name=parent_id]').value='';\
\n                document.getElementById('reply-indicator').style.display='none';\
\n            }}\
\n            else{{msg.style.color='red';msg.textContent=j.error||'Failed';}}\
\n        }})\
\n        .catch(function(){{msg.style.display='';msg.style.color='red';msg.textContent='Network error';}})\
\n        .finally(function(){{btn.disabled=false;btn.textContent='Submit';}});\
\n    }};\
\n    if(typeof getToken==='function'){{\
\n        Promise.resolve(getToken()).then(go).catch(function(){{go(null)}});\
\n    }}else{{go(null);}}\
\n}});\
\n}})();\
\n</script>",
        captcha_script = captcha_script,
        content_id = content_id,
        content_type = content_type,
        name_req = name_req,
        email_req = email_req,
        captcha_html = captcha_html,
        captcha_get_token_js = captcha_get_token_js,
    ));

    html
}

/// Render a single comment and its nested replies recursively.
fn render_comment(html: &mut String, comment: &Value, all_replies: &[&Value], depth: usize) {
    let id = comment.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let name = comment.get("author_name").and_then(|v| v.as_str()).unwrap_or("Anonymous");
    let body = comment.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let cdate = comment.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
    let indent = if depth > 0 {
        format!(" style=\"margin-left:{}px;border-left:2px solid #e5e7eb;padding-left:12px\"", depth.min(3) * 24)
    } else {
        String::new()
    };
    let escaped_name = html_escape(name);
    let escaped_body = html_escape(body);

    html.push_str(&format!(
        "<div class=\"comment\"{}><strong>{}</strong> <time>{}</time> \
         <a href=\"#\" class=\"reply-btn\" data-id=\"{}\" data-name=\"{}\" \
         style=\"font-size:12px;color:var(--color-accent);margin-left:8px\">Reply</a>\
         <p>{}</p></div>",
        indent, escaped_name, cdate, id, escaped_name, escaped_body,
    ));

    // Render child replies
    let children: Vec<&&Value> = all_replies.iter().filter(|r| r.get("parent_id").and_then(|v| v.as_i64()) == Some(id)).collect();
    for child in children {
        render_comment(html, child, all_replies, depth + 1);
    }
}

fn build_categories_sidebar(context: &Value) -> String {
    let categories = match context.get("categories") {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let portfolio_slug = settings.get("portfolio_slug").and_then(|v| v.as_str()).unwrap_or("portfolio");
    let portfolio_label = settings.get("portfolio_label").and_then(|v| v.as_str()).unwrap_or("experiences");

    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    // Build collapsible category dropdown
    let mut html = String::new();

    // Portfolio categories as collapsible group
    if !categories.is_empty() {
        html.push_str(&format!(
            "<div class=\"nav-category-group\">\
             <button class=\"nav-category-toggle open\" onclick=\"this.classList.toggle('open');this.nextElementSibling.classList.toggle('open')\">\
             <span>{}</span> <span class=\"arrow\">&#9662;</span></button>\
             <div class=\"nav-subcategories open\">",
            html_escape(portfolio_label)
        ));

        // "all" link
        let all_active = if active_slug.is_empty() { " active" } else { "" };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">all</a>\n",
            slug_url(portfolio_slug, ""), all_active
        ));

        for cat in categories {
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if slug.is_empty() { continue; }
            let active_class = if slug == active_slug { " active" } else { "" };
            html.push_str(&format!(
                "<a href=\"{}\" class=\"cat-link{}\">{}</a>\n",
                slug_url(portfolio_slug, &format!("category/{}", slug)), active_class, html_escape(name)
            ));
        }

        html.push_str("</div></div>\n");
    }

    html
}

fn build_social_links(settings: &Value) -> String {
    let sg = |key: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    let brand_colors = sg("social_brand_colors") == "true";

    // (setting_key, platform_label, icon_svg, brand_color)
    let platforms: &[(&str, &str, &str, &str)] = &[
        ("social_instagram", "Instagram",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="20" rx="5"/><path d="M16 11.37A4 4 0 1 1 12.63 8 4 4 0 0 1 16 11.37z"/><line x1="17.5" y1="6.5" x2="17.51" y2="6.5"/></svg>"#,
         "#E4405F"),
        ("social_twitter", "X",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>"#,
         "#1DA1F2"),
        ("social_facebook", "Facebook",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 2h-3a5 5 0 0 0-5 5v3H7v4h3v8h4v-8h3l1-4h-4V7a1 1 0 0 1 1-1h3z"/></svg>"#,
         "#1877F2"),
        ("social_youtube", "YouTube",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22.54 6.42a2.78 2.78 0 0 0-1.94-2C18.88 4 12 4 12 4s-6.88 0-8.6.46a2.78 2.78 0 0 0-1.94 2A29 29 0 0 0 1 11.75a29 29 0 0 0 .46 5.33A2.78 2.78 0 0 0 3.4 19.1c1.72.46 8.6.46 8.6.46s6.88 0 8.6-.46a2.78 2.78 0 0 0 1.94-2 29 29 0 0 0 .46-5.25 29 29 0 0 0-.46-5.33z"/><polygon points="9.75 15.02 15.5 11.75 9.75 8.48 9.75 15.02"/></svg>"#,
         "#FF0000"),
        ("social_tiktok", "TikTok",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 12a4 4 0 1 0 4 4V4a5 5 0 0 0 5 5"/></svg>"#,
         "#ff0050"),
        ("social_linkedin", "LinkedIn",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 8a6 6 0 0 1 6 6v7h-4v-7a2 2 0 0 0-2-2 2 2 0 0 0-2 2v7h-4v-7a6 6 0 0 1 6-6z"/><rect x="2" y="9" width="4" height="12"/><circle cx="4" cy="4" r="2"/></svg>"#,
         "#0A66C2"),
        ("social_pinterest", "Pinterest",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 12a4 4 0 1 1 8 0c0 2.5-1.5 5-4 5s-2.5-1-2.5-1l-1 4"/><circle cx="12" cy="12" r="10"/></svg>"#,
         "#BD081C"),
        ("social_behance", "Behance",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M22 7h-7V5h7v2zm1.726 10c-.442 1.297-2.029 3-5.101 3-3.074 0-5.564-1.729-5.564-5.675 0-3.91 2.325-5.92 5.466-5.92 3.082 0 4.964 1.782 5.375 4.426.078.506.109 1.188.095 2.14H15.97c.13 3.211 3.483 3.312 4.588 2.029h3.168z"/></svg>"#,
         "#1769FF"),
        ("social_dribbble", "Dribbble",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M19.13 5.09C15.22 9.14 10 10.44 2.25 10.94"/><path d="M21.75 12.84c-6.62-1.41-12.14 1-16.38 6.32"/><path d="M8.56 2.75c4.37 6 6 12.56 6.44 19.5"/></svg>"#,
         "#EA4C89"),
        ("social_github", "GitHub",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"/></svg>"#,
         "#f0f0f0"),
        ("social_vimeo", "Vimeo",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 8.5c-.1 2.1-1.5 5-4.4 8.6C14.5 20.7 12 22 10 22c-1.3 0-2.3-1.2-3.1-3.5-.6-2-1.1-4-1.7-6-.6-2.3-1.3-3.5-2-3.5-.2 0-.7.3-1.5.9L.5 8.5c1-.9 2-1.7 2.9-2.6C4.8 4.7 5.8 4 6.5 4c1.8-.2 2.8 1 3.2 3.5.4 2.7.6 4.4.8 5 .5 2 .9 3 1.4 3 .4 0 1-.6 1.8-1.9.8-1.3 1.2-2.3 1.3-3 .1-1.2-.3-1.8-1.4-1.8-.5 0-1 .1-1.5.3 1-3.3 2.9-4.9 5.7-4.8 2.1.1 3.1 1.4 2.9 4.2z"/></svg>"#,
         "#1AB7EA"),
        ("social_500px", "500px",
         r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="4"/></svg>"#,
         "#0099E5"),
    ];

    let collected: Vec<(String, String, &str, &str)> = platforms.iter()
        .filter_map(|&(key, label, icon, color)| {
            let url = sg(key);
            if url.is_empty() { None } else { Some((label.to_string(), url, icon, color)) }
        })
        .collect();

    if collected.is_empty() {
        return String::new();
    }

    let mut html = String::from("<div class=\"social-links\">");
    for (label, url, icon, color) in &collected {
        let style = if brand_colors { format!(" style=\"color:{}\"", color) } else { String::new() };
        html.push_str(&format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\" title=\"{}\"{}>{}</a>\n",
            html_escape(url), html_escape(label), style, icon
        ));
    }
    html.push_str("</div>");
    html
}


/// Build share buttons for single post/portfolio pages.
/// Reads share_enabled, share_facebook, share_x, share_linkedin from settings.
fn build_share_buttons(settings: &Value, page_url: &str, page_title: &str) -> String {
    let sg = |key: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    if sg("share_enabled") != "true" {
        return String::new();
    }

    let encoded_url = urlencoding_simple(page_url);
    let encoded_title = urlencoding_simple(page_title);

    let mut buttons = Vec::new();

    if sg("share_facebook") == "true" {
        buttons.push(format!(
            r#"<a href="https://www.facebook.com/sharer/sharer.php?u={url}" target="_blank" rel="noopener" class="share-btn share-facebook" title="Share on Facebook"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z"/></svg> Facebook</a>"#,
            url = encoded_url
        ));
    }

    if sg("share_x") == "true" {
        buttons.push(format!(
            r#"<a href="https://x.com/intent/tweet?url={url}&text={title}" target="_blank" rel="noopener" class="share-btn share-x" title="Share on X"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg> Post</a>"#,
            url = encoded_url,
            title = encoded_title
        ));
    }

    if sg("share_linkedin") == "true" {
        buttons.push(format!(
            r#"<a href="https://www.linkedin.com/sharing/share-offsite/?url={url}" target="_blank" rel="noopener" class="share-btn share-linkedin" title="Share on LinkedIn"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 0 1-2.063-2.065 2.064 2.064 0 1 1 2.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg> LinkedIn</a>"#,
            url = encoded_url
        ));
    }

    if buttons.is_empty() {
        return String::new();
    }

    format!("<div class=\"share-buttons\">{}</div>", buttons.join("\n"))
}

/// Build a URL path from a slug prefix and an optional sub-path.
/// When slug is empty, the feature owns "/" so we avoid double slashes.
/// e.g. slug_url("portfolio", "my-item") => "/portfolio/my-item"
///      slug_url("", "my-item")          => "/my-item"
///      slug_url("portfolio", "")         => "/portfolio"
///      slug_url("", "")                  => "/"
fn slug_url(slug: &str, sub: &str) -> String {
    match (slug.is_empty(), sub.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{}", sub),
        (false, true) => format!("/{}", slug),
        (false, false) => format!("/{}/{}", slug, sub),
    }
}

fn build_favicon_link(settings: &Value) -> String {
    let favicon = settings
        .get("site_favicon")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if favicon.is_empty() {
        return String::new();
    }
    format!("<link rel=\"icon\" href=\"{}\">", html_escape(favicon))
}

fn build_logo_html(settings: &Value) -> String {
    let logo = settings
        .get("site_logo")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if logo.is_empty() {
        return String::new();
    }
    let site_name = settings
        .get("site_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Velocty");
    format!(
        "<a href=\"/\"><img src=\"{}\" alt=\"{}\" class=\"logo-img\"></a>",
        html_escape(logo),
        html_escape(site_name)
    )
}

fn build_footer_legal_links(settings: &Value) -> String {
    let get = |key: &str| -> &str {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("")
    };
    let privacy = get("privacy_policy_enabled") == "true";
    let terms = get("terms_of_use_enabled") == "true";
    if !privacy && !terms {
        return String::new();
    }
    let mut html = String::from("<div class=\"footer-legal\">");
    if privacy {
        html.push_str("<a href=\"/privacy\">Privacy Policy</a>");
    }
    if privacy && terms {
        html.push_str(" · ");
    }
    if terms {
        html.push_str("<a href=\"/terms\">Terms of Use</a>");
    }
    html.push_str("</div>");
    html
}

fn build_back_to_top(settings: &Value) -> String {
    let enabled = settings
        .get("design_back_to_top")
        .and_then(|v| v.as_str())
        .unwrap_or("false") == "true";
    if !enabled {
        return String::new();
    }
    r#"<button id="back-to-top" aria-label="Back to top" style="display:none;position:fixed;bottom:24px;right:24px;z-index:999;width:40px;height:40px;border-radius:50%;border:1px solid #ddd;background:rgba(255,255,255,0.9);cursor:pointer;font-size:18px;line-height:1;box-shadow:0 2px 8px rgba(0,0,0,0.1);transition:opacity 0.3s">↑</button>
<script>
(function(){
var btn=document.getElementById('back-to-top');
if(!btn)return;
window.addEventListener('scroll',function(){btn.style.display=window.scrollY>300?'block':'none';});
btn.addEventListener('click',function(){window.scrollTo({top:0,behavior:'smooth'});});
})();
</script>"#.to_string()
}

fn build_cookie_consent_banner(settings: &Value) -> String {
    let get = |key: &str| -> &str {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
    };
    if get("cookie_consent_enabled") != "true" {
        return String::new();
    }

    let style = get("cookie_consent_style"); // minimal, modal, corner
    let position = get("cookie_consent_position"); // bottom, top
    let policy_url = get("cookie_consent_policy_url");
    let policy_url = if policy_url.is_empty() { "/privacy" } else { policy_url };
    let show_reject = get("cookie_consent_show_reject") == "true";
    let theme = get("cookie_consent_theme"); // auto, dark, light

    // Position CSS
    let pos_css = match (style, position) {
        ("modal", _) => "position:fixed;top:0;left:0;right:0;bottom:0;display:flex;align-items:center;justify-content:center;z-index:99999;background:rgba(0,0,0,0.5)",
        ("corner", _) => "position:fixed;bottom:20px;left:20px;z-index:99999;max-width:380px",
        (_, "top") => "position:fixed;top:20px;left:50%;transform:translateX(-50%);z-index:99999;max-width:580px;width:calc(100% - 40px)",
        _ => "position:fixed;bottom:20px;left:50%;transform:translateX(-50%);z-index:99999;max-width:580px;width:calc(100% - 40px)",
    };

    // Theme colors
    let (bg, text, border, btn_bg, btn_text) = match theme {
        "light" => ("#ffffff", "#1f2937", "#e5e7eb", "#111827", "#ffffff"),
        "dark" => ("#1f2937", "#f3f4f6", "#374151", "#f3f4f6", "#1f2937"),
        _ => ("#1f2937", "#f3f4f6", "#374151", "#f3f4f6", "#1f2937"), // auto = dark
    };

    // Link color: use text color so it's always readable
    let link_color = text;

    let reject_btn = if show_reject {
        format!(r#"<button id="cc-reject" style="padding:8px 20px;border-radius:6px;border:1px solid {border};font-size:13px;font-weight:500;cursor:pointer;background:transparent;color:{text}">Reject All</button>"#)
    } else {
        String::new()
    };

    let inner_style = if style == "modal" {
        format!("background:{bg};color:{text};border:1px solid {border};border-radius:12px;padding:28px 32px;max-width:480px;width:90%;box-shadow:0 20px 60px rgba(0,0,0,0.3)")
    } else if style == "corner" {
        format!("background:{bg};color:{text};border:1px solid {border};border-radius:12px;padding:20px 24px;box-shadow:0 8px 30px rgba(0,0,0,0.2)")
    } else {
        format!("background:{bg};color:{text};border:1px solid {border};border-radius:12px;padding:18px 24px;box-shadow:0 8px 30px rgba(0,0,0,0.25)")
    };

    let btns_style = "display:flex;gap:8px;margin-top:14px;justify-content:flex-end;flex-wrap:wrap";

    format!(
        r##"<div id="cc-banner" style="{pos_css}">
<div style="{inner_style}">
<div style="font-size:13px;line-height:1.6">
<strong style="font-size:14px">🍪 We use cookies</strong><br>
We use cookies to improve your experience. <a href="{policy_url}" style="color:{link_color};text-decoration:underline">Learn more</a>
</div>
<div style="{btns_style}">
{reject_btn}
<button id="cc-necessary" style="padding:8px 20px;border-radius:6px;border:1px solid {border};font-size:13px;font-weight:500;cursor:pointer;background:transparent;color:{text}">Necessary Only</button>
<button id="cc-accept" style="padding:8px 20px;border-radius:6px;border:none;font-size:13px;font-weight:600;cursor:pointer;background:{btn_bg};color:{btn_text}">Accept All</button>
</div>
</div>
</div>
<script>
(function(){{
var b=document.getElementById('cc-banner');
if(!b)return;
var c=document.cookie.match(/velocty_consent=([^;]+)/);
if(c){{b.remove();if(c[1]==='all')loadAnalytics();return;}}
function set(v){{document.cookie='velocty_consent='+v+';path=/;max-age=31536000;SameSite=Lax';b.remove();if(v==='all')loadAnalytics();}}
document.getElementById('cc-accept').onclick=function(){{set('all');}};
document.getElementById('cc-necessary').onclick=function(){{set('necessary');}};
var rj=document.getElementById('cc-reject');if(rj)rj.onclick=function(){{set('none');}};
function loadAnalytics(){{document.querySelectorAll('script[data-consent="analytics"]').forEach(function(s){{var n=document.createElement('script');if(s.src)n.src=s.src;else n.textContent=s.textContent;n.async=true;Array.from(s.attributes).forEach(function(a){{if(a.name!=='type'&&a.name!=='data-consent')n.setAttribute(a.name,a.value);}});document.head.appendChild(n);}});}}
}})();
</script>"##,
        pos_css = pos_css,
        inner_style = inner_style,
        policy_url = html_escape(policy_url),
        link_color = link_color,
        btn_bg = btn_bg,
        border = border,
        text = text,
        btn_text = btn_text,
        reject_btn = reject_btn,
        btns_style = btns_style,
    )
}


fn format_date_iso8601(raw: &str, settings: &Value) -> String {
    let tz_name = settings
        .get("timezone")
        .and_then(|v| v.as_str())
        .unwrap_or("UTC");

    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&format!("{} 00:00:00", raw), "%Y-%m-%d %H:%M:%S"));

    match naive {
        Ok(ndt) => {
            let utc_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
            if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
                utc_dt.with_timezone(&tz).format("%Y-%m-%dT%H:%M:%S%:z").to_string()
            } else {
                utc_dt.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
            }
        }
        Err(_) => raw.to_string(),
    }
}

fn format_date(raw: &str, settings: &Value) -> String {
    let fmt = settings
        .get("date_format")
        .and_then(|v| v.as_str())
        .unwrap_or("%B %d, %Y");
    let tz_name = settings
        .get("timezone")
        .and_then(|v| v.as_str())
        .unwrap_or("UTC");

    // Try parsing common DB formats: "YYYY-MM-DD HH:MM:SS" or "YYYY-MM-DD"
    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&format!("{} 00:00:00", raw), "%Y-%m-%d %H:%M:%S"));

    match naive {
        Ok(ndt) => {
            // Try to apply timezone offset
            let utc_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
            if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
                utc_dt.with_timezone(&tz).format(fmt).to_string()
            } else {
                utc_dt.format(fmt).to_string()
            }
        }
        Err(_) => raw.to_string(), // Fallback: return raw string
    }
}

fn truncate_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        let mut result = words[..max_words].join(" ");
        result.push_str("…");
        result
    }
}

fn build_pagination(current: i64, total: i64) -> String {
    let mut html = String::from(r#"<nav class="pagination">"#);
    if current > 1 {
        html.push_str(&format!(r#"<a href="?page={}">&laquo; Prev</a>"#, current - 1));
    }
    for p in 1..=total {
        if p == current {
            html.push_str(&format!(r#"<span class="current">{}</span>"#, p));
        } else {
            html.push_str(&format!(r#"<a href="?page={}">{}</a>"#, p, p));
        }
    }
    if current < total {
        html.push_str(&format!(r#"<a href="?page={}">Next &raquo;</a>"#, current + 1));
    }
    html.push_str("</nav>");
    html
}

fn render_portfolio_grid(context: &Value) -> String {
    let items = match context.get("items") {
        Some(Value::Array(items)) => items,
        _ => return "<p>No portfolio items yet.</p>".to_string(),
    };

    if items.is_empty() {
        return "<p>No portfolio items yet.</p>".to_string();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
    };
    let display_type = sg("portfolio_display_type", "masonry");
    let show_tags = sg("portfolio_show_tags", "true") == "true";
    let _show_likes = sg("portfolio_enable_likes", "true") == "true";
    let fade_anim = sg("portfolio_fade_animation", "true") == "true";

    let grid_class = if display_type == "grid" { "css-grid" } else { "masonry-grid" };
    let item_class = if fade_anim { "grid-item fade-in" } else { "grid-item" };

    let mut html = format!(r#"<div class="{}">"#, grid_class);

    let portfolio_slug = sg("portfolio_slug", "portfolio");

    for entry in items {
        let item = entry.get("item").unwrap_or(entry);
        let tags = entry.get("tags").and_then(|t| t.as_array());

        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let image = item
            .get("thumbnail_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("image_path").and_then(|v| v.as_str()))
            .unwrap_or("");
        let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);

        let cats_data = entry
            .get("categories")
            .and_then(|c| c.as_array())
            .map(|cats| cats
                .iter()
                .filter_map(|c| c.get("slug").and_then(|s| s.as_str()))
                .collect::<Vec<_>>()
                .join(" "))
            .unwrap_or_default();

        let tag_data = if show_tags {
            tags.map(|tl| tl.iter()
                .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(", "))
                .unwrap_or_default()
        } else {
            String::new()
        };

        let item_url = slug_url(&portfolio_slug, slug);
        html.push_str(&format!(
            r#"<div class="{item_class}" data-categories="{cats_data}">
    <a href="{item_url}" class="portfolio-link" data-title="{title}" data-likes="{likes}" data-tags="{tag_data}">
        <img src="/uploads/{image}" alt="{title}" loading="lazy">
    </a>"#,
            item_class = item_class,
            cats_data = cats_data,
            item_url = item_url,
            title = html_escape(title),
            image = image,
            likes = likes,
            tag_data = html_escape(&tag_data),
        ));

        // Tags below image
        if show_tags {
            if let Some(tag_list) = tags {
                if !tag_list.is_empty() {
                    html.push_str(r#"<div class="item-tags">"#);
                    let tag_strs: Vec<String> = tag_list
                        .iter()
                        .filter_map(|t| {
                            let name = t.get("name").and_then(|v| v.as_str())?;
                            let tslug = t.get("slug").and_then(|v| v.as_str())?;
                            Some(format!(
                                "<a href=\"{}\">{}</a>",
                                slug_url(&portfolio_slug, &format!("tag/{}", tslug)),
                                html_escape(name)
                            ))
                        })
                        .collect();
                    html.push_str(&tag_strs.join(" · "));
                    html.push_str("</div>");
                }
            }
        }

        html.push_str("</div>\n");
    }

    html.push_str("</div>");

    // Pagination
    let current_page = context.get("current_page").and_then(|v| v.as_i64()).unwrap_or(1);
    let total_pages = context.get("total_pages").and_then(|v| v.as_i64()).unwrap_or(1);
    let pagination_type = sg("portfolio_pagination_type", "classic");

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
                    current_page + 1, total_pages
                ));
            }
            _ => {
                // Classic pagination
                html.push_str(&build_pagination(current_page, total_pages));
            }
        }
    }

    html
}

fn render_portfolio_single(context: &Value) -> String {
    let item = match context.get("item") {
        Some(i) => i,
        None => return render_404(context),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
    };
    let show_likes = sg("portfolio_enable_likes", "true") == "true";
    let show_cats = sg("portfolio_show_categories", "true") == "true";
    let show_tags = sg("portfolio_show_tags", "true") == "true";
    let portfolio_slug = sg("portfolio_slug", "portfolio");

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let image = item.get("image_path").and_then(|v| v.as_str()).unwrap_or("");
    let desc = item
        .get("description_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);
    let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

    let tags = context.get("tags").and_then(|t| t.as_array());
    let categories = context.get("categories").and_then(|c| c.as_array());

    let like_html = if show_likes {
        format!(
            r#"<span class="like-btn" data-id="{}">♥ <span class="like-count">{}</span></span>"#,
            item_id, format_likes(likes)
        )
    } else {
        String::new()
    };

    let mut html = format!(
        r#"<article class="portfolio-single">
    <div class="portfolio-image">
        <img src="/uploads/{image}" alt="{title}">
    </div>
    <div class="portfolio-meta">
        <h1>{title}</h1>
        {like_html}
    </div>"#,
        image = image,
        title = html_escape(title),
        like_html = like_html,
    );

    if show_cats {
        if let Some(cats) = categories {
            if !cats.is_empty() {
                html.push_str(r#"<div class="portfolio-categories">"#);
                for cat in cats {
                    let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                    html.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        slug_url(&portfolio_slug, &format!("category/{}", slug)),
                        html_escape(name)
                    ));
                }
                html.push_str("</div>");
            }
        }
    }

    if show_tags {
        if let Some(tag_list) = tags {
            if !tag_list.is_empty() {
                html.push_str(r#"<div class="item-tags" style="margin-bottom:12px">"#);
                let tag_strs: Vec<String> = tag_list
                    .iter()
                    .filter_map(|t| {
                        let name = t.get("name").and_then(|v| v.as_str())?;
                        let tslug = t.get("slug").and_then(|v| v.as_str())?;
                        Some(format!(
                            "<a href=\"{}\">{}</a>",
                            slug_url(&portfolio_slug, &format!("tag/{}", tslug)), html_escape(name)
                        ))
                    })
                    .collect();
                html.push_str(&tag_strs.join(" · "));
                html.push_str("</div>");
            }
        }
    }

    if !desc.is_empty() {
        html.push_str(&format!(r#"<div class="portfolio-description">{}</div>"#, desc));
    }

    // Share buttons
    let site_url = sg("site_url", "");
    let item_slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    if !site_url.is_empty() {
        let page_url = format!("{}{}", site_url, slug_url(&portfolio_slug, item_slug));
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // Commerce: Buy / Download section
    let commerce_enabled = context.get("commerce_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if commerce_enabled {
        let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let purchase_note = item.get("purchase_note").and_then(|v| v.as_str()).unwrap_or("");
        let payment_provider = item.get("payment_provider").and_then(|v| v.as_str()).unwrap_or("");

        html.push_str(&build_commerce_html(price, purchase_note, item_id, &settings, payment_provider));
    }

    // Comments on portfolio (gated on comments_enabled flag from route)
    let comments_on = context.get("comments_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if comments_on {
        html.push_str(&build_comments_section(context, &settings, item_id, "portfolio"));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = settings.get("site_name").and_then(|v| v.as_str()).unwrap_or("Velocty");
        let site_url = settings.get("site_url").and_then(|v| v.as_str()).unwrap_or("http://localhost:8000");
        let portfolio_slug = settings.get("portfolio_slug").and_then(|v| v.as_str()).unwrap_or("portfolio");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let meta_desc = item.get("meta_description").and_then(|v| v.as_str()).unwrap_or("");
        html.push_str(&format!(
            r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "ImageObject",
    "name": "{}",
    "description": "{}",
    "contentUrl": "{}/uploads/{}",
    "url": "{}{}",
    "publisher": {{ "@type": "Organization", "name": "{}" }}
}}
</script>"#,
            html_escape(title), html_escape(meta_desc),
            site_url, image,
            site_url, slug_url(portfolio_slug, slug),
            html_escape(site_name),
        ));
    }

    html
}

fn render_blog_list(context: &Value) -> String {
    let posts = match context.get("posts") {
        Some(Value::Array(p)) => p,
        _ => return "<p>No posts yet.</p>".to_string(),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
    };
    let display_type = sg("blog_display_type", "grid");
    let list_style = sg("blog_list_style", "compact");
    let blog_slug = sg("blog_slug", "journal");
    let show_author = sg("blog_show_author", "true") == "true";
    let show_date = sg("blog_show_date", "true") == "true";
    let show_reading_time = sg("blog_show_reading_time", "true") == "true";
    let excerpt_words: usize = sg("blog_excerpt_words", "40").parse().unwrap_or(40);

    // Container class based on display type
    let container_class = match display_type.as_str() {
        "masonry" => "blog-list blog-masonry",
        "grid" => "blog-list blog-grid",
        _ => if list_style == "editorial" { "blog-list blog-editorial" } else { "blog-list" },
    };

    let blog_label = sg("blog_label", "journal");
    let mut html = format!("<div class=\"{}\">\n<h1>{}</h1>", container_class, html_escape(&blog_label));

    for post in posts {
        let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let raw_excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
        let raw_date = post
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let date = format_date(raw_date, &settings);
        let thumb = post
            .get("featured_image")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let author = post.get("author_name").and_then(|v| v.as_str()).unwrap_or("");
        let word_count = post.get("word_count").and_then(|v| v.as_i64()).unwrap_or(0);

        // Truncate excerpt to configured word count
        let excerpt = truncate_words(raw_excerpt, excerpt_words);

        // Reading time estimate (~200 wpm)
        let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;

        let thumb_html = if !thumb.is_empty() {
            format!(
                "<div class=\"blog-thumb\"><img src=\"/uploads/{}\" alt=\"{}\"></div>",
                thumb,
                html_escape(title)
            )
        } else {
            String::new()
        };

        // Build meta line (author, date, reading time)
        let mut meta_parts: Vec<String> = Vec::new();
        if show_author && !author.is_empty() {
            meta_parts.push(format!("<span class=\"blog-author\">{}</span>", html_escape(author)));
        }
        if show_date && !date.is_empty() {
            meta_parts.push(format!("<time>{}</time>", date));
        }
        if show_reading_time && word_count > 0 {
            meta_parts.push(format!("<span class=\"reading-time\">{} min read</span>", reading_time));
        }
        let meta_html = if !meta_parts.is_empty() {
            format!("<div class=\"blog-meta\">{}</div>", meta_parts.join(" · "))
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<article class=\"blog-item\">\
             {thumb_html}\
             <div class=\"blog-body\">\
             <h2><a href=\"/{blog_slug}/{slug}\">{title}</a></h2>\
             {meta_html}\
             <div class=\"blog-excerpt\">{excerpt}</div>\
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
    let current_page = context.get("current_page").and_then(|v| v.as_i64()).unwrap_or(1);
    let total_pages = context.get("total_pages").and_then(|v| v.as_i64()).unwrap_or(1);
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
                    current_page + 1, total_pages
                ));
            }
            _ => {
                html.push_str(&build_pagination(current_page, total_pages));
            }
        }
    }

    html
}

fn render_blog_single(context: &Value) -> String {
    let post = match context.get("post") {
        Some(p) => p,
        None => return render_404(context),
    };
    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or(def).to_string()
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
    let author = post.get("author_name").and_then(|v| v.as_str()).unwrap_or("");
    let word_count = post.get("word_count").and_then(|v| v.as_i64()).unwrap_or(0);
    let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;

    let mut html = format!(
        "<article class=\"blog-single\">\n    <h1>{}</h1>",
        html_escape(title),
    );

    // Build meta line
    let mut meta_parts: Vec<String> = Vec::new();
    if show_author && !author.is_empty() {
        meta_parts.push(format!("<span class=\"blog-author\">{}</span>", html_escape(author)));
    }
    if show_date && !date.is_empty() {
        meta_parts.push(format!("<time>{}</time>", date));
    }
    if show_reading_time && word_count > 0 {
        meta_parts.push(format!("<span class=\"reading-time\">{} min read</span>", reading_time));
    }
    if !meta_parts.is_empty() {
        html.push_str(&format!("\n    <div class=\"blog-meta\">{}</div>", meta_parts.join(" · ")));
    }

    if !featured.is_empty() {
        html.push_str(&format!(
            r#"<div class="featured-image"><img src="/uploads/{}" alt="{}"></div>"#,
            featured,
            html_escape(title)
        ));
    }

    html.push_str(&format!(r#"<div class="post-content">{}</div>"#, content));

    // Tags
    let blog_slug = sg("blog_slug", "journal");
    if let Some(Value::Array(tags)) = context.get("tags") {
        if !tags.is_empty() {
            html.push_str("<div class=\"post-tags\">");
            let tag_strs: Vec<String> = tags.iter().filter_map(|t| {
                let name = t.get("name").and_then(|v| v.as_str())?;
                let tslug = t.get("slug").and_then(|v| v.as_str())?;
                Some(format!("<a href=\"/{}/tag/{}\">{}</a>", blog_slug, tslug, html_escape(name)))
            }).collect();
            html.push_str(&tag_strs.join(" "));
            html.push_str("</div>");
        }
    }

    // Share buttons (Facebook, X, LinkedIn — gated on settings)
    let site_url = sg("site_url", "");
    let post_slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    if !site_url.is_empty() {
        let page_url = format!("{}/{}/{}", site_url, blog_slug, post_slug);
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // Prev / Next post navigation
    let mut nav_html = String::new();
    if let Some(prev) = context.get("prev_post") {
        let prev_title = prev.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let prev_slug = prev.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        nav_html.push_str(&format!("<a href=\"/{}/{}\">&larr; {}</a>", blog_slug, prev_slug, html_escape(prev_title)));
    } else {
        nav_html.push_str("<span></span>");
    }
    if let Some(next) = context.get("next_post") {
        let next_title = next.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let next_slug = next.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        nav_html.push_str(&format!("<a href=\"/{}/{}\">{} &rarr;</a>", blog_slug, next_slug, html_escape(next_title)));
    }
    if !nav_html.is_empty() {
        html.push_str(&format!("<nav class=\"post-nav\">{}</nav>", nav_html));
    }

    // Comments (gated on comments_enabled flag from route)
    let comments_on = context.get("comments_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_comments_section(context, &settings, post_id, "post"));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = settings.get("site_name").and_then(|v| v.as_str()).unwrap_or("Velocty");
        let site_url = settings.get("site_url").and_then(|v| v.as_str()).unwrap_or("http://localhost:8000");
        let blog_slug = settings.get("blog_slug").and_then(|v| v.as_str()).unwrap_or("journal");
        let slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let headline = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let desc = post.get("meta_description").and_then(|v| v.as_str()).unwrap_or("");
        let raw_pub = post.get("published_at").and_then(|v| v.as_str()).unwrap_or("");
        let raw_mod = post.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
        let published = format_date_iso8601(raw_pub, &settings);
        let modified = format_date_iso8601(raw_mod, &settings);
        let image = post.get("featured_image").and_then(|v| v.as_str()).unwrap_or("");
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
            html_escape(headline), html_escape(desc),
            site_url, blog_slug, slug,
            published, modified,
            html_escape(site_name),
        );
        if !image.is_empty() {
            ld.push_str(&format!(r#", "image": "{}/uploads/{}""#, site_url, html_escape(image)));
        }
        ld.push_str("\n}\n</script>");
        html.push_str(&ld);
    }

    html
}

fn render_archives(context: &Value) -> String {
    let archives = match context.get("archives") {
        Some(Value::Array(a)) => a,
        _ => return "<div class=\"blog-list\" style=\"padding:30px\"><h1>Archives</h1><p>No posts yet.</p></div>".to_string(),
    };

    if archives.is_empty() {
        return "<div class=\"blog-list\" style=\"padding:30px\"><h1>Archives</h1><p>No posts yet.</p></div>".to_string();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let blog_slug = settings.get("blog_slug").and_then(|v| v.as_str()).unwrap_or("journal");

    let mut html = String::from("<div class=\"blog-list\" style=\"padding:30px\"><h1>Archives</h1>");

    // Group by year
    let mut current_year = String::new();
    for entry in archives {
        let year = entry.get("year").and_then(|v| v.as_str()).unwrap_or("");
        let month = entry.get("month").and_then(|v| v.as_str()).unwrap_or("");
        let count = entry.get("count").and_then(|v| v.as_i64()).unwrap_or(0);

        if year != current_year {
            if !current_year.is_empty() {
                html.push_str("</ul>");
            }
            current_year = year.to_string();
            html.push_str(&format!("<h2 style=\"margin-top:24px;margin-bottom:8px\">{}</h2><ul style=\"list-style:none;padding:0\">", year));
        }

        // Convert month number to name
        let month_name = match month {
            "01" => "January", "02" => "February", "03" => "March",
            "04" => "April", "05" => "May", "06" => "June",
            "07" => "July", "08" => "August", "09" => "September",
            "10" => "October", "11" => "November", "12" => "December",
            _ => month,
        };

        html.push_str(&format!(
            "<li style=\"padding:4px 0\"><a href=\"/archives/{}/{}\" style=\"color:var(--color-text);text-decoration:none\">{}</a> <span style=\"color:var(--color-text-secondary);font-size:13px\">({})</span></li>",
            year, month, month_name, count
        ));
    }

    if !current_year.is_empty() {
        html.push_str("</ul>");
    }

    html.push_str("</div>");
    html
}

fn render_404(_context: &Value) -> String {
    r#"<div class="error-page">
    <h1>404</h1>
    <p>Page not found.</p>
    <a href="/">← Back to home</a>
</div>"#
        .to_string()
}

fn urlencoding_simple(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Remove any remaining {{placeholder}} tags from rendered HTML.
/// Uses a simple scan instead of regex to avoid adding a dependency.
fn strip_unreplaced_placeholders(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 3 < len && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Look for closing }}
            if let Some(end) = input[i + 2..].find("}}") {
                let tag = &input[i + 2..i + 2 + end];
                // Only strip if it looks like a valid placeholder (lowercase + underscores)
                if !tag.is_empty() && tag.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                    i = i + 2 + end + 2; // skip past }}
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn format_likes(count: i64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

const LIGHTBOX_JS: &str = r#"
(function() {
    const b = document.body.dataset;
    const mode = b.clickMode || 'lightbox';
    if (mode !== 'lightbox') return;

    const showTitle = b.lbShowTitle !== 'false';
    const showTags = b.lbShowTags !== 'false';
    const showNav = b.lbNav !== 'false';
    const useKeyboard = b.lbKeyboard !== 'false';
    const showLikes = b.showLikes !== 'false';

    const links = document.querySelectorAll('.portfolio-link');
    let overlay, img, titleEl, tagsEl, likesEl, closeBtn, prevBtn, nextBtn;
    let currentIndex = 0;
    const items = Array.from(links);

    function createOverlay() {
        overlay = document.createElement('div');
        overlay.className = 'lightbox-overlay';
        overlay.innerHTML =
            '<button class="lb-close">&times;</button>' +
            (showNav ? '<button class="lb-prev">&lsaquo;</button><button class="lb-next">&rsaquo;</button>' : '') +
            '<div class="lb-content">' +
                '<img class="lb-image" src="" alt="">' +
                (showTitle ? '<div class="lb-title"></div>' : '') +
                (showTags ? '<div class="lb-tags"></div>' : '') +
                (showLikes ? '<div class="lb-likes" style="color:#fff;font-size:14px;margin-top:4px"></div>' : '') +
            '</div>';
        document.body.appendChild(overlay);
        img = overlay.querySelector('.lb-image');
        titleEl = overlay.querySelector('.lb-title');
        tagsEl = overlay.querySelector('.lb-tags');
        likesEl = overlay.querySelector('.lb-likes');
        closeBtn = overlay.querySelector('.lb-close');
        prevBtn = overlay.querySelector('.lb-prev');
        nextBtn = overlay.querySelector('.lb-next');

        closeBtn.addEventListener('click', close);
        if (prevBtn) prevBtn.addEventListener('click', function() { navigate(-1); });
        if (nextBtn) nextBtn.addEventListener('click', function() { navigate(1); });
        overlay.addEventListener('click', function(e) { if (e.target === overlay) close(); });
    }

    function open(index) {
        if (!overlay) createOverlay();
        currentIndex = index;
        var link = items[index];
        var imgSrc = link.querySelector('img').src;
        img.src = imgSrc;
        if (titleEl) titleEl.textContent = link.dataset.title || '';
        if (tagsEl) tagsEl.textContent = link.dataset.tags || '';
        if (likesEl) { var lk = link.dataset.likes || '0'; likesEl.innerHTML = '&#9829; ' + lk; }
        overlay.classList.add('active');
        document.body.style.overflow = 'hidden';
    }

    function close() {
        overlay.classList.remove('active');
        document.body.style.overflow = '';
    }

    function navigate(dir) {
        currentIndex = (currentIndex + dir + items.length) % items.length;
        open(currentIndex);
    }

    items.forEach(function(link, i) {
        link.addEventListener('click', function(e) {
            e.preventDefault();
            open(i);
        });
    });

    if (useKeyboard) {
        document.addEventListener('keydown', function(e) {
            if (!overlay || !overlay.classList.contains('active')) return;
            if (e.key === 'Escape') close();
            if (e.key === 'ArrowLeft') navigate(-1);
            if (e.key === 'ArrowRight') navigate(1);
        });
    }

    // Fade-in animation via IntersectionObserver
    if (b.fadeAnimation === 'true') {
        var fadeItems = document.querySelectorAll('.grid-item.fade-in');
        if (fadeItems.length && 'IntersectionObserver' in window) {
            var obs = new IntersectionObserver(function(entries) {
                entries.forEach(function(entry) {
                    if (entry.isIntersecting) {
                        entry.target.classList.add('visible');
                        obs.unobserve(entry.target);
                    }
                });
            }, { threshold: 0.1 });
            fadeItems.forEach(function(el) { obs.observe(el); });
        } else {
            fadeItems.forEach(function(el) { el.classList.add('visible'); });
        }
    }

    // Load More button
    var loadMoreBtn = document.getElementById('load-more-btn');
    if (loadMoreBtn) {
        loadMoreBtn.addEventListener('click', function() {
            var page = parseInt(this.dataset.page);
            var total = parseInt(this.dataset.total);
            if (page > total) return;
            this.textContent = 'Loading...';
            this.disabled = true;
            var btn = this;
            fetch(location.pathname + '?page=' + page, { headers: { 'Accept': 'text/html' } })
                .then(function(r) { return r.text(); })
                .then(function(html) {
                    var tmp = document.createElement('div');
                    tmp.innerHTML = html;
                    var newItems = tmp.querySelectorAll('.grid-item');
                    var grid = document.querySelector('.masonry-grid, .css-grid');
                    newItems.forEach(function(el) { grid.appendChild(el); });
                    if (page + 1 > total) { btn.style.display = 'none'; }
                    else { btn.dataset.page = page + 1; btn.textContent = 'Load More'; btn.disabled = false; }
                })
                .catch(function() { btn.textContent = 'Load More'; btn.disabled = false; });
        });
    }

    // Infinite scroll
    var sentinel = document.getElementById('infinite-sentinel');
    if (sentinel && 'IntersectionObserver' in window) {
        var loading = false;
        var infObs = new IntersectionObserver(function(entries) {
            if (!entries[0].isIntersecting || loading) return;
            var page = parseInt(sentinel.dataset.page);
            var total = parseInt(sentinel.dataset.total);
            if (page > total) { infObs.disconnect(); return; }
            loading = true;
            fetch(location.pathname + '?page=' + page, { headers: { 'Accept': 'text/html' } })
                .then(function(r) { return r.text(); })
                .then(function(html) {
                    var tmp = document.createElement('div');
                    tmp.innerHTML = html;
                    var newItems = tmp.querySelectorAll('.grid-item');
                    var grid = document.querySelector('.masonry-grid, .css-grid');
                    newItems.forEach(function(el) { grid.appendChild(el); });
                    sentinel.dataset.page = page + 1;
                    loading = false;
                    if (page + 1 > total) infObs.disconnect();
                })
                .catch(function() { loading = false; });
        }, { threshold: 0 });
        infObs.observe(sentinel);
    }
})();
"#;

const IMAGE_PROTECTION_JS: &str = r#"<script>
(function(){
    document.addEventListener('contextmenu', function(e) {
        if (e.target.tagName === 'IMG') { e.preventDefault(); }
    });
    document.addEventListener('dragstart', function(e) {
        if (e.target.tagName === 'IMG') { e.preventDefault(); }
    });
    var style = document.createElement('style');
    style.textContent = '.masonry-grid img, .portfolio-image img, .lb-image { -webkit-user-select: none; user-select: none; pointer-events: auto; }';
    document.head.appendChild(style);
})();
</script>"#;

const DEFAULT_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    font-family: var(--font-body);
    font-size: var(--font-size-body);
    color: var(--color-text);
    background: var(--color-bg);
    line-height: 1.6;
    text-transform: var(--text-transform);
}

a { color: var(--color-accent); }

h1, h2, h3, h4, h5, h6 { font-family: var(--font-heading); }
h1 { font-size: var(--font-size-h1); }
h2 { font-size: var(--font-size-h2); }
h3 { font-size: var(--font-size-h3); }
h4 { font-size: var(--font-size-h4); }
h5 { font-size: var(--font-size-h5); }
h6 { font-size: var(--font-size-h6); }

.site-wrapper {
    display: flex;
    min-height: 100vh;
}

/* ── Sidebar ── */
.sidebar {
    width: var(--sidebar-width);
    position: fixed;
    top: 0;
    left: 0;
    height: 100vh;
    padding: 28px 24px;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--color-bg);
    z-index: 10;
}

.sidebar-top { flex: 1; }

.logo-img {
    max-width: 180px;
    max-height: 60px;
    margin-bottom: 6px;
    display: block;
}

.site-name {
    font-size: 22px;
    font-weight: 700;
    margin-bottom: 2px;
    line-height: 1.2;
}

.site-name a { color: var(--color-text); text-decoration: none; }

.site-tagline {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    margin-bottom: 20px;
    line-height: 1.5;
}

/* Collapsible category nav */
.category-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 8px;
    font-family: var(--font-nav);
    font-size: 13px;
}

.nav-category-toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    padding: 3px 0;
    color: var(--color-text);
    font-family: var(--font-nav);
    font-size: 13px;
    background: none;
    border: none;
    text-align: left;
}
.nav-category-toggle:hover { color: var(--color-accent); }
.nav-category-toggle .arrow { font-size: 9px; transition: transform 0.2s; }
.nav-category-toggle.open .arrow { transform: rotate(180deg); }

.nav-subcategories {
    display: none;
    flex-direction: column;
    gap: 1px;
    padding-left: 12px;
}
.nav-subcategories.open { display: flex; }

.cat-link {
    font-family: var(--font-nav);
    font-size: 13px;
    color: var(--color-text);
    text-decoration: none;
    padding: 2px 0;
}

.cat-link:hover { color: var(--color-accent); }
.cat-link.active { font-weight: 700; color: var(--color-accent); }

.nav-link {
    font-family: var(--font-nav);
    font-size: 13px;
    color: var(--color-text);
    text-decoration: none;
    padding: 3px 0;
    display: block;
}
.nav-link:hover { color: var(--color-accent); }
.nav-link.active { color: var(--color-accent); }

.archives-link {
    font-family: var(--font-nav);
    font-size: 13px;
    color: var(--color-text);
    text-decoration: none;
    margin-top: 4px;
    display: inline-block;
}
.archives-link:hover { color: var(--color-accent); }

.sidebar-bottom {
    margin-top: auto;
    padding-top: 16px;
}

.social-links { margin-bottom: 8px; }
.social-links a {
    color: var(--color-text);
    text-decoration: none;
    font-size: 13px;
    margin-right: 8px;
}
.social-links a:hover { color: var(--color-accent); }

.footer-legal {
    font-family: var(--font-captions);
    font-size: 10px;
    margin-top: 8px;
    line-height: 1.5;
}
.footer-legal a {
    color: var(--color-text-secondary);
    text-decoration: none;
}
.footer-legal a:hover { text-decoration: underline; }

.footer-text {
    font-family: var(--font-captions);
    font-size: 10px;
    color: var(--color-text-secondary);
    margin-top: 6px;
    line-height: 1.5;
}

.site-footer {
    margin-left: var(--sidebar-width);
    padding: 24px var(--grid-gap);
    border-top: 1px solid rgba(0,0,0,.08);
    text-align: center;
}
.site-footer .social-links { justify-content: center; display: flex; gap: 12px; flex-wrap: wrap; }
.site-footer .social-links a { margin-right: 0; }

.share-buttons {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin: 20px 0;
    padding: 16px 0;
    border-top: 1px solid rgba(0,0,0,.08);
}
.share-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: var(--font-nav);
    text-decoration: none;
    color: #fff;
    transition: opacity .2s;
}
.share-btn:hover { opacity: .85; }
.share-facebook { background: #1877F2; }
.share-x { background: #000; }
.share-linkedin { background: #0A66C2; }

/* ── Content ── */
.content {
    margin-left: var(--sidebar-width);
    flex: 1;
    padding: 0;
    min-height: 100vh;
}

/* ── Portfolio Masonry Grid ── */
.masonry-grid {
    column-count: var(--grid-columns);
    column-gap: var(--grid-gap);
    padding: var(--grid-gap);
}

.css-grid {
    display: grid;
    grid-template-columns: repeat(var(--grid-columns), 1fr);
    gap: var(--grid-gap);
    padding: var(--grid-gap);
}

.grid-item {
    break-inside: avoid;
    margin-bottom: var(--grid-gap);
}

.css-grid .grid-item { margin-bottom: 0; }

.grid-item img {
    width: 100%;
    display: block;
}

.grid-item .portfolio-link {
    display: block;
    overflow: hidden;
}

.grid-item img:hover {
    opacity: 0.88;
    transition: opacity 0.2s;
}

.grid-item.fade-in {
    opacity: 0;
    transform: translateY(20px);
    transition: opacity 0.5s ease, transform 0.5s ease;
}
.grid-item.fade-in.visible {
    opacity: 1;
    transform: translateY(0);
}

.item-tags {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    padding: 4px 0 8px;
}
.item-tags a { color: var(--color-text-secondary); text-decoration: none; }
.item-tags a:hover { text-decoration: underline; }

/* ── Lightbox ── */
.lightbox-overlay {
    display: none;
    position: fixed;
    top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(0,0,0,0.88);
    z-index: 1000;
    justify-content: center;
    align-items: center;
}

.lightbox-overlay.active { display: flex; }

.lb-content {
    text-align: center;
    max-width: 80vw;
    max-height: 85vh;
}

.lb-image {
    max-width: 80vw;
    max-height: 78vh;
    object-fit: contain;
    border: 8px solid var(--lightbox-border-color);
}

.lb-title {
    color: var(--lightbox-title-color);
    font-family: var(--font-captions);
    font-size: 14px;
    margin-top: 12px;
}

.lb-tags {
    color: var(--lightbox-tag-color);
    font-size: 12px;
    margin-top: 4px;
}

.lb-close {
    position: absolute;
    top: 16px; right: 20px;
    background: none; border: none;
    color: #fff; font-size: 32px;
    cursor: pointer;
    opacity: 0.7;
}
.lb-close:hover { opacity: 1; }

.lb-prev, .lb-next {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: none; border: none;
    color: var(--lightbox-nav-color);
    opacity: 0.5;
    font-size: 48px;
    cursor: pointer;
    padding: 20px;
}
.lb-prev:hover, .lb-next:hover { opacity: 1; }
.lb-prev { left: 8px; }
.lb-next { right: 8px; }

/* ── Journal / Blog List ── */
.blog-list {
    max-width: 900px;
    padding: 28px 30px;
}

.blog-list > h1 {
    font-size: 18px;
    font-weight: 400;
    margin-bottom: 24px;
    letter-spacing: 0.02em;
}

.blog-item {
    display: flex;
    gap: 20px;
    margin-bottom: 28px;
    padding-bottom: 28px;
    border-bottom: 1px solid rgba(0,0,0,0.06);
    align-items: flex-start;
}

.blog-thumb { flex-shrink: 0; }
.blog-thumb img {
    width: 120px;
    height: 120px;
    object-fit: cover;
    display: block;
}

.blog-body { flex: 1; min-width: 0; }

.blog-item h2 {
    font-size: 16px;
    font-weight: 700;
    margin-bottom: 2px;
    line-height: 1.3;
}
.blog-item h2 a { color: var(--color-text); text-decoration: none; }
.blog-item h2 a:hover { text-decoration: underline; }

.blog-meta {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    margin-bottom: 6px;
}
.blog-meta time { font-size: 11px; color: var(--color-text-secondary); }
.reading-time { font-style: italic; }

.blog-item .blog-excerpt {
    font-size: 13px;
    color: var(--color-text);
    line-height: 1.6;
    text-align: justify;
}

/* Blog Grid mode */
.blog-list.blog-grid {
    display: grid;
    grid-template-columns: repeat(var(--blog-grid-columns, 3), 1fr);
    gap: 24px;
    max-width: none;
}
.blog-grid .blog-item {
    flex-direction: column;
    gap: 8px;
    border-bottom: none;
    padding-bottom: 0;
}
.blog-grid .blog-thumb img { width: 100%; height: 200px; }

/* Blog Masonry mode */
.blog-list.blog-masonry {
    column-count: var(--blog-grid-columns, 3);
    column-gap: 24px;
    max-width: none;
}
.blog-masonry .blog-item {
    break-inside: avoid;
    flex-direction: column;
    gap: 8px;
    border-bottom: none;
    padding-bottom: 0;
}
.blog-masonry .blog-thumb img { width: 100%; height: auto; }

/* Blog Editorial list style */
.blog-list.blog-editorial .blog-item {
    flex-direction: column;
    gap: 12px;
    padding-bottom: 32px;
    margin-bottom: 32px;
}
.blog-editorial .blog-thumb img { width: 100%; height: 300px; }
.blog-editorial .blog-item h2 { font-size: 24px; }
.blog-editorial .blog-item .blog-excerpt { font-size: 16px; }

/* ── Journal / Blog Single ── */
.blog-single {
    max-width: 680px;
    padding: 28px 30px;
}

.blog-single h1 {
    font-size: var(--font-size-h1);
    font-weight: 700;
    margin-bottom: 4px;
    line-height: 1.2;
}

.blog-single .blog-meta {
    font-family: var(--font-captions);
    font-size: 12px;
    color: var(--color-text-secondary);
    margin-bottom: 24px;
}
.blog-single time { font-size: 12px; color: var(--color-text-secondary); }

.featured-image img {
    width: 100%;
    margin-bottom: 28px;
    display: block;
}

.post-content {
    line-height: 1.8;
    text-align: justify;
}
.post-content p { margin-bottom: 1em; }
.post-content h2, .post-content h3 { margin-top: 1.5em; margin-bottom: 0.5em; }
.post-content blockquote {
    border-left: 3px solid var(--color-accent);
    padding-left: 16px;
    margin: 1.2em 0;
    font-style: italic;
    color: var(--color-text-secondary);
}
.post-content img { max-width: 100%; height: auto; margin: 1em 0; }

.post-tags {
    margin-top: 28px;
    padding-top: 16px;
    border-top: 1px solid rgba(0,0,0,0.06);
    font-family: var(--font-captions);
    font-size: 12px;
}
.post-tags a {
    color: var(--color-text-secondary);
    text-decoration: none;
    margin-right: 6px;
}
.post-tags a:hover { text-decoration: underline; }
.post-tags a::before { content: '#'; }

.post-share {
    margin-top: 16px;
}
.post-share a {
    display: inline-block;
    padding: 6px 16px;
    background: #000;
    color: #fff;
    font-size: 12px;
    text-decoration: none;
    border-radius: 3px;
}
.post-share a:hover { background: #333; }

.post-nav {
    display: flex;
    justify-content: space-between;
    margin-top: 32px;
    padding-top: 16px;
    border-top: 1px solid rgba(0,0,0,0.06);
    font-size: 13px;
}
.post-nav a { color: var(--color-text); text-decoration: none; }
.post-nav a:hover { color: var(--color-accent); }

/* ── Portfolio Single ── */
.portfolio-single {
    max-width: 1000px;
    padding: 28px 30px;
}

.portfolio-image img { width: 100%; display: block; margin-bottom: 8px; }

.portfolio-images .portfolio-image { margin-bottom: 16px; }

.portfolio-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 16px 0;
}
.portfolio-meta h1 { font-size: var(--font-size-h1); }
.like-btn { cursor: pointer; font-size: 18px; }
.portfolio-categories a {
    font-size: 12px;
    color: var(--color-text-secondary);
    margin-right: 8px;
    text-decoration: none;
}
.portfolio-categories a:hover { text-decoration: underline; }

.portfolio-description {
    line-height: 1.8;
    text-align: justify;
    margin-top: 16px;
}

/* ── Comments ── */
.comments { margin-top: 40px; }
.comments h3 { margin-bottom: 16px; font-size: 16px; }
.comment { margin-bottom: 16px; padding-bottom: 16px; border-bottom: 1px solid rgba(0,0,0,0.06); }
.comment strong { font-size: 14px; }
.comment time { font-size: 11px; color: var(--color-text-secondary); margin-left: 8px; }
.comment p { margin-top: 4px; font-size: 14px; }

.comment-form { margin-top: 30px; }
.comment-form input, .comment-form textarea {
    display: block; width: 100%; max-width: 500px;
    padding: 8px 12px; margin-bottom: 12px;
    border: 1px solid #ddd; border-radius: 3px;
    font-family: inherit; font-size: 14px;
}
.comment-form textarea { min-height: 100px; resize: vertical; }
.comment-form button {
    font-family: var(--font-buttons);
    padding: 8px 24px; background: var(--color-accent);
    color: #fff; border: none; border-radius: 3px;
    cursor: pointer; font-size: 14px;
}

/* ── Pagination ── */
.pagination { display: flex; gap: 8px; padding: 20px 30px; }
.pagination a, .pagination .current {
    font-family: var(--font-buttons);
    padding: 6px 12px; border: 1px solid #ddd; border-radius: 3px;
    text-decoration: none; color: var(--color-text); font-size: 13px;
}
.pagination .current { background: var(--color-accent); color: #fff; border-color: var(--color-accent); }

/* ── Error ── */
.error-page { padding: 60px 30px; text-align: center; }
.error-page h1 { font-size: 72px; color: var(--color-text-secondary); }
.error-page p { margin-top: 12px; color: var(--color-text-secondary); }
.error-page a { color: var(--color-accent); text-decoration: none; }

/* ── Mobile Menu ── */
.mobile-header {
    display: none;
    position: fixed;
    top: 0; left: 0; right: 0;
    background: var(--color-bg);
    padding: 12px 16px;
    z-index: 20;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid rgba(0,0,0,0.06);
}
.mobile-header .logo-img { max-height: 32px; margin: 0; }
.mobile-menu-btn {
    background: none; border: none;
    font-size: 24px; cursor: pointer;
    color: var(--color-text);
}

/* ── Responsive ── */
@media (max-width: 1024px) {
    .sidebar { display: none; }
    .mobile-header { display: flex; }
    .sidebar.mobile-open { display: flex; width: 100%; height: 100vh; z-index: 100; }
    .content { margin-left: 0; padding-top: 56px; }
    .masonry-grid { column-count: 2; }
}

@media (max-width: 768px) {
    .masonry-grid { column-count: 1; }
    .blog-item { flex-direction: column; }
    .blog-thumb img { width: 100%; height: auto; }
    .blog-single { padding: 20px 16px; }
    .portfolio-single { padding: 20px 16px; }
}
"#;

fn build_commerce_html(
    price: f64,
    purchase_note: &str,
    item_id: i64,
    settings: &Value,
    payment_provider: &str,
) -> String {
    let gs = |key: &str| -> &str {
        settings.get(key).and_then(|v| v.as_str()).unwrap_or("")
    };
    let enabled = |key: &str| -> bool {
        gs(key) == "true"
    };

    let currency = {
        let c = gs("commerce_currency");
        if c.is_empty() { "USD" } else { c }
    };

    // Determine which single provider to use for this item
    let provider = if !payment_provider.is_empty() {
        payment_provider.to_string()
    } else {
        // Fallback for legacy items: use first enabled provider
        let providers = [
            ("paypal", "commerce_paypal_enabled", "paypal_client_id"),
            ("stripe", "commerce_stripe_enabled", "stripe_publishable_key"),
            ("razorpay", "commerce_razorpay_enabled", "razorpay_key_id"),
            ("mollie", "commerce_mollie_enabled", "mollie_api_key"),
            ("square", "commerce_square_enabled", "square_access_token"),
            ("2checkout", "commerce_2checkout_enabled", "twocheckout_merchant_code"),
            ("payoneer", "commerce_payoneer_enabled", "payoneer_client_id"),
        ];
        providers.iter()
            .find(|(_, en, key)| enabled(en) && !gs(key).is_empty())
            .map(|(name, _, _)| name.to_string())
            .unwrap_or_default()
    };

    if provider.is_empty() {
        return String::new();
    }

    let btn_style = "display:block;width:100%;padding:12px;border:none;border-radius:8px;font-size:15px;font-weight:600;cursor:pointer;margin-bottom:8px;text-align:center";

    let mut s = String::new();
    s.push_str(r#"<div class="commerce-section" style="margin-top:32px;padding:24px;border-radius:12px;border:1px solid #e0e0e0">"#);

    // Price row
    s.push_str(r#"<div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:16px"><span style="font-size:28px;font-weight:700">"#);
    s.push_str(&html_escape(currency));
    s.push_str(&format!(" {:.2}", price));
    s.push_str(r#"</span><span style="font-size:13px;color:#888">Digital Download</span></div>"#);

    // Purchase note
    if !purchase_note.is_empty() {
        s.push_str(r#"<div style="font-size:13px;color:#888;padding:10px 14px;background:#f9f9f9;border-radius:8px;margin-bottom:4px"><strong>Includes:</strong> "#);
        s.push_str(&html_escape(purchase_note));
        s.push_str("</div>");
    }

    // Buy section
    s.push_str(r#"<div id="commerce-buy" style="margin-top:16px">"#);
    s.push_str(r#"<div id="commerce-email-step">"#);
    s.push_str(r#"<input type="email" id="buyer-email" placeholder="Your email address" style="width:100%;padding:10px 14px;border:1px solid #ddd;border-radius:8px;font-size:14px;margin-bottom:10px">"#);

    // Render only the selected provider's button
    match provider.as_str() {
        "paypal" => {
            s.push_str(r#"<div id="paypal-button-container" style="min-height:45px"></div>"#);
        }
        "stripe" => {
            s.push_str(&format!(
                r#"<button type="button" id="stripe-buy-btn" style="{};background:#635BFF;color:#fff" onclick="commerceRedirect('stripe')">Pay with Stripe</button>"#,
                btn_style
            ));
        }
        "razorpay" => {
            s.push_str(&format!(
                r#"<button type="button" id="razorpay-buy-btn" style="{};background:#072654;color:#fff" onclick="commerceRazorpay()">Pay with Razorpay</button>"#,
                btn_style
            ));
        }
        "mollie" => {
            s.push_str(&format!(
                r#"<button type="button" id="mollie-buy-btn" style="{};background:#0a0a0a;color:#fff" onclick="commerceRedirect('mollie')">Pay with Mollie</button>"#,
                btn_style
            ));
        }
        "square" => {
            s.push_str(&format!(
                r#"<button type="button" id="square-buy-btn" style="{};background:#006AFF;color:#fff" onclick="commerceRedirect('square')">Pay with Square</button>"#,
                btn_style
            ));
        }
        "2checkout" => {
            s.push_str(&format!(
                r#"<button type="button" id="2co-buy-btn" style="{};background:#F36F21;color:#fff" onclick="commerceRedirect('2checkout')">Pay with 2Checkout</button>"#,
                btn_style
            ));
        }
        "payoneer" => {
            s.push_str(&format!(
                r#"<button type="button" id="payoneer-buy-btn" style="{};background:#FF6C00;color:#fff" onclick="commerceRedirect('payoneer')">Pay with Payoneer</button>"#,
                btn_style
            ));
        }
        _ => {}
    }

    s.push_str("</div>"); // end commerce-email-step

    // Processing state
    s.push_str(r#"<div id="commerce-processing" style="display:none;text-align:center;padding:20px"><p style="color:#888">Processing your purchase...</p></div>"#);

    // Success state
    s.push_str(r#"<div id="commerce-success" style="display:none;text-align:center;padding:20px">"#);
    s.push_str(r#"<div style="font-size:32px;margin-bottom:8px">&#10004;</div>"#);
    s.push_str(r#"<h3 style="margin-bottom:8px">Purchase Complete!</h3>"#);
    s.push_str(r#"<p style="font-size:13px;color:#888;margin-bottom:16px">Check your email for the download link.</p>"#);
    s.push_str("<a id=\"commerce-download-link\" href=\"#\" style=\"display:inline-block;padding:10px 24px;background:#E8913A;color:#fff;border-radius:8px;text-decoration:none;font-weight:600\">Download Now</a>");
    s.push_str("<div id=\"commerce-license\" style=\"margin-top:16px;padding:12px;background:#f0fdf4;border-radius:8px;font-size:13px;display:none\"><strong>License Key:</strong> <code id=\"commerce-license-key\" style=\"user-select:all\"></code></div>");
    s.push_str("</div></div>"); // end commerce-success, commerce-buy

    // Already purchased lookup
    s.push_str(r#"<div style="margin-top:12px;border-top:1px solid #eee;padding-top:12px">"#);
    s.push_str(r#"<details style="font-size:12px;color:#888"><summary style="cursor:pointer">Already purchased?</summary>"#);
    s.push_str(r#"<div style="margin-top:8px">"#);
    s.push_str(r#"<input type="email" id="lookup-email" placeholder="Enter your purchase email" style="width:100%;padding:8px 12px;border:1px solid #ddd;border-radius:6px;font-size:13px;margin-bottom:8px">"#);
    s.push_str(r#"<button type="button" onclick="lookupPurchase()" style="padding:6px 16px;border:1px solid #ddd;border-radius:6px;background:#fff;cursor:pointer;font-size:13px">Look Up</button>"#);
    s.push_str(r#"<p id="lookup-result" style="margin-top:8px;font-size:12px;display:none"></p>"#);
    s.push_str("</div></details></div></div>"); // end lookup, commerce-section

    // ── JavaScript ──────────────────────────────────────
    s.push_str("<script>\n");
    s.push_str(&format!("var _vItemId={};\n", item_id));

    // Shared: validate email
    s.push_str("function _vEmail(){var e=document.getElementById('buyer-email').value.trim();if(!e||!e.includes('@')){alert('Please enter a valid email address');return null;}return e;}\n");

    // Shared: show success
    s.push_str("function _vSuccess(d){document.getElementById('commerce-processing').style.display='none';");
    s.push_str("if(!d.ok){alert(d.error||'Payment failed');document.getElementById('commerce-email-step').style.display='';return;}");
    s.push_str("document.getElementById('commerce-success').style.display='';");
    s.push_str("document.getElementById('commerce-download-link').href='/download/'+d.download_token;");
    s.push_str("if(d.license_key){document.getElementById('commerce-license').style.display='';document.getElementById('commerce-license-key').textContent=d.license_key;}}\n");

    // Shared: show processing
    s.push_str("function _vProc(){document.getElementById('commerce-email-step').style.display='none';document.getElementById('commerce-processing').style.display='';}\n");

    // Redirect-based providers (Stripe, Mollie, Square, 2Checkout, Payoneer)
    if matches!(provider.as_str(), "stripe" | "mollie" | "square" | "2checkout" | "payoneer") {
        s.push_str("function commerceRedirect(provider){\n");
        s.push_str("var email=_vEmail();if(!email)return;\n_vProc();\n");
        s.push_str("fetch('/api/checkout/'+provider+'/create',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({portfolio_id:_vItemId,buyer_email:email})})");
        s.push_str(".then(function(r){return r.json()}).then(function(d){\n");
        s.push_str("if(!d.ok){alert(d.error||'Checkout failed');document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';return;}\n");
        s.push_str("if(d.checkout_url){window.location.href=d.checkout_url;}");
        s.push_str("else{_vSuccess(d);}");
        s.push_str("\n}).catch(function(e){alert('Error: '+e.message);document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';});\n}\n");
    }

    // Razorpay JS SDK
    if provider == "razorpay" {
        let rp_key = gs("razorpay_key_id");
        s.push_str("</script>\n<script src=\"https://checkout.razorpay.com/v1/checkout.js\"></script>\n<script>\n");
        s.push_str("function commerceRazorpay(){\n");
        s.push_str("var email=_vEmail();if(!email)return;\n_vProc();\n");
        s.push_str("fetch('/api/checkout/razorpay/create',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({portfolio_id:_vItemId,buyer_email:email})})");
        s.push_str(".then(function(r){return r.json()}).then(function(d){\n");
        s.push_str("if(!d.ok){alert(d.error||'Failed');document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';return;}\n");
        s.push_str("var opts={\n");
        s.push_str(&format!("key:'{}',\n", html_escape(rp_key)));
        s.push_str("amount:d.amount,currency:d.currency,order_id:d.razorpay_order_id,\n");
        s.push_str("prefill:{email:email},\n");
        s.push_str("handler:function(resp){\n");
        s.push_str("fetch('/api/checkout/razorpay/verify',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({");
        s.push_str("order_id:d.order_id,razorpay_order_id:resp.razorpay_order_id,razorpay_payment_id:resp.razorpay_payment_id,razorpay_signature:resp.razorpay_signature,buyer_email:email,buyer_name:''");
        s.push_str("})}).then(function(r){return r.json()}).then(function(v){_vSuccess(v);});\n},\n");
        s.push_str("modal:{ondismiss:function(){document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';}}\n");
        s.push_str("};\nnew Razorpay(opts).open();\n");
        s.push_str("}).catch(function(e){alert('Error: '+e.message);document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';});\n}\n");
    }

    // PayPal JS SDK
    if provider == "paypal" {
        let pp_id = gs("paypal_client_id");
        let pp_cur = {
            let c = gs("paypal_currency");
            if c.is_empty() { currency } else { c }
        };
        s.push_str("</script>\n");
        s.push_str(&format!("<script src=\"https://www.paypal.com/sdk/js?client-id={}&currency={}\"></script>\n<script>\n", html_escape(pp_id), html_escape(pp_cur)));
        s.push_str("paypal.Buttons({\n");
        s.push_str("style:{layout:'vertical',shape:'rect',label:'pay'},\n");
        s.push_str("createOrder:function(){\n");
        s.push_str("var email=_vEmail();if(!email)return;\n");
        s.push_str("return fetch('/api/checkout/paypal/create',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({portfolio_id:_vItemId})}).then(function(r){return r.json()}).then(function(d){if(!d.ok){alert(d.error||'Failed');return;}window._vOid=d.order_id;return d.order_id.toString();});\n");
        s.push_str("},\n");
        s.push_str("onApprove:function(data){\n_vProc();\n");
        s.push_str("var email=document.getElementById('buyer-email').value.trim();\n");
        s.push_str("return fetch('/api/checkout/paypal/capture',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({order_id:window._vOid,paypal_order_id:data.orderID,buyer_email:email,buyer_name:''})}).then(function(r){return r.json()}).then(function(d){_vSuccess(d);});\n},\n");
        s.push_str("onError:function(err){console.error('PayPal error:',err);alert('Payment error. Please try again.');}\n");
        s.push_str("}).render('#paypal-button-container');\n");
    }

    // Lookup function (always included)
    s.push_str("function lookupPurchase(){\n");
    s.push_str("var email=document.getElementById('lookup-email').value.trim();\n");
    s.push_str("var result=document.getElementById('lookup-result');\n");
    s.push_str("if(!email)return;\nresult.style.display='';result.textContent='Looking up...';\n");
    s.push_str(&format!("fetch('/api/checkout/check',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{portfolio_id:{},email:email}})}}).then(function(r){{return r.json()}}).then(function(d){{\n", item_id));
    s.push_str("if(d.purchased&&d.token_valid){result.innerHTML='<a href=\"/download/'+d.download_token+'\" style=\"color:#E8913A;font-weight:600\">Go to Download Page &rarr;</a>';}\n");
    s.push_str("else if(d.purchased){result.textContent='Purchase found but download link has expired.';result.style.color='#f59e0b';}\n");
    s.push_str("else{result.textContent='No purchase found for this email.';result.style.color='#ef4444';}\n");
    s.push_str("}).catch(function(){result.textContent='Error looking up purchase.';});\n}\n");
    s.push_str("</script>\n");

    s
}
