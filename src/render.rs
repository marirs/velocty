use serde_json::Value;

use crate::db::DbPool;
use crate::models::design::Design;
use crate::seo;
use crate::typography;

/// Renders a full page using the active design's shell (layout_html) from the DB.
/// The shell contains {{placeholder}} tags that are replaced with generated content.
pub fn render_page(pool: &DbPool, template_type: &str, context: &Value) -> String {
    let design = Design::active(pool).expect("No active design found");
    render_with_shell(pool, &design, template_type, context)
}

/// Unified renderer: uses the design's layout_html as the page shell,
/// replaces {{placeholder}} tags with generated content from settings and context.
fn render_with_shell(
    _pool: &DbPool,
    design: &Design,
    template_type: &str,
    context: &Value,
) -> String {
    let settings = context.get("settings").cloned().unwrap_or_default();
    let css_vars = typography::build_css_variables(&settings);
    let font_links = typography::build_font_links(&settings);

    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };

    // ── Body content (page-type specific) ──
    let body_html = match template_type {
        "homepage" | "portfolio_grid" => render_portfolio_grid(context),
        "portfolio_single" => render_portfolio_single(context),
        "blog_list" => render_blog_list(context),
        "blog_single" => render_blog_single(context),
        "archives" => render_archives(context),
        "404" => render_404(context),
        _ => render_404(context),
    };

    // ── Site identity ──
    let site_name_raw = settings
        .get("site_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Velocty");
    let site_name_display = settings
        .get("site_name_display")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let site_name = if site_name_display == "none" || site_name_display == "logo" {
        ""
    } else {
        site_name_raw
    };
    let tagline_enabled = settings
        .get("site_tagline_enabled")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        != "false";
    let site_tagline = if tagline_enabled {
        settings
            .get("site_caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    } else {
        ""
    };

    // ── Navigation ──
    let nav_cats_mode = sg("portfolio_nav_categories", "under_link");
    let _header_type_early = sg("layout_header_type", "sidebar");
    let portfolio_slug = sg("portfolio_slug", "portfolio");
    let portfolio_label = sg("portfolio_label", "experiences");
    let portfolio_enabled = sg("portfolio_enabled", "false") == "true";

    // categories_html: inline in nav (under_link for sidebar, submenu for topbar)
    let categories_html =
        if portfolio_enabled && (nav_cats_mode == "under_link" || nav_cats_mode == "submenu") {
            let start_open = nav_cats_mode == "under_link"; // sidebar under_link starts open, topbar submenu starts closed
            build_categories_sidebar(context, start_open)
        } else {
            String::new()
        };

    // categories_page_top: rendered at top of portfolio page content (sidebar only)
    let categories_page_top = if portfolio_enabled && nav_cats_mode == "page_top" {
        let align = sg("portfolio_nav_categories_align", "left");
        build_categories_page_top(context, &portfolio_slug, &align)
    } else {
        String::new()
    };

    // categories_below_menu: horizontal row below topbar nav (topbar only)
    let categories_below_menu = if portfolio_enabled && nav_cats_mode == "below_menu" {
        build_categories_below_menu(context, &portfolio_slug)
    } else {
        String::new()
    };

    // Portfolio nav-link: only show when categories don't replace it
    // under_link and submenu use the category toggle AS the portfolio link → skip nav-link
    // page_top, below_menu, hidden → show portfolio as normal nav-link
    let portfolio_in_nav = portfolio_enabled
        && (nav_cats_mode == "hidden"
            || nav_cats_mode == "page_top"
            || nav_cats_mode == "below_menu");

    let blog_slug = sg("blog_slug", "journal");
    let blog_label = sg("blog_label", "journal");
    let blog_enabled = sg("journal_enabled", "true") != "false";
    let contact_label = sg("contact_label", "catch up");
    let contact_enabled = sg("contact_page_enabled", "false") == "true";

    // ── Journal categories ──
    let journal_cats_mode = sg("journal_nav_categories", "hidden");

    let journal_categories_html = if blog_enabled && journal_cats_mode == "under_link" {
        build_journal_categories_sidebar(context, true)
    } else {
        String::new()
    };

    let journal_categories_page_top = if blog_enabled && journal_cats_mode == "page_top" {
        let align = sg("journal_nav_categories_align", "left");
        build_journal_categories_page_top(context, &blog_slug, &align)
    } else {
        String::new()
    };

    // Journal nav-link: only show when categories don't replace it
    let blog_in_nav =
        blog_enabled && (journal_cats_mode == "hidden" || journal_cats_mode == "page_top");

    // Build nav links in user-defined order (nav_order setting)
    // Portfolio/Journal with categories_html is treated as a single unit
    let nav_order_raw = sg("nav_order", "portfolio,blog,contact");
    let nav_order: Vec<&str> = nav_order_raw.split(',').map(|s| s.trim()).collect();

    let mut nav_links = String::new();
    for key in &nav_order {
        match *key {
            "portfolio" => {
                // categories_html replaces the portfolio nav-link in under_link/submenu modes
                if !categories_html.is_empty() {
                    nav_links.push_str(&categories_html);
                } else if portfolio_in_nav {
                    nav_links.push_str(&format!(
                        "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
                        slug_url(&portfolio_slug, ""),
                        html_escape(&portfolio_label)
                    ));
                }
            }
            "blog" => {
                if !journal_categories_html.is_empty() {
                    nav_links.push_str(&journal_categories_html);
                } else if blog_in_nav {
                    nav_links.push_str(&format!(
                        "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
                        slug_url(&blog_slug, ""),
                        html_escape(&blog_label)
                    ));
                }
            }
            "contact" => {
                if contact_enabled {
                    nav_links.push_str(&format!(
                        "<a href=\"/contact\" class=\"nav-link\">{}</a>\n",
                        html_escape(&contact_label)
                    ));
                }
            }
            _ => {}
        }
    }

    // ── Social & Sharing ──
    let social_pos = sg("social_icons_position", "sidebar");
    let social_full = build_social_links(&settings);
    let social_sidebar = if social_pos == "sidebar" || social_pos == "both" {
        social_full.clone()
    } else {
        String::new()
    };
    let social_footer = if social_pos == "footer" || social_pos == "both" {
        build_social_links_inline(&settings)
    } else {
        String::new()
    };

    let share_pos = sg("share_icons_position", "below_content");
    let site_url = sg("site_url", "");
    let share_sidebar = if share_pos == "sidebar" && !site_url.is_empty() {
        build_share_buttons(&settings, &site_url, site_name_raw)
    } else {
        String::new()
    };

    // ── Footer ──
    let copyright_text = sg("copyright_text", "");
    let copyright_align = sg("copyright_alignment", "center");
    let social_footer_align = sg("footer_alignment", "center");

    let footer_inner = {
        let has_copyright = !copyright_text.is_empty();
        let has_social = !social_footer.is_empty();
        if !has_copyright && !has_social {
            String::new()
        } else {
            let copyright_html = if has_copyright {
                format!("<span class=\"footer-copyright\">{}</span>", copyright_text)
            } else {
                String::new()
            };
            let social_html = if has_social {
                format!("<span class=\"footer-social\">{}</span>", social_footer)
            } else {
                String::new()
            };

            let mut left = String::new();
            let mut center = String::new();
            let mut right = String::new();

            // Place copyright into its slot
            if has_copyright {
                match copyright_align.as_str() {
                    "center" => center.push_str(&copyright_html),
                    "right" => right.push_str(&copyright_html),
                    _ => left.push_str(&copyright_html),
                }
            }
            // Place social into its slot
            if has_social {
                match social_footer_align.as_str() {
                    "center" => center.push_str(&social_html),
                    "right" => right.push_str(&social_html),
                    _ => left.push_str(&social_html),
                }
            }

            format!(
                "<div class=\"footer-cell footer-left\">{}</div><div class=\"footer-cell footer-center\">{}</div><div class=\"footer-cell footer-right\">{}</div>",
                left, center, right
            )
        }
    };

    // ── Layout mode ──
    let header_type = sg("layout_header_type", "sidebar");
    let is_topbar = header_type == "topbar";

    // ── Footer behavior ──
    let footer_behavior = sg("footer_behavior", "regular");
    let footer_active = if footer_behavior != "regular" {
        let scope = sg("footer_behavior_scope", "site_wide");
        if scope == "site_wide" {
            true
        } else {
            let pages = sg("footer_behavior_pages", "");
            pages.split(',').any(|p| p.trim() == template_type)
        }
    } else {
        false
    };

    // ── Layout classes ──
    let body_class = {
        let mut cls = String::new();
        if sg("layout_content_boundary", "full") == "boxed" {
            cls.push_str("boxed-mode");
        }
        if footer_active {
            if !cls.is_empty() {
                cls.push(' ');
            }
            if footer_behavior == "fixed_reveal" {
                cls.push_str("footer-fixed-reveal");
            } else if footer_behavior == "always_visible" {
                cls.push_str("footer-always-visible");
            }
        }
        cls
    };
    let wrapper_classes = {
        let mut cls = String::new();
        if !is_topbar && sg("layout_sidebar_position", "left") == "right" {
            cls.push_str(" sidebar-right");
        }
        if sg("layout_content_boundary", "full") == "boxed" {
            cls.push_str(" layout-boxed");
        }
        if is_topbar {
            let nav_pos = sg("nav_position", "below_logo");
            if nav_pos == "right" {
                cls.push_str(" topbar-nav-right");
            }
        }
        cls
    };

    // ── Data attributes for JS ──
    let click_mode = sg("portfolio_click_mode", "lightbox");
    let show_likes = sg("portfolio_enable_likes", "true");
    let show_cats = sg("portfolio_show_categories", "false");
    let show_tags = sg("portfolio_show_tags", "false");
    let fade_anim = sg("portfolio_fade_animation", "true");
    let display_type = sg("portfolio_display_type", "masonry");
    let pagination_type = sg("portfolio_pagination_type", "classic");
    let lb_title_pos = sg("portfolio_lightbox_show_title", "center");
    let lb_tags_pos = sg("portfolio_lightbox_show_tags", "center");
    let lb_nav = sg("portfolio_lightbox_nav", "true");
    let lb_keyboard = sg("portfolio_lightbox_keyboard", "true");
    let lb_nav_color = sg("portfolio_lightbox_nav_color", "#FFFFFF");
    let lb_buy = sg("commerce_lightbox_buy", "true");
    let lb_buy_pos = sg("commerce_lightbox_buy_position", "bottom_left");
    let commerce_cur = sg("commerce_currency", "USD");
    let hearts_pos = sg("portfolio_like_position", "bottom_right");

    let data_attrs = format!(
        "data-click-mode=\"{click_mode}\" data-show-likes=\"{show_likes}\" data-show-categories=\"{show_cats}\" data-show-tags=\"{show_tags}\" data-fade-animation=\"{fade_anim}\" data-display-type=\"{display_type}\" data-pagination-type=\"{pagination_type}\" data-lb-title-pos=\"{lb_title_pos}\" data-lb-tags-pos=\"{lb_tags_pos}\" data-lb-nav=\"{lb_nav}\" data-lb-keyboard=\"{lb_keyboard}\" data-lb-nav-color=\"{lb_nav_color}\" data-share-position=\"{share_pos}\" data-site-url=\"{site_url}\" data-lb-buy=\"{lb_buy}\" data-lb-buy-position=\"{lb_buy_pos}\" data-commerce-currency=\"{commerce_cur}\" data-hearts-position=\"{hearts_pos}\"",
        click_mode = click_mode, show_likes = show_likes, show_cats = show_cats,
        show_tags = show_tags, fade_anim = fade_anim, display_type = display_type,
        pagination_type = pagination_type, lb_title_pos = lb_title_pos,
        lb_tags_pos = lb_tags_pos, lb_nav = lb_nav, lb_keyboard = lb_keyboard,
        lb_nav_color = lb_nav_color,
        share_pos = sg("share_icons_position", "below_content"),
        site_url = sg("site_url", ""),
        lb_buy = lb_buy, lb_buy_pos = lb_buy_pos, commerce_cur = commerce_cur,
        hearts_pos = hearts_pos,
    );

    let image_protection_js = if sg("portfolio_image_protection", "false") == "true" {
        IMAGE_PROTECTION_JS
    } else {
        ""
    };

    // ── SEO ──
    let seo_meta = context
        .get("seo")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // ── Replace placeholders in the shell ──
    let mut html = if is_topbar {
        ONEGUY_TOPBAR_SHELL_HTML.to_string()
    } else {
        design.layout_html.clone()
    };
    html = html.replace("{{seo_meta}}", &seo_meta);
    html = html.replace("{{webmaster_meta}}", &seo::build_webmaster_meta(&settings));
    html = html.replace("{{favicon_link}}", &build_favicon_link(&settings));
    html = html.replace("{{font_links}}", &font_links);
    html = html.replace("{{css_vars}}", &css_vars);
    html = html.replace("{{base_css}}", BASE_CSS);
    html = html.replace("{{design_css}}", &design.style_css);
    html = html.replace("{{body_class}}", &body_class);
    html = html.replace("{{data_attrs}}", &data_attrs);
    html = html.replace("{{wrapper_classes}}", &wrapper_classes);
    html = html.replace("{{logo_html}}", &build_logo_html(&settings));
    html = html.replace("{{site_name}}", &html_escape(site_name));
    html = html.replace("{{tagline}}", &html_escape(site_tagline));
    html = html.replace("{{categories_below_menu}}", &categories_below_menu);
    html = html.replace("{{nav_links}}", &nav_links);
    html = html.replace("{{share_sidebar}}", &share_sidebar);
    let custom_sidebar = if is_topbar {
        String::new()
    } else {
        build_custom_sidebar_html(&settings)
    };
    html = html.replace("{{custom_sidebar_html}}", &custom_sidebar);
    html = html.replace("{{social_sidebar}}", &social_sidebar);
    let legal_links = if is_topbar {
        String::new()
    } else {
        build_footer_legal_links(&settings)
    };
    html = html.replace("{{footer_legal_links}}", &legal_links);
    let page_type = context
        .get("page_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let body_with_cats = {
        let mut b = String::new();
        if !categories_page_top.is_empty()
            && (page_type == "portfolio_grid" || page_type == "portfolio_single")
        {
            b.push_str(&categories_page_top);
        }
        if !journal_categories_page_top.is_empty()
            && (page_type == "blog_list" || page_type == "blog_single")
        {
            b.push_str(&journal_categories_page_top);
        }
        b.push_str(&body_html);
        b
    };
    html = html.replace("{{body_content}}", &body_with_cats);
    html = html.replace("{{footer_inner}}", &footer_inner);
    html = html.replace("{{back_to_top}}", &build_back_to_top(&settings));
    html = html.replace("{{lightbox_js}}", LIGHTBOX_JS);
    html = html.replace("{{image_protection_js}}", image_protection_js);
    html = html.replace(
        "{{analytics_scripts}}",
        &seo::build_analytics_scripts(&settings),
    );
    html = html.replace(
        "{{cookie_consent}}",
        &build_cookie_consent_banner(&settings),
    );

    html
}

/// Renders a legal page (Privacy Policy, Terms of Use) using the same DB-driven shell.
pub fn render_legal_page(
    pool: &DbPool,
    settings: &std::collections::HashMap<String, String>,
    title: &str,
    html_body: &str,
) -> String {
    let settings_json = serde_json::to_value(settings).unwrap_or_default();
    let categories = crate::models::category::Category::list(pool, Some("portfolio"));
    let cats_json = serde_json::to_value(&categories).unwrap_or_default();
    let seo_html = format!(
        "<title>{} — {}</title>",
        html_escape(title),
        html_escape(
            settings
                .get("site_name")
                .map(|s| s.as_str())
                .unwrap_or("Velocty")
        )
    );
    let body = format!(r#"<div class="legal-content">{}</div>"#, html_body);
    let context = serde_json::json!({
        "settings": settings_json,
        "categories": cats_json,
        "seo": seo_html,
    });
    let design = Design::active(pool).expect("No active design found");

    let result = {
        let settings_v = context.get("settings").cloned().unwrap_or_default();
        let css_vars = typography::build_css_variables(&settings_v);
        let font_links = typography::build_font_links(&settings_v);
        let sg = |key: &str, def: &str| -> String {
            settings_v
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or(def)
                .to_string()
        };

        let mut html = design.layout_html.clone();
        html = html.replace("{{seo_meta}}", &seo_html);
        html = html.replace("{{webmaster_meta}}", "");
        html = html.replace("{{favicon_link}}", &build_favicon_link(&settings_v));
        html = html.replace("{{font_links}}", &font_links);
        html = html.replace("{{css_vars}}", &css_vars);
        html = html.replace("{{base_css}}", BASE_CSS);
        html = html.replace("{{design_css}}", &design.style_css);
        html = html.replace("{{body_class}}", "");
        html = html.replace("{{data_attrs}}", "");
        html = html.replace("{{wrapper_classes}}", "");
        html = html.replace("{{logo_html}}", &build_logo_html(&settings_v));
        let site_name_display = sg("site_name_display", "text");
        let site_name_raw = sg("site_name", "Velocty");
        let site_name = if site_name_display == "none" || site_name_display == "logo" {
            String::new()
        } else {
            site_name_raw
        };
        html = html.replace("{{site_name}}", &html_escape(&site_name));
        let tagline_on = sg("site_tagline_enabled", "true") != "false";
        let tagline = if tagline_on {
            sg("site_caption", "")
        } else {
            String::new()
        };
        html = html.replace("{{tagline}}", &html_escape(&tagline));
        html = html.replace("{{nav_links}}", "");
        html = html.replace("{{share_sidebar}}", "");
        html = html.replace("{{custom_sidebar_html}}", "");
        html = html.replace("{{social_sidebar}}", &build_social_links(&settings_v));
        html = html.replace(
            "{{footer_legal_links}}",
            &build_footer_legal_links(&settings_v),
        );
        html = html.replace("{{body_content}}", &body);
        html = html.replace(
            "{{footer_inner}}",
            &format!(
                "<span class=\"footer-copyright\">&copy; {} {}</span>",
                chrono::Utc::now().format("%Y"),
                html_escape(&sg("site_name", "Velocty"))
            ),
        );
        html = html.replace("{{back_to_top}}", &build_back_to_top(&settings_v));
        html = html.replace("{{lightbox_js}}", "");
        html = html.replace("{{image_protection_js}}", "");
        html = html.replace(
            "{{analytics_scripts}}",
            &seo::build_analytics_scripts(&settings_v),
        );
        html = html.replace(
            "{{cookie_consent}}",
            &build_cookie_consent_banner(&settings_v),
        );
        html
    };
    result
}

/// Reusable comment display + form for blog and portfolio pages.
/// Renders approved comments (threaded) and the submission form with captcha.
fn build_comments_section(
    context: &Value,
    settings: &Value,
    content_id: i64,
    content_type: &str,
) -> String {
    let mut html = String::new();

    // Render existing comments (threaded)
    if let Some(Value::Array(comments)) = context.get("comments") {
        // Separate top-level and replies
        let top: Vec<&Value> = comments
            .iter()
            .filter(|c| c.get("parent_id").and_then(|v| v.as_i64()).is_none())
            .collect();
        let replies: Vec<&Value> = comments
            .iter()
            .filter(|c| c.get("parent_id").and_then(|v| v.as_i64()).is_some())
            .collect();

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
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let require_name = sg("comments_require_name") != "false";
    let require_email = sg("comments_require_email") == "true";
    let name_req = if require_name { " required" } else { "" };
    let email_req = if require_email { " required" } else { "" };

    let (captcha_provider, captcha_site_key): (String, String) =
        if sg("security_recaptcha_enabled") == "true" {
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
                let version = settings
                    .get("security_recaptcha_version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("v3");
                if version == "v3" {
                    captcha_script = format!(
                        r#"<script src="https://www.google.com/recaptcha/api.js?render={}"></script>"#,
                        captcha_site_key
                    );
                    captcha_get_token_js = format!(
                        "function(){{return grecaptcha.execute('{}',{{action:'comment'}})}}",
                        captcha_site_key
                    );
                } else {
                    captcha_script = "https://www.google.com/recaptcha/api.js".to_string();
                    captcha_html = format!(
                        r#"<div class="g-recaptcha" data-sitekey="{}"></div>"#,
                        captcha_site_key
                    );
                    captcha_get_token_js =
                        "function(){return Promise.resolve(grecaptcha.getResponse())}".to_string();
                }
            }
            "turnstile" => {
                captcha_script =
                    "https://challenges.cloudflare.com/turnstile/v0/api.js".to_string();
                captcha_html = format!(
                    r#"<div class="cf-turnstile" data-sitekey="{}"></div>"#,
                    captcha_site_key
                );
                captcha_get_token_js = "function(){return Promise.resolve(document.querySelector('[name=cf-turnstile-response]').value)}".to_string();
            }
            "hcaptcha" => {
                captcha_script = "https://js.hcaptcha.com/1/api.js".to_string();
                captcha_html = format!(
                    r#"<div class="h-captcha" data-sitekey="{}"></div>"#,
                    captcha_site_key
                );
                captcha_get_token_js =
                    "function(){return Promise.resolve(hcaptcha.getResponse())}".to_string();
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
    let name = comment
        .get("author_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Anonymous");
    let body = comment.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let cdate = comment
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let indent = if depth > 0 {
        format!(
            " style=\"margin-left:{}px;border-left:2px solid #e5e7eb;padding-left:12px\"",
            depth.min(3) * 24
        )
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
    let children: Vec<&&Value> = all_replies
        .iter()
        .filter(|r| r.get("parent_id").and_then(|v| v.as_i64()) == Some(id))
        .collect();
    for child in children {
        render_comment(html, child, all_replies, depth + 1);
    }
}

fn build_categories_sidebar(context: &Value, start_open: bool) -> String {
    let categories = match context
        .get("nav_categories")
        .or_else(|| context.get("categories"))
    {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let portfolio_slug = settings
        .get("portfolio_slug")
        .and_then(|v| v.as_str())
        .unwrap_or("portfolio");
    let portfolio_label = settings
        .get("portfolio_label")
        .and_then(|v| v.as_str())
        .unwrap_or("experiences");

    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    // Build collapsible category dropdown
    // Always render the parent toggle — even when all categories are hidden,
    // the portfolio label + "All" link should remain visible.
    let mut html = String::new();

    let show_all = settings
        .get("portfolio_show_all_categories")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        == "true";
    let has_children = !categories.is_empty() || show_all;

    if has_children {
        let open_cls = if start_open { " open" } else { "" };
        html.push_str(&format!(
            "<div class=\"nav-category-group\">\n             <button class=\"nav-category-toggle{}\" onclick=\"this.classList.toggle('open');this.nextElementSibling.classList.toggle('open')\">\
             <span>{}</span> <span class=\"arrow\">&#9662;</span></button>\
             <div class=\"nav-subcategories{}\">",
            open_cls, html_escape(portfolio_label), open_cls
        ));

        if show_all {
            let all_label = settings
                .get("portfolio_all_categories_label")
                .and_then(|v| v.as_str())
                .unwrap_or("All");
            let all_active = if active_slug.is_empty() {
                " active"
            } else {
                ""
            };
            html.push_str(&format!(
                "<a href=\"{}\" class=\"cat-link{}\">{}</a>\n",
                slug_url(portfolio_slug, ""),
                all_active,
                html_escape(all_label)
            ));
        }

        for cat in categories {
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if slug.is_empty() {
                continue;
            }
            let active_class = if slug == active_slug { " active" } else { "" };
            html.push_str(&format!(
                "<a href=\"{}\" class=\"cat-link{}\">{}</a>\n",
                slug_url(portfolio_slug, &format!("category/{}", slug)),
                active_class,
                html_escape(name)
            ));
        }

        html.push_str("</div></div>\n");
    } else {
        // No categories and no "All" link — render portfolio as a plain nav link
        html.push_str(&format!(
            "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
            slug_url(portfolio_slug, ""),
            html_escape(portfolio_label)
        ));
    }

    html
}

/// Page Top mode: horizontal category links at the top of portfolio content area (sidebar layout)
fn build_categories_page_top(context: &Value, portfolio_slug: &str, align: &str) -> String {
    let categories = match context.get("categories") {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };
    if categories.is_empty() {
        return String::new();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let align_cls = if align == "right" { " cats-right" } else { "" };
    let mut html = format!("<div class=\"categories-page-top{}\">", align_cls);

    let show_all = settings
        .get("portfolio_show_all_categories")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        == "true";
    if show_all {
        let all_label = settings
            .get("portfolio_all_categories_label")
            .and_then(|v| v.as_str())
            .unwrap_or("All");
        let all_active = if active_slug.is_empty() {
            " active"
        } else {
            ""
        };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(portfolio_slug, ""),
            all_active,
            html_escape(all_label)
        ));
    }

    for cat in categories {
        let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        if slug.is_empty() {
            continue;
        }
        let active_class = if slug == active_slug { " active" } else { "" };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(portfolio_slug, &format!("category/{}", slug)),
            active_class,
            html_escape(name)
        ));
    }

    html.push_str("</div>");
    html
}

/// Below Main Menu mode: horizontal category links below the topbar navigation (topbar layout)
fn build_categories_below_menu(context: &Value, portfolio_slug: &str) -> String {
    let categories = match context.get("categories") {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };
    if categories.is_empty() {
        return String::new();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let mut html = String::from("<div class=\"categories-below-menu\">");

    let show_all = settings
        .get("portfolio_show_all_categories")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        == "true";
    if show_all {
        let all_label = settings
            .get("portfolio_all_categories_label")
            .and_then(|v| v.as_str())
            .unwrap_or("All");
        let all_active = if active_slug.is_empty() {
            " active"
        } else {
            ""
        };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(portfolio_slug, ""),
            all_active,
            html_escape(all_label)
        ));
    }

    for cat in categories {
        let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        if slug.is_empty() {
            continue;
        }
        let active_class = if slug == active_slug { " active" } else { "" };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(portfolio_slug, &format!("category/{}", slug)),
            active_class,
            html_escape(name)
        ));
    }

    html.push_str("</div>");
    html
}

/// Journal sidebar: collapsible category tree under the journal nav link
fn build_journal_categories_sidebar(context: &Value, start_open: bool) -> String {
    let categories = match context.get("nav_journal_categories") {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let blog_slug = settings
        .get("blog_slug")
        .and_then(|v| v.as_str())
        .unwrap_or("journal");
    let blog_label = settings
        .get("blog_label")
        .and_then(|v| v.as_str())
        .unwrap_or("journal");

    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let mut html = String::new();

    let show_all = settings
        .get("journal_show_all_categories")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        == "true";
    let has_children = !categories.is_empty() || show_all;

    if has_children {
        let open_cls = if start_open { " open" } else { "" };
        html.push_str(&format!(
            "<div class=\"nav-category-group\">\n             <button class=\"nav-category-toggle{}\" onclick=\"this.classList.toggle('open');this.nextElementSibling.classList.toggle('open')\">\
             <span>{}</span> <span class=\"arrow\">&#9662;</span></button>\
             <div class=\"nav-subcategories{}\">",
            open_cls, html_escape(blog_label), open_cls
        ));

        if show_all {
            let all_label = settings
                .get("journal_all_categories_label")
                .and_then(|v| v.as_str())
                .unwrap_or("All");
            let all_active = if active_slug.is_empty() {
                " active"
            } else {
                ""
            };
            html.push_str(&format!(
                "<a href=\"{}\" class=\"cat-link{}\">{}</a>\n",
                slug_url(blog_slug, ""),
                all_active,
                html_escape(all_label)
            ));
        }

        for cat in categories {
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if slug.is_empty() {
                continue;
            }
            let active_class = if slug == active_slug { " active" } else { "" };
            html.push_str(&format!(
                "<a href=\"{}\" class=\"cat-link{}\">{}</a>\n",
                slug_url(blog_slug, &format!("category/{}", slug)),
                active_class,
                html_escape(name)
            ));
        }

        html.push_str("</div></div>\n");
    } else {
        html.push_str(&format!(
            "<a href=\"{}\" class=\"nav-link\">{}</a>\n",
            slug_url(blog_slug, ""),
            html_escape(blog_label)
        ));
    }

    html
}

/// Journal Page Top mode: horizontal category links at the top of blog content area
fn build_journal_categories_page_top(context: &Value, blog_slug: &str, align: &str) -> String {
    let categories = match context.get("nav_journal_categories") {
        Some(Value::Array(cats)) => cats,
        _ => return String::new(),
    };
    if categories.is_empty() {
        return String::new();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let align_cls = if align == "right" { " cats-right" } else { "" };
    let mut html = format!("<div class=\"categories-page-top{}\">", align_cls);

    let show_all = settings
        .get("journal_show_all_categories")
        .and_then(|v| v.as_str())
        .unwrap_or("true")
        == "true";
    if show_all {
        let all_label = settings
            .get("journal_all_categories_label")
            .and_then(|v| v.as_str())
            .unwrap_or("All");
        let all_active = if active_slug.is_empty() {
            " active"
        } else {
            ""
        };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(blog_slug, ""),
            all_active,
            html_escape(all_label)
        ));
    }

    for cat in categories {
        let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        if slug.is_empty() {
            continue;
        }
        let active_class = if slug == active_slug { " active" } else { "" };
        html.push_str(&format!(
            "<a href=\"{}\" class=\"cat-link{}\">{}</a>",
            slug_url(blog_slug, &format!("category/{}", slug)),
            active_class,
            html_escape(name)
        ));
    }

    html.push_str("</div>");
    html
}

fn build_social_links(settings: &Value) -> String {
    let sg = |key: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let brand_colors = sg("social_brand_colors") == "true";

    // (setting_key, platform_label, icon_svg, brand_color)
    // All icons use fill="currentColor" so color is controlled by the style attribute or CSS
    let platforms: &[(&str, &str, &str, &str)] = &[
        (
            "social_instagram",
            "Instagram",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2.163c3.204 0 3.584.012 4.85.07 3.252.148 4.771 1.691 4.919 4.919.058 1.265.069 1.645.069 4.849 0 3.205-.012 3.584-.069 4.849-.149 3.225-1.664 4.771-4.919 4.919-1.266.058-1.644.07-4.85.07-3.204 0-3.584-.012-4.849-.07-3.26-.149-4.771-1.699-4.919-4.92-.058-1.265-.07-1.644-.07-4.849 0-3.204.013-3.583.07-4.849.149-3.227 1.664-4.771 4.919-4.919 1.266-.057 1.645-.069 4.849-.069zM12 0C8.741 0 8.333.014 7.053.072 2.695.272.273 2.69.073 7.052.014 8.333 0 8.741 0 12c0 3.259.014 3.668.072 4.948.2 4.358 2.618 6.78 6.98 6.98C8.333 23.986 8.741 24 12 24c3.259 0 3.668-.014 4.948-.072 4.354-.2 6.782-2.618 6.979-6.98.059-1.28.073-1.689.073-4.948 0-3.259-.014-3.667-.072-4.947-.196-4.354-2.617-6.78-6.979-6.98C15.668.014 15.259 0 12 0zm0 5.838a6.162 6.162 0 100 12.324 6.162 6.162 0 000-12.324zM12 16a4 4 0 110-8 4 4 0 010 8zm6.406-11.845a1.44 1.44 0 100 2.881 1.44 1.44 0 000-2.881z"/></svg>"#,
            "#E4405F",
        ),
        (
            "social_twitter",
            "X",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>"#,
            "#000",
        ),
        (
            "social_facebook",
            "Facebook",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z"/></svg>"#,
            "#1877F2",
        ),
        (
            "social_youtube",
            "YouTube",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M23.498 6.186a3.016 3.016 0 00-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 00.502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 002.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 002.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"/></svg>"#,
            "#FF0000",
        ),
        (
            "social_tiktok",
            "TikTok",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12.525.02c1.31-.02 2.61-.01 3.91-.02.08 1.53.63 3.09 1.75 4.17 1.12 1.11 2.7 1.62 4.24 1.79v4.03c-1.44-.05-2.89-.35-4.2-.97-.57-.26-1.1-.59-1.62-.93-.01 2.92.01 5.84-.02 8.75-.08 1.4-.54 2.79-1.35 3.94-1.31 1.92-3.58 3.17-5.91 3.21-1.43.08-2.86-.31-4.08-1.03-2.02-1.19-3.44-3.37-3.65-5.71-.02-.5-.03-1-.01-1.49.18-1.9 1.12-3.72 2.58-4.96 1.66-1.44 3.98-2.13 6.15-1.72.02 1.48-.04 2.96-.04 4.44-.99-.32-2.15-.23-3.02.37-.63.41-1.11 1.04-1.36 1.75-.21.51-.15 1.07-.14 1.61.24 1.64 1.82 3.02 3.5 2.87 1.12-.01 2.19-.66 2.77-1.61.19-.33.4-.67.41-1.06.1-1.79.06-3.57.07-5.36.01-4.03-.01-8.05.02-12.07z"/></svg>"#,
            "#ff0050",
        ),
        (
            "social_linkedin",
            "LinkedIn",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.064 2.064 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg>"#,
            "#0A66C2",
        ),
        (
            "social_pinterest",
            "Pinterest",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12.017 0C5.396 0 .029 5.367.029 11.987c0 5.079 3.158 9.417 7.618 11.162-.105-.949-.199-2.403.041-3.439.219-.937 1.406-5.957 1.406-5.957s-.359-.72-.359-1.781c0-1.668.967-2.914 2.171-2.914 1.023 0 1.518.769 1.518 1.69 0 1.029-.655 2.568-.994 3.995-.283 1.194.599 2.169 1.777 2.169 2.133 0 3.772-2.249 3.772-5.495 0-2.873-2.064-4.882-5.012-4.882-3.414 0-5.418 2.561-5.418 5.207 0 1.031.397 2.138.893 2.738a.36.36 0 01.083.345l-.333 1.36c-.053.22-.174.267-.402.161-1.499-.698-2.436-2.889-2.436-4.649 0-3.785 2.75-7.262 7.929-7.262 4.163 0 7.398 2.967 7.398 6.931 0 4.136-2.607 7.464-6.227 7.464-1.216 0-2.359-.631-2.75-1.378l-.748 2.853c-.271 1.043-1.002 2.35-1.492 3.146C9.57 23.812 10.763 24 12.017 24c6.624 0 11.99-5.367 11.99-11.988C24.007 5.367 18.641 0 12.017 0z"/></svg>"#,
            "#BD081C",
        ),
        (
            "social_behance",
            "Behance",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M22 7h-7V5h7v2zm1.726 10c-.442 1.297-2.029 3-5.101 3-3.074 0-5.564-1.729-5.564-5.675 0-3.91 2.325-5.92 5.466-5.92 3.082 0 4.964 1.782 5.375 4.426.078.506.109 1.188.095 2.14H15.97c.13 3.211 3.483 3.312 4.588 2.029h3.168zM15.61 13.28c-.078-1.229-.996-2.28-2.34-2.28-1.258 0-2.205.906-2.405 2.28h4.745zM6.832 17.36c0-1.665-1.133-2.34-2.76-2.34H1.5v4.68h2.572c1.627 0 2.76-.675 2.76-2.34zM6.435 12c0-1.44-.96-2.16-2.394-2.16H1.5v4.32h2.541c1.434 0 2.394-.72 2.394-2.16zM0 8h4.5c2.58 0 4.32 1.26 4.32 3.6 0 1.44-.72 2.52-1.98 3.12 1.62.48 2.58 1.8 2.58 3.48 0 2.52-1.98 3.8-4.68 3.8H0V8z"/></svg>"#,
            "#1769FF",
        ),
        (
            "social_dribbble",
            "Dribbble",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 24C5.385 24 0 18.615 0 12S5.385 0 12 0s12 5.385 12 12-5.385 12-12 12zm10.12-10.358c-.35-.11-3.17-.953-6.384-.438 1.34 3.684 1.887 6.684 1.992 7.308a10.174 10.174 0 004.392-6.87zm-6.115 7.808c-.153-.9-.75-4.032-2.19-7.77l-.066.02c-5.79 2.015-7.86 6.025-8.04 6.4a10.15 10.15 0 006.29 2.166c1.42 0 2.77-.29 4.006-.816zm-11.62-2.58c.232-.4 3.045-5.055 8.332-6.765.135-.045.27-.084.405-.12-.26-.585-.54-1.167-.832-1.74C7.17 11.775 2.206 11.71 1.756 11.7l-.004.312c0 2.633.998 5.037 2.634 6.855zm-2.42-8.955c.46.008 4.683.026 9.477-1.248-1.698-3.018-3.53-5.558-3.8-5.928-2.868 1.35-5.01 3.99-5.676 7.17zM9.6 2.052c.282.38 2.145 2.914 3.822 6 3.645-1.365 5.19-3.44 5.373-3.702A10.13 10.13 0 0012 1.8c-.82 0-1.62.09-2.4.252zm10.14 3.2c-.21.29-1.9 2.49-5.69 4.02.24.49.47.985.68 1.485.075.18.15.36.22.53 3.41-.43 6.8.26 7.14.33-.02-2.42-.88-4.64-2.35-6.37z"/></svg>"#,
            "#EA4C89",
        ),
        (
            "social_github",
            "GitHub",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>"#,
            "#333",
        ),
        (
            "social_vimeo",
            "Vimeo",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M23.977 6.416c-.105 2.338-1.739 5.543-4.894 9.609-3.268 4.247-6.026 6.37-8.29 6.37-1.409 0-2.578-1.294-3.553-3.881L5.322 11.4C4.603 8.816 3.834 7.522 3.01 7.522c-.179 0-.806.378-1.881 1.132L0 7.197a315.065 315.065 0 003.501-3.123C5.08 2.701 6.266 1.984 7.055 1.91c1.867-.18 3.016 1.1 3.447 3.838.465 2.953.789 4.789.971 5.507.539 2.45 1.131 3.674 1.776 3.674.502 0 1.256-.796 2.263-2.385 1.004-1.589 1.54-2.797 1.612-3.628.144-1.371-.395-2.061-1.614-2.061-.574 0-1.167.121-1.777.391 1.186-3.868 3.434-5.757 6.762-5.637 2.473.06 3.628 1.664 3.493 4.797l-.011.01z"/></svg>"#,
            "#1AB7EA",
        ),
        (
            "social_500px",
            "500px",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M7.439 9.01A2.994 2.994 0 004.449 12a2.994 2.994 0 002.99 2.99 2.994 2.994 0 002.99-2.99 2.994 2.994 0 00-2.99-2.99zm0 4.48A1.494 1.494 0 015.949 12c0-.825.665-1.49 1.49-1.49s1.49.665 1.49 1.49-.665 1.49-1.49 1.49zm9.122-4.48A2.994 2.994 0 0013.571 12a2.994 2.994 0 002.99 2.99 2.994 2.994 0 002.99-2.99 2.994 2.994 0 00-2.99-2.99zm0 4.48A1.494 1.494 0 0115.071 12c0-.825.665-1.49 1.49-1.49s1.49.665 1.49 1.49-.665 1.49-1.49 1.49zM12 2C6.478 2 2 6.478 2 12s4.478 10 10 10 10-4.478 10-10S17.522 2 12 2zm0 18.5c-4.687 0-8.5-3.813-8.5-8.5S7.313 3.5 12 3.5s8.5 3.813 8.5 8.5-3.813 8.5-8.5 8.5z"/></svg>"#,
            "#0099E5",
        ),
    ];

    let collected: Vec<(String, String, &str, &str)> = platforms
        .iter()
        .filter_map(|&(key, label, icon, color)| {
            let url = sg(key);
            if url.is_empty() {
                None
            } else {
                Some((label.to_string(), url, icon, color))
            }
        })
        .collect();

    if collected.is_empty() {
        return String::new();
    }

    let mut html = String::from("<div class=\"social-links\">");
    for (label, url, icon, color) in &collected {
        let style = if brand_colors {
            format!(" style=\"color:{}\"", color)
        } else {
            String::new()
        };
        html.push_str(&format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\" title=\"{}\"{}>{}</a>",
            html_escape(url),
            html_escape(label),
            style,
            icon
        ));
    }
    html.push_str("</div>");
    html
}

/// Build social links as bare <a> tags (no wrapper div) for footer flex layout.
fn build_social_links_inline(settings: &Value) -> String {
    let sg = |key: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let brand_colors = sg("social_brand_colors") == "true";

    let platforms: &[(&str, &str, &str, &str)] = &[
        (
            "social_instagram",
            "Instagram",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2.163c3.204 0 3.584.012 4.85.07 3.252.148 4.771 1.691 4.919 4.919.058 1.265.069 1.645.069 4.849 0 3.205-.012 3.584-.069 4.849-.149 3.225-1.664 4.771-4.919 4.919-1.266.058-1.644.07-4.85.07-3.204 0-3.584-.012-4.849-.07-3.26-.149-4.771-1.699-4.919-4.92-.058-1.265-.07-1.644-.07-4.849 0-3.204.013-3.583.07-4.849.149-3.227 1.664-4.771 4.919-4.919 1.266-.057 1.645-.069 4.849-.069zM12 0C8.741 0 8.333.014 7.053.072 2.695.272.273 2.69.073 7.052.014 8.333 0 8.741 0 12c0 3.259.014 3.668.072 4.948.2 4.358 2.618 6.78 6.98 6.98C8.333 23.986 8.741 24 12 24c3.259 0 3.668-.014 4.948-.072 4.354-.2 6.782-2.618 6.979-6.98.059-1.28.073-1.689.073-4.948 0-3.259-.014-3.667-.072-4.947-.196-4.354-2.617-6.78-6.979-6.98C15.668.014 15.259 0 12 0zm0 5.838a6.162 6.162 0 100 12.324 6.162 6.162 0 000-12.324zM12 16a4 4 0 110-8 4 4 0 010 8zm6.406-11.845a1.44 1.44 0 100 2.881 1.44 1.44 0 000-2.881z"/></svg>"#,
            "#E4405F",
        ),
        (
            "social_twitter",
            "X",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>"#,
            "#000",
        ),
        (
            "social_facebook",
            "Facebook",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z"/></svg>"#,
            "#1877F2",
        ),
        (
            "social_youtube",
            "YouTube",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M23.498 6.186a3.016 3.016 0 00-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 00.502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 002.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 002.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"/></svg>"#,
            "#FF0000",
        ),
        (
            "social_tiktok",
            "TikTok",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12.525.02c1.31-.02 2.61-.01 3.91-.02.08 1.53.63 3.09 1.75 4.17 1.12 1.11 2.7 1.62 4.24 1.79v4.03c-1.44-.05-2.89-.35-4.2-.97-.57-.26-1.1-.59-1.62-.93-.01 2.92.01 5.84-.02 8.75-.08 1.4-.54 2.79-1.35 3.94-1.31 1.92-3.58 3.17-5.91 3.21-1.43.08-2.86-.31-4.08-1.03-2.02-1.19-3.44-3.37-3.65-5.71-.02-.5-.03-1-.01-1.49.18-1.9 1.12-3.72 2.58-4.96 1.66-1.44 3.98-2.13 6.15-1.72.02 1.48-.04 2.96-.04 4.44-.99-.32-2.15-.23-3.02.37-.63.41-1.11 1.04-1.36 1.75-.21.51-.15 1.07-.14 1.61.24 1.64 1.82 3.02 3.5 2.87 1.12-.01 2.19-.66 2.77-1.61.19-.33.4-.67.41-1.06.1-1.79.06-3.57.07-5.36.01-4.03-.01-8.05.02-12.07z"/></svg>"#,
            "#ff0050",
        ),
        (
            "social_linkedin",
            "LinkedIn",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.064 2.064 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg>"#,
            "#0A66C2",
        ),
        (
            "social_pinterest",
            "Pinterest",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12.017 0C5.396 0 .029 5.367.029 11.987c0 5.079 3.158 9.417 7.618 11.162-.105-.949-.199-2.403.041-3.439.219-.937 1.406-5.957 1.406-5.957s-.359-.72-.359-1.781c0-1.668.967-2.914 2.171-2.914 1.023 0 1.518.769 1.518 1.69 0 1.029-.655 2.568-.994 3.995-.283 1.194.599 2.169 1.777 2.169 2.133 0 3.772-2.249 3.772-5.495 0-2.873-2.064-4.882-5.012-4.882-3.414 0-5.418 2.561-5.418 5.207 0 1.031.397 2.138.893 2.738a.36.36 0 01.083.345l-.333 1.36c-.053.22-.174.267-.402.161-1.499-.698-2.436-2.889-2.436-4.649 0-3.785 2.75-7.262 7.929-7.262 4.163 0 7.398 2.967 7.398 6.931 0 4.136-2.607 7.464-6.227 7.464-1.216 0-2.359-.631-2.75-1.378l-.748 2.853c-.271 1.043-1.002 2.35-1.492 3.146C9.57 23.812 10.763 24 12.017 24c6.624 0 11.99-5.367 11.99-11.988C24.007 5.367 18.641 0 12.017 0z"/></svg>"#,
            "#BD081C",
        ),
        (
            "social_behance",
            "Behance",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M22 7h-7V5h7v2zm1.726 10c-.442 1.297-2.029 3-5.101 3-3.074 0-5.564-1.729-5.564-5.675 0-3.91 2.325-5.92 5.466-5.92 3.082 0 4.964 1.782 5.375 4.426.078.506.109 1.188.095 2.14H15.97c.13 3.211 3.483 3.312 4.588 2.029h3.168zM15.61 13.28c-.078-1.229-.996-2.28-2.34-2.28-1.258 0-2.205.906-2.405 2.28h4.745zM6.832 17.36c0-1.665-1.133-2.34-2.76-2.34H1.5v4.68h2.572c1.627 0 2.76-.675 2.76-2.34zM6.435 12c0-1.44-.96-2.16-2.394-2.16H1.5v4.32h2.541c1.434 0 2.394-.72 2.394-2.16zM0 8h4.5c2.58 0 4.32 1.26 4.32 3.6 0 1.44-.72 2.52-1.98 3.12 1.62.48 2.58 1.8 2.58 3.48 0 2.52-1.98 3.8-4.68 3.8H0V8z"/></svg>"#,
            "#1769FF",
        ),
        (
            "social_dribbble",
            "Dribbble",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 24C5.385 24 0 18.615 0 12S5.385 0 12 0s12 5.385 12 12-5.385 12-12 12zm10.12-10.358c-.35-.11-3.17-.953-6.384-.438 1.34 3.684 1.887 6.684 1.992 7.308a10.174 10.174 0 004.392-6.87zm-6.115 7.808c-.153-.9-.75-4.032-2.19-7.77l-.066.02c-5.79 2.015-7.86 6.025-8.04 6.4a10.15 10.15 0 006.29 2.166c1.42 0 2.77-.29 4.006-.816zm-11.62-2.58c.232-.4 3.045-5.055 8.332-6.765.135-.045.27-.084.405-.12-.26-.585-.54-1.167-.832-1.74C7.17 11.775 2.206 11.71 1.756 11.7l-.004.312c0 2.633.998 5.037 2.634 6.855zm-2.42-8.955c.46.008 4.683.026 9.477-1.248-1.698-3.018-3.53-5.558-3.8-5.928-2.868 1.35-5.01 3.99-5.676 7.17zM9.6 2.052c.282.38 2.145 2.914 3.822 6 3.645-1.365 5.19-3.44 5.373-3.702A10.13 10.13 0 0012 1.8c-.82 0-1.62.09-2.4.252zm10.14 3.2c-.21.29-1.9 2.49-5.69 4.02.24.49.47.985.68 1.485.075.18.15.36.22.53 3.41-.43 6.8.26 7.14.33-.02-2.42-.88-4.64-2.35-6.37z"/></svg>"#,
            "#EA4C89",
        ),
        (
            "social_github",
            "GitHub",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>"#,
            "#333",
        ),
        (
            "social_vimeo",
            "Vimeo",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M23.977 6.416c-.105 2.338-1.739 5.543-4.894 9.609-3.268 4.247-6.026 6.37-8.29 6.37-1.409 0-2.578-1.294-3.553-3.881L5.322 11.4C4.603 8.816 3.834 7.522 3.01 7.522c-.179 0-.806.378-1.881 1.132L0 7.197a315.065 315.065 0 003.501-3.123C5.08 2.701 6.266 1.984 7.055 1.91c1.867-.18 3.016 1.1 3.447 3.838.465 2.953.789 4.789.971 5.507.539 2.45 1.131 3.674 1.776 3.674.502 0 1.256-.796 2.263-2.385 1.004-1.589 1.54-2.797 1.612-3.628.144-1.371-.395-2.061-1.614-2.061-.574 0-1.167.121-1.777.391 1.186-3.868 3.434-5.757 6.762-5.637 2.473.06 3.628 1.664 3.493 4.797l-.011.01z"/></svg>"#,
            "#1AB7EA",
        ),
        (
            "social_500px",
            "500px",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M7.439 9.01A2.994 2.994 0 004.449 12a2.994 2.994 0 002.99 2.99 2.994 2.994 0 002.99-2.99 2.994 2.994 0 00-2.99-2.99zm0 4.48A1.494 1.494 0 015.949 12c0-.825.665-1.49 1.49-1.49s1.49.665 1.49 1.49-.665 1.49-1.49 1.49zm9.122-4.48A2.994 2.994 0 0013.571 12a2.994 2.994 0 002.99 2.99 2.994 2.994 0 002.99-2.99 2.994 2.994 0 00-2.99-2.99zm0 4.48A1.494 1.494 0 0115.071 12c0-.825.665-1.49 1.49-1.49s1.49.665 1.49 1.49-.665 1.49-1.49 1.49zM12 2C6.478 2 2 6.478 2 12s4.478 10 10 10 10-4.478 10-10S17.522 2 12 2zm0 18.5c-4.687 0-8.5-3.813-8.5-8.5S7.313 3.5 12 3.5s8.5 3.813 8.5 8.5-3.813 8.5-8.5 8.5z"/></svg>"#,
            "#0099E5",
        ),
    ];

    let collected: Vec<(String, String, &str, &str)> = platforms
        .iter()
        .filter_map(|&(key, label, icon, color)| {
            let url = sg(key);
            if url.is_empty() {
                None
            } else {
                Some((label.to_string(), url, icon, color))
            }
        })
        .collect();

    if collected.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    for (label, url, icon, color) in &collected {
        let style = if brand_colors {
            format!(" style=\"color:{}\"", color)
        } else {
            String::new()
        };
        html.push_str(&format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\" class=\"social-icon-footer\" title=\"{}\"{}>{}</a>",
            html_escape(url), html_escape(label), style, icon
        ));
    }
    html
}

/// Build share icons for sharing pages/site.
/// Renders icon-only links (no text). Respects social_brand_colors setting.
fn build_share_buttons(settings: &Value, page_url: &str, page_title: &str) -> String {
    let sg = |key: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    if sg("share_enabled") != "true" {
        return String::new();
    }

    let brand_colors = sg("social_brand_colors") == "true";
    let encoded_url = urlencoding_simple(page_url);
    let encoded_title = urlencoding_simple(page_title);

    // (setting_key, label, share_url_template, icon_svg, brand_color)
    // All icons use fill="currentColor" — color controlled by style attr or CSS
    let platforms: &[(&str, &str, &str, &str, &str)] = &[
        (
            "share_facebook",
            "Share on Facebook",
            "https://www.facebook.com/sharer/sharer.php?u={url}",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z"/></svg>"#,
            "#1877F2",
        ),
        (
            "share_x",
            "Share on X",
            "https://x.com/intent/tweet?url={url}&text={title}",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>"#,
            "#000",
        ),
        (
            "share_linkedin",
            "Share on LinkedIn",
            "https://www.linkedin.com/sharing/share-offsite/?url={url}",
            r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.064 2.064 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg>"#,
            "#0A66C2",
        ),
    ];

    let mut icons = Vec::new();
    for &(key, label, url_tpl, icon, color) in platforms {
        if sg(key) != "true" {
            continue;
        }
        let href = url_tpl
            .replace("{url}", &encoded_url)
            .replace("{title}", &encoded_title);
        let style = if brand_colors {
            format!(" style=\"color:{}\"", color)
        } else {
            String::new()
        };
        icons.push(format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\" class=\"share-icon\" title=\"{}\"{}>{}</a>",
            href, label, style, icon
        ));
    }

    if icons.is_empty() {
        return String::new();
    }

    let share_label = sg("share_label");
    let label_html = if !share_label.is_empty() {
        format!(
            "<span class=\"share-label\">{}</span>",
            html_escape(&share_label)
        )
    } else {
        String::new()
    };
    format!(
        "<div class=\"share-icons\">{}{}</div>",
        label_html,
        icons.join("\n")
    )
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
    let display = settings
        .get("site_name_display")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let logo = settings
        .get("site_logo")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // In "none" mode, no logo image shown
    if display == "none" {
        return String::new();
    }

    // Only show logo image if one is uploaded
    if logo.is_empty() {
        return String::new();
    }

    let site_name = settings
        .get("site_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Velocty");
    let logo_width = settings
        .get("site_logo_width")
        .and_then(|v| v.as_str())
        .unwrap_or("180px");
    let width_style = if logo_width != "180px" && !logo_width.is_empty() {
        format!(" style=\"max-width:{}\"", html_escape(logo_width))
    } else {
        String::new()
    };
    format!(
        "<a href=\"/\"><img src=\"{}\" alt=\"{}\" class=\"logo-img\"{}></a>",
        html_escape(logo),
        html_escape(site_name),
        width_style
    )
}

fn build_custom_sidebar_html(settings: &Value) -> String {
    let heading = settings
        .get("layout_sidebar_custom_heading")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let text = settings
        .get("layout_sidebar_custom_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if heading.is_empty() && text.is_empty() {
        return String::new();
    }
    let mut html = String::from("<div class=\"sidebar-custom\">");
    if !heading.is_empty() {
        html.push_str(&format!(
            "<h3 class=\"sidebar-custom-heading\">{}</h3>",
            html_escape(heading)
        ));
    }
    if !text.is_empty() {
        html.push_str(&format!(
            "<div class=\"sidebar-custom-text\">{}</div>",
            text
        ));
    }
    html.push_str("</div>");
    html
}

fn build_footer_legal_links(settings: &Value) -> String {
    let get = |key: &str| -> &str { settings.get(key).and_then(|v| v.as_str()).unwrap_or("") };
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
        .unwrap_or("false")
        == "true";
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
    let get = |key: &str| -> &str { settings.get(key).and_then(|v| v.as_str()).unwrap_or("") };
    if get("cookie_consent_enabled") != "true" {
        return String::new();
    }

    let style = get("cookie_consent_style"); // minimal, modal, corner
    let position = get("cookie_consent_position"); // bottom, top
    let policy_url = get("cookie_consent_policy_url");
    let policy_url = if policy_url.is_empty() {
        "/privacy"
    } else {
        policy_url
    };
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
        format!(
            r#"<button id="cc-reject" style="padding:8px 20px;border-radius:6px;border:1px solid {border};font-size:13px;font-weight:500;cursor:pointer;background:transparent;color:{text}">Reject All</button>"#
        )
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

    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").or_else(|_| {
        chrono::NaiveDateTime::parse_from_str(&format!("{} 00:00:00", raw), "%Y-%m-%d %H:%M:%S")
    });

    match naive {
        Ok(ndt) => {
            let utc_dt =
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
            if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
                utc_dt
                    .with_timezone(&tz)
                    .format("%Y-%m-%dT%H:%M:%S%:z")
                    .to_string()
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
    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").or_else(|_| {
        chrono::NaiveDateTime::parse_from_str(&format!("{} 00:00:00", raw), "%Y-%m-%d %H:%M:%S")
    });

    match naive {
        Ok(ndt) => {
            // Try to apply timezone offset
            let utc_dt =
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
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
        result.push('…');
        result
    }
}

fn build_pagination(current: i64, total: i64) -> String {
    let mut html = String::from(r#"<nav class="pagination">"#);
    if current > 1 {
        html.push_str(&format!(
            r#"<a href="?page={}">&laquo; Prev</a>"#,
            current - 1
        ));
    }
    for p in 1..=total {
        if p == current {
            html.push_str(&format!(r#"<span class="current">{}</span>"#, p));
        } else {
            html.push_str(&format!(r#"<a href="?page={}">{}</a>"#, p, p));
        }
    }
    if current < total {
        html.push_str(&format!(
            r#"<a href="?page={}">Next &raquo;</a>"#,
            current + 1
        ));
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
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let display_type = sg("portfolio_display_type", "masonry");
    let show_tags_mode_raw = sg("portfolio_show_tags", "false");
    let show_tags_mode = if show_tags_mode_raw == "true" {
        "below_left".to_string()
    } else {
        show_tags_mode_raw
    };
    let show_tags = show_tags_mode != "false";
    let show_cats_mode_raw = sg("portfolio_show_categories", "false");
    let show_cats_mode = if show_cats_mode_raw == "true" {
        "below_left".to_string()
    } else {
        show_cats_mode_raw
    };
    let show_cats = show_cats_mode != "false";
    let show_likes_enabled = sg("portfolio_enable_likes", "true") == "true";
    let grid_hearts_pos = sg("portfolio_like_grid_position", "hidden");
    let show_grid_hearts = show_likes_enabled && grid_hearts_pos != "hidden";
    let fade_mode = sg("portfolio_fade_animation", "true");
    let fade_anim = fade_mode != "none" && fade_mode != "false";
    let fade_class = if fade_mode == "slide_up" {
        "slide-up"
    } else if fade_anim {
        "fade-in"
    } else {
        ""
    };
    let border_style = sg("portfolio_border_style", "none");
    let show_title = sg("portfolio_show_title", "false") == "true";
    let show_price = sg("commerce_show_price", "true") == "true";
    let price_position = sg("commerce_price_position", "top_right");
    let commerce_currency = {
        let c = sg("commerce_currency", "USD");
        if c.is_empty() {
            "USD".to_string()
        } else {
            c
        }
    };

    let grid_class = if display_type == "grid" {
        "css-grid"
    } else {
        "masonry-grid"
    };
    let border_class = match border_style.as_str() {
        "standard" => " border-standard",
        "polaroid" => " border-polaroid",
        _ => "",
    };
    let item_class_str = format!(
        "grid-item{}{}",
        if fade_class.is_empty() {
            String::new()
        } else {
            format!(" {}", fade_class)
        },
        border_class
    );

    // Determine if we need overlay positioning on the link
    let _cats_is_overlay = matches!(
        show_cats_mode.as_str(),
        "hover" | "bottom_left" | "bottom_right"
    );
    let _tags_is_overlay = matches!(
        show_tags_mode.as_str(),
        "hover" | "bottom_left" | "bottom_right"
    );
    let cats_is_below = matches!(show_cats_mode.as_str(), "below_left" | "below_right");
    let tags_is_below = matches!(show_tags_mode.as_str(), "below_left" | "below_right");

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
        let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let item_price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let item_sell = item
            .get("sell_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let cats_data = entry
            .get("categories")
            .and_then(|c| c.as_array())
            .map(|cats| {
                cats.iter()
                    .filter_map(|c| c.get("slug").and_then(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        let tag_data = if show_tags {
            tags.map(|tl| {
                tl.iter()
                    .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
        } else {
            String::new()
        };

        // Build category inner HTML (just the links, no wrapper div)
        let cats_inner = if show_cats {
            let cat_list = entry.get("categories").and_then(|c| c.as_array());
            if let Some(cats) = cat_list {
                if !cats.is_empty() {
                    let cat_strs: Vec<String> = cats
                        .iter()
                        .filter_map(|c| {
                            let name = c.get("name").and_then(|v| v.as_str())?;
                            let cslug = c.get("slug").and_then(|v| v.as_str())?;
                            Some(format!(
                                "<a href=\"{}\">{}</a>",
                                slug_url(&portfolio_slug, &format!("category/{}", cslug)),
                                html_escape(name)
                            ))
                        })
                        .collect();
                    if !cat_strs.is_empty() {
                        cat_strs.join(" · ")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Build tag inner HTML (just the links, no wrapper div)
        let tags_inner = if show_tags {
            if let Some(tag_list) = tags {
                if !tag_list.is_empty() {
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
                    if !tag_strs.is_empty() {
                        tag_strs.join(" · ")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let item_url = slug_url(&portfolio_slug, slug);
        // Price badge HTML (overlay positions: inside the <a> tag)
        let has_price_overlay =
            show_price && item_sell && item_price > 0.0 && price_position != "below_title";
        let price_badge_overlay = if has_price_overlay {
            let pos_style = match price_position.as_str() {
                "top_left" => "position:absolute;top:8px;left:8px",
                "bottom_left" => "position:absolute;bottom:8px;left:8px",
                _ => "position:absolute;top:8px;right:8px", // top_right default
            };
            format!(
                r#"<span class="price-badge" style="{};background:rgba(0,0,0,.75);color:#fff;padding:4px 10px;border-radius:4px;font-size:12px;font-weight:700;z-index:2">{} {:.2}</span>"#,
                pos_style,
                html_escape(&commerce_currency),
                item_price
            )
        } else {
            String::new()
        };

        // Heart overlay on grid thumbnail
        let heart_overlay = if show_grid_hearts {
            let h_is_top = grid_hearts_pos.starts_with("top");
            let h_is_right = grid_hearts_pos.ends_with("right");
            // Check if price badge is on the same corner
            let price_is_top = price_position.starts_with("top");
            let price_is_right = price_position.ends_with("right") || price_position == "top_right";
            let same_corner =
                has_price_overlay && (h_is_top == price_is_top) && (h_is_right == price_is_right);
            let v_offset = if same_corner {
                if h_is_top {
                    "top:36px"
                } else {
                    "bottom:36px"
                }
            } else if h_is_top {
                "top:8px"
            } else {
                "bottom:8px"
            };
            let h_offset = if h_is_right { "right:8px" } else { "left:8px" };
            format!(
                r#"<span class="like-btn grid-heart" data-id="{}" style="position:absolute;{};{};z-index:3;background:rgba(0,0,0,.45);color:#fff;padding:3px 8px;border-radius:16px;font-size:12px;cursor:pointer;user-select:none">&hearts; <span class="like-count">{}</span></span>"#,
                item_id,
                v_offset,
                h_offset,
                format_likes(likes)
            )
        } else {
            String::new()
        };

        html.push_str(&format!(
            r#"<div class="{item_class}" data-categories="{cats_data}" data-price="{price}" data-sell="{sell}">
    <a href="{item_url}" class="portfolio-link" data-id="{item_id}" data-title="{title}" data-likes="{likes}" data-tags="{tag_data}" style="position:relative;display:block">
        <img src="/uploads/{image}" alt="{title}" loading="lazy">
        {price_badge}
        {heart_overlay}
    </a>"#,
            item_class = item_class_str,
            cats_data = cats_data,
            item_url = item_url,
            item_id = item_id,
            title = html_escape(title),
            image = image,
            likes = likes,
            tag_data = html_escape(&tag_data),
            price = item_price,
            sell = item_sell,
            price_badge = price_badge_overlay,
            heart_overlay = heart_overlay,
        ));

        // Hover overlay: shared container with dimmed background, centered content
        let cats_is_hover = show_cats_mode == "hover";
        let tags_is_hover = show_tags_mode == "hover";
        if (cats_is_hover && !cats_inner.is_empty()) || (tags_is_hover && !tags_inner.is_empty()) {
            html.push_str("<div class=\"item-hover-overlay\">");
            html.push_str("<div class=\"item-hover-content\">");
            if cats_is_hover && !cats_inner.is_empty() {
                html.push_str(&format!(
                    "<div class=\"item-categories item-meta-hover\">{}</div>",
                    cats_inner
                ));
            }
            if tags_is_hover && !tags_inner.is_empty() {
                html.push_str(&format!(
                    "<div class=\"item-tags item-meta-hover\">{}</div>",
                    tags_inner
                ));
            }
            html.push_str("</div></div>");
        }

        // Corner overlays (bottom_left, bottom_right) — always visible, positioned in corners
        if show_cats_mode == "bottom_left" && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-bottom_left\">{}</div>",
                cats_inner
            ));
        }
        if show_cats_mode == "bottom_right" && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-bottom_right\">{}</div>",
                cats_inner
            ));
        }
        if show_tags_mode == "bottom_left" && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-bottom_left\">{}</div>",
                tags_inner
            ));
        }
        if show_tags_mode == "bottom_right" && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-bottom_right\">{}</div>",
                tags_inner
            ));
        }

        // Title below image
        if show_title && !title.is_empty() {
            html.push_str(&format!(
                r#"<div class="item-title">{}</div>"#,
                html_escape(title)
            ));
        }

        // Price badge: below_title position
        if show_price && item_sell && item_price > 0.0 && price_position == "below_title" {
            html.push_str(&format!(
                r#"<div class="price-badge-below" style="font-size:13px;font-weight:700;color:#333;padding:4px 0">{} {:.2}</div>"#,
                html_escape(&commerce_currency), item_price
            ));
        }

        // Below-image categories
        if cats_is_below && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-{}\">{}</div>",
                show_cats_mode, cats_inner
            ));
        }

        // Below-image tags
        if tags_is_below && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-{}\">{}</div>",
                show_tags_mode, tags_inner
            ));
        }

        html.push_str("</div>\n");
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
                    current_page + 1,
                    total_pages
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
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let show_likes = sg("portfolio_enable_likes", "true") == "true";
    let show_cats = sg("portfolio_show_categories", "false") != "false";
    let show_tags = sg("portfolio_show_tags", "false") != "false";
    let portfolio_slug = sg("portfolio_slug", "portfolio");

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let image = item
        .get("image_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let desc = item
        .get("description_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);
    let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

    let tags = context.get("tags").and_then(|t| t.as_array());
    let categories = context.get("categories").and_then(|c| c.as_array());

    let hearts_pos = sg("portfolio_like_position", "bottom_right");
    let like_overlay = if show_likes {
        let (h_top, h_left) = match hearts_pos.as_str() {
            "top_left" => ("top:12px", "left:12px"),
            "top_right" => ("top:12px", "right:12px"),
            "bottom_left" => ("bottom:12px", "left:12px"),
            _ => ("bottom:12px", "right:12px"),
        };
        format!(
            r#"<span class="like-btn" data-id="{}" style="position:absolute;{};{};z-index:10;background:rgba(0,0,0,.45);padding:4px 10px;border-radius:20px;color:#fff;font-size:14px">♥ <span class="like-count">{}</span></span>"#,
            item_id,
            h_top,
            h_left,
            format_likes(likes)
        )
    } else {
        String::new()
    };

    let share_pos = sg("share_icons_position", "below_content");
    let site_url = sg("site_url", "");
    let item_slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let page_url = if !site_url.is_empty() {
        format!("{}{}", site_url, slug_url(&portfolio_slug, item_slug))
    } else {
        String::new()
    };

    // Share buttons below image (between image and meta)
    let share_below_image = if share_pos == "below_image" && !page_url.is_empty() {
        build_share_buttons(&settings, &page_url, title)
    } else {
        String::new()
    };

    // Pre-build commerce HTML so we can place it at the right position
    let commerce_enabled = context
        .get("commerce_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let commerce_html = if commerce_enabled {
        let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let purchase_note = item
            .get("purchase_note")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let payment_provider = item
            .get("payment_provider")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        build_commerce_html(price, purchase_note, item_id, &settings, payment_provider)
    } else {
        String::new()
    };
    let commerce_pos = sg("commerce_button_position", "below_description");
    let is_sidebar_commerce = commerce_pos == "sidebar_right" && !commerce_html.is_empty();

    let mut html = String::from(r#"<article class="portfolio-single">"#);

    // Sidebar layout: wrap image+content in a flex row with commerce on the right
    if is_sidebar_commerce {
        html.push_str(r#"<div class="portfolio-single-row" style="display:flex;gap:32px;align-items:flex-start">"#);
        html.push_str(r#"<div class="portfolio-single-main" style="flex:1;min-width:0">"#);
    }

    html.push_str(&format!(
        r#"<div class="portfolio-image" style="position:relative">
        <img src="/uploads/{image}" alt="{title}">
        {like_overlay}
    </div>
    {share_below_image}"#,
        image = image,
        title = html_escape(title),
        like_overlay = like_overlay,
        share_below_image = share_below_image,
    ));

    // Commerce: below_image position
    if commerce_pos == "below_image" && !commerce_html.is_empty() {
        html.push_str(&commerce_html);
    }

    html.push_str(&format!(
        r#"<div class="portfolio-meta">
        <h1>{title}</h1>
    </div>"#,
        title = html_escape(title),
    ));

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

    if !desc.is_empty() {
        html.push_str(&format!(
            r#"<div class="portfolio-description">{}</div>"#,
            desc
        ));
    }

    // Share buttons — below content (after description)
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // Commerce: below_description position (default)
    if commerce_pos == "below_description" && !commerce_html.is_empty() {
        html.push_str(&commerce_html);
    }

    // Close sidebar layout main column and add commerce sidebar
    if is_sidebar_commerce {
        html.push_str("</div>"); // end portfolio-single-main
        html.push_str(
            r#"<div class="portfolio-single-sidebar" style="width:340px;flex-shrink:0">"#,
        );
        html.push_str(&commerce_html);
        html.push_str("</div>"); // end portfolio-single-sidebar
        html.push_str("</div>"); // end portfolio-single-row
    }

    // Comments on portfolio (gated on comments_enabled flag from route)
    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if comments_on {
        html.push_str(&build_comments_section(
            context,
            &settings,
            item_id,
            "portfolio",
        ));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = settings
            .get("site_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Velocty");
        let site_url = settings
            .get("site_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:8000");
        let portfolio_slug = settings
            .get("portfolio_slug")
            .and_then(|v| v.as_str())
            .unwrap_or("portfolio");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let meta_desc = item
            .get("meta_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
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
            html_escape(title),
            html_escape(meta_desc),
            site_url,
            image,
            site_url,
            slug_url(portfolio_slug, slug),
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
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
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
        _ => {
            if list_style == "editorial" {
                "blog-list blog-editorial"
            } else {
                "blog-list"
            }
        }
    };

    let blog_label = sg("blog_label", "journal");
    let mut html = format!(
        "<div class=\"{}\">\n<h1>{}</h1>",
        container_class,
        html_escape(&blog_label)
    );

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
        let author = post
            .get("author_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
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
            meta_parts.push(format!(
                "<span class=\"blog-author\">{}</span>",
                html_escape(author)
            ));
        }
        if show_date && !date.is_empty() {
            meta_parts.push(format!("<time>{}</time>", date));
        }
        if show_reading_time && word_count > 0 {
            meta_parts.push(format!(
                "<span class=\"reading-time\">{} min read</span>",
                reading_time
            ));
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

fn render_blog_single(context: &Value) -> String {
    let post = match context.get("post") {
        Some(p) => p,
        None => return render_404(context),
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
    let word_count = post.get("word_count").and_then(|v| v.as_i64()).unwrap_or(0);
    let reading_time = ((word_count as f64) / 200.0).ceil().max(1.0) as i64;

    let mut html = format!(
        "<article class=\"blog-single\">\n    <h1>{}</h1>",
        html_escape(title),
    );

    // Build meta line
    let mut meta_parts: Vec<String> = Vec::new();
    if show_author && !author.is_empty() {
        meta_parts.push(format!(
            "<span class=\"blog-author\">{}</span>",
            html_escape(author)
        ));
    }
    if show_date && !date.is_empty() {
        meta_parts.push(format!("<time>{}</time>", date));
    }
    if show_reading_time && word_count > 0 {
        meta_parts.push(format!(
            "<span class=\"reading-time\">{} min read</span>",
            reading_time
        ));
    }
    if !meta_parts.is_empty() {
        html.push_str(&format!(
            "\n    <div class=\"blog-meta\">{}</div>",
            meta_parts.join(" · ")
        ));
    }

    let share_pos = sg("share_icons_position", "below_content");
    let site_url = sg("site_url", "");
    let blog_slug = sg("blog_slug", "journal");
    let post_slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let page_url = if !site_url.is_empty() {
        format!("{}/{}/{}", site_url, blog_slug, post_slug)
    } else {
        String::new()
    };

    if !featured.is_empty() {
        html.push_str(&format!(
            r#"<div class="featured-image"><img src="/uploads/{}" alt="{}"></div>"#,
            featured,
            html_escape(title)
        ));
        // Share buttons below image
        if share_pos == "below_image" && !page_url.is_empty() {
            html.push_str(&build_share_buttons(&settings, &page_url, title));
        }
    }

    html.push_str(&format!(r#"<div class="post-content">{}</div>"#, content));

    // Tags
    if let Some(Value::Array(tags)) = context.get("tags") {
        if !tags.is_empty() {
            html.push_str("<div class=\"post-tags\">");
            let tag_strs: Vec<String> = tags
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(|v| v.as_str())?;
                    let tslug = t.get("slug").and_then(|v| v.as_str())?;
                    Some(format!(
                        "<a href=\"/{}/tag/{}\">{}</a>",
                        blog_slug,
                        tslug,
                        html_escape(name)
                    ))
                })
                .collect();
            html.push_str(&tag_strs.join(" "));
            html.push_str("</div>");
        }
    }

    // Share buttons — below content (after tags)
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // Prev / Next post navigation
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

    // Comments (gated on comments_enabled flag from route)
    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if comments_on {
        let post_id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        html.push_str(&build_comments_section(context, &settings, post_id, "post"));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = settings
            .get("site_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Velocty");
        let site_url = settings
            .get("site_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:8000");
        let blog_slug = settings
            .get("blog_slug")
            .and_then(|v| v.as_str())
            .unwrap_or("journal");
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
            site_url,
            blog_slug,
            slug,
            published,
            modified,
            html_escape(site_name),
        );
        if !image.is_empty() {
            ld.push_str(&format!(
                r#", "image": "{}/uploads/{}""#,
                site_url,
                html_escape(image)
            ));
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
    let _blog_slug = settings
        .get("blog_slug")
        .and_then(|v| v.as_str())
        .unwrap_or("journal");

    let mut html =
        String::from("<div class=\"blog-list\" style=\"padding:30px\"><h1>Archives</h1>");

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
            "01" => "January",
            "02" => "February",
            "03" => "March",
            "04" => "April",
            "05" => "May",
            "06" => "June",
            "07" => "July",
            "08" => "August",
            "09" => "September",
            "10" => "October",
            "11" => "November",
            "12" => "December",
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

    // Title/tags: position values (hidden/left/center/right) with backward compat for true/false
    var rawTitlePos = b.lbTitlePos || 'center';
    var rawTagsPos = b.lbTagsPos || 'center';
    if (rawTitlePos === 'false') rawTitlePos = 'hidden';
    if (rawTitlePos === 'true') rawTitlePos = 'center';
    if (rawTagsPos === 'false') rawTagsPos = 'hidden';
    if (rawTagsPos === 'true') rawTagsPos = 'center';
    const titlePos = rawTitlePos;
    const tagsPos = rawTagsPos;
    const showTitle = titlePos !== 'hidden';
    const showTags = tagsPos !== 'hidden';
    const showNav = b.lbNav !== 'false';
    const useKeyboard = b.lbKeyboard !== 'false';
    const showLikes = b.showLikes !== 'false';
    const sharePos = b.sharePosition || 'below_content';
    const siteUrl = b.siteUrl || '';
    const showLbShare = sharePos === 'below_image' && siteUrl;
    const lbBuy = b.lbBuy !== 'false';
    const lbBuyPos = b.lbBuyPosition || 'bottom_left';
    const commerceCur = b.commerceCurrency || 'USD';
    const heartsPos = b.heartsPosition || 'bottom_right';
    const navColor = b.lbNavColor || '#FFFFFF';

    // Lightbox setup — only when click mode is lightbox
    if (mode === 'lightbox') {

    const links = document.querySelectorAll('.portfolio-link');
    let overlay, img, titleEl, tagsEl, likesEl, shareEl, buyEl, closeBtn, prevBtn, nextBtn;
    let currentIndex = 0;
    const items = Array.from(links);

    function createOverlay() {
        overlay = document.createElement('div');
        overlay.className = 'lightbox-overlay';
        var buyPos = lbBuyPos === 'bottom_right' ? 'right:12px' : 'left:12px';
        var buyHtml = lbBuy ? '<div class="lb-buy" style="position:absolute;bottom:24px;' + buyPos + ';z-index:10"></div>' : '';
        var hIsBottom = heartsPos.indexOf('top') !== 0;
        var hIsRight = heartsPos.indexOf('right', 1) !== -1;
        var buyIsRight = lbBuyPos === 'bottom_right';
        var sameCorner = lbBuy && hIsBottom && (hIsRight === buyIsRight);
        var hTop = !hIsBottom ? 'top:12px' : (sameCorner ? 'bottom:70px' : 'bottom:24px');
        var hLeft = hIsRight ? 'right:12px' : 'left:12px';
        var heartsHtml = showLikes ? '<div class="lb-likes" style="position:absolute;' + hTop + ';' + hLeft + ';z-index:10;cursor:pointer;user-select:none;color:#fff;font-size:14px;background:rgba(0,0,0,.45);padding:4px 10px;border-radius:20px"></div>' : '';
        // Nav arrows inside the image wrapper, vertically centered
        var navHtml = '';
        if (showNav) {
            navHtml = '<button class="lb-prev" style="color:' + navColor + '">' +
                '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>' +
                '</button>' +
                '<button class="lb-next" style="color:' + navColor + '">' +
                '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>' +
                '</button>';
        }
        var titleAlign = titlePos === 'right' ? 'right' : titlePos === 'left' ? 'left' : 'center';
        var tagsAlign = tagsPos === 'right' ? 'right' : tagsPos === 'left' ? 'left' : 'center';
        overlay.innerHTML =
            '<button class="lb-close">&times;</button>' +
            '<div class="lb-content">' +
                '<div class="lb-image-wrap">' +
                    '<img class="lb-image" src="" alt="">' +
                    navHtml +
                    buyHtml +
                    heartsHtml +
                '</div>' +
                (showLbShare ? '<div class="lb-share"></div>' : '') +
                (showTitle ? '<div class="lb-title" style="text-align:' + titleAlign + '"></div>' : '') +
                (showTags ? '<div class="lb-tags" style="text-align:' + tagsAlign + '"></div>' : '') +
            '</div>';
        document.body.appendChild(overlay);
        img = overlay.querySelector('.lb-image');
        titleEl = overlay.querySelector('.lb-title');
        tagsEl = overlay.querySelector('.lb-tags');
        likesEl = overlay.querySelector('.lb-likes');
        shareEl = overlay.querySelector('.lb-share');
        buyEl = overlay.querySelector('.lb-buy');
        closeBtn = overlay.querySelector('.lb-close');
        prevBtn = overlay.querySelector('.lb-prev');
        nextBtn = overlay.querySelector('.lb-next');

        closeBtn.addEventListener('click', close);
        if (prevBtn) prevBtn.addEventListener('click', function(e) { e.stopPropagation(); navigate(-1); });
        if (nextBtn) nextBtn.addEventListener('click', function(e) { e.stopPropagation(); navigate(1); });
        overlay.addEventListener('click', function(e) { if (e.target === overlay) close(); });
        if (likesEl) {
            likesEl.addEventListener('click', function(e) {
                e.stopPropagation();
                var lid = likesEl.dataset.itemId;
                if (!lid) return;
                fetch('/api/like/' + lid, { method: 'POST' }).then(function(r){return r.json()}).then(function(d){
                    likesEl.innerHTML = '&#9829; ' + d.count;
                    if (d.liked) likesEl.classList.add('liked');
                    else likesEl.classList.remove('liked');
                    var link = items[currentIndex];
                    if (link) link.dataset.likes = d.count;
                }).catch(function(){});
            });
        }
    }

    function open(index) {
        if (!overlay) createOverlay();
        currentIndex = index;
        var link = items[index];
        var imgSrc = link.querySelector('img').src;
        var itemTitle = link.dataset.title || '';
        img.src = imgSrc;
        if (titleEl) titleEl.textContent = itemTitle;
        if (tagsEl) tagsEl.textContent = link.dataset.tags || '';
        if (likesEl) {
            var lk = link.dataset.likes || '0';
            likesEl.innerHTML = '&#9829; ' + lk;
            likesEl.classList.remove('liked');
            var lid = link.dataset.id;
            if (lid) {
                likesEl.dataset.itemId = lid;
                fetch('/api/like/' + lid + '/status').then(function(r){return r.json()}).then(function(d){
                    likesEl.innerHTML = '&#9829; ' + d.count;
                    if (d.liked) likesEl.classList.add('liked');
                    else likesEl.classList.remove('liked');
                }).catch(function(){});
            }
        }
        if (buyEl) {
            var gridItem = link.closest('.grid-item');
            var sell = gridItem ? gridItem.dataset.sell : 'false';
            var price = gridItem ? parseFloat(gridItem.dataset.price || '0') : 0;
            if (sell === 'true' && price > 0) {
                buyEl.innerHTML = '<a href="' + link.href + '" class="lb-buy-btn" style="display:inline-block;padding:10px 20px;background:#E8913A;color:#fff;border-radius:6px;text-decoration:none;font-weight:600;font-size:14px">' + commerceCur + ' ' + price.toFixed(2) + ' &mdash; Buy</a>';
                buyEl.style.display = '';
            } else {
                buyEl.innerHTML = '';
                buyEl.style.display = 'none';
            }
        }
        if (shareEl) {
            var shareUrl = encodeURIComponent(link.href || imgSrc);
            var shareTitle = encodeURIComponent(itemTitle);
            shareEl.innerHTML =
                '<a href="https://www.facebook.com/sharer/sharer.php?u=' + shareUrl + '" target="_blank" rel="noopener" class="lb-share-btn" title="Share on Facebook"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M18 2h-3a5 5 0 0 0-5 5v3H7v4h3v8h4v-8h3l1-4h-4V7a1 1 0 0 1 1-1h3z"/></svg></a>' +
                '<a href="https://twitter.com/intent/tweet?url=' + shareUrl + '&text=' + shareTitle + '" target="_blank" rel="noopener" class="lb-share-btn" title="Share on X"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg></a>' +
                '<a href="https://www.linkedin.com/sharing/share-offsite/?url=' + shareUrl + '" target="_blank" rel="noopener" class="lb-share-btn" title="Share on LinkedIn"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M16 8a6 6 0 0 1 6 6v7h-4v-7a2 2 0 0 0-2-2 2 2 0 0 0-2 2v7h-4v-7a6 6 0 0 1 6-6z"/><rect x="2" y="9" width="4" height="12"/><circle cx="4" cy="4" r="2"/></svg></a>';
        }
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

    document.addEventListener('keydown', function(e) {
        if (!overlay || !overlay.classList.contains('active')) return;
        if (e.key === 'Escape') close();
        if (useKeyboard) {
            if (e.key === 'ArrowLeft') navigate(-1);
            if (e.key === 'ArrowRight') navigate(1);
        }
    });

    } // end lightbox-only block

    // ── Like button handler (portfolio single page + any .like-btn) ──
    if (showLikes) {
        document.querySelectorAll('.like-btn').forEach(function(btn) {
            var lid = btn.dataset.id;
            if (!lid) return;
            // Check status on load
            fetch('/api/like/' + lid + '/status').then(function(r){return r.json()}).then(function(d){
                var countEl = btn.querySelector('.like-count');
                if (countEl) countEl.textContent = d.count;
                if (d.liked) btn.classList.add('liked');
            }).catch(function(){});
            // Click to toggle
            btn.addEventListener('click', function(e) {
                e.preventDefault(); e.stopPropagation();
                fetch('/api/like/' + lid, { method: 'POST' }).then(function(r){return r.json()}).then(function(d){
                    var countEl = btn.querySelector('.like-count');
                    if (countEl) countEl.textContent = d.count;
                    if (d.liked) btn.classList.add('liked');
                    else btn.classList.remove('liked');
                }).catch(function(){});
            });
        });
    }

    // JS Masonry layout
    function layoutMasonry() {
        var grid = document.querySelector('.masonry-grid');
        if (!grid) return;
        var items = grid.querySelectorAll('.grid-item');
        if (!items.length) return;
        var style = getComputedStyle(document.documentElement);
        var cols = parseInt(style.getPropertyValue('--grid-columns')) || 3;
        var gap = parseInt(style.getPropertyValue('--grid-gap')) || 12;
        // Responsive override
        if (window.innerWidth <= 768) cols = 1;
        else if (window.innerWidth <= 1024) cols = 2;
        var containerW = grid.clientWidth;
        var colW = (containerW - gap * (cols - 1)) / cols;
        var colHeights = [];
        for (var c = 0; c < cols; c++) colHeights.push(0);
        items.forEach(function(item) {
            // Find shortest column
            var minH = colHeights[0], minC = 0;
            for (var c = 1; c < cols; c++) {
                if (colHeights[c] < minH) { minH = colHeights[c]; minC = c; }
            }
            item.style.width = colW + 'px';
            item.style.left = (minC * (colW + gap)) + 'px';
            item.style.top = colHeights[minC] + 'px';
            colHeights[minC] += item.offsetHeight + gap;
        });
        var maxH = 0;
        for (var c = 0; c < cols; c++) { if (colHeights[c] > maxH) maxH = colHeights[c]; }
        grid.style.height = maxH + 'px';
    }
    // Run masonry after all images load
    var masonryGrid = document.querySelector('.masonry-grid');
    if (masonryGrid) {
        var imgs = masonryGrid.querySelectorAll('img');
        var loaded = 0;
        var total = imgs.length;
        if (total === 0) { layoutMasonry(); }
        else {
            imgs.forEach(function(img) {
                if (img.complete) { loaded++; if (loaded >= total) layoutMasonry(); }
                else {
                    img.addEventListener('load', function() { loaded++; if (loaded >= total) layoutMasonry(); });
                    img.addEventListener('error', function() { loaded++; if (loaded >= total) layoutMasonry(); });
                }
            });
        }
        window.addEventListener('resize', layoutMasonry);
    }

    // Fade/slide animation via IntersectionObserver
    if (b.fadeAnimation && b.fadeAnimation !== 'false' && b.fadeAnimation !== 'none') {
        var fadeItems = document.querySelectorAll('.grid-item.fade-in, .grid-item.slide-up');
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
                    layoutMasonry();
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
                    layoutMasonry();
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

/// Universal base CSS — shared across all designs. Stays in binary.
/// Contains: reset, body, typography, lightbox, comments, pagination, error, share icons.
pub const BASE_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    font-family: var(--font-body);
    font-size: var(--font-size-body);
    color: var(--color-text);
    background: var(--color-bg);
    line-height: var(--line-height, 1.6);
    text-transform: var(--text-transform);
    direction: var(--text-direction, ltr);
    text-align: var(--text-alignment, left);
}

a { color: var(--color-link, var(--color-accent)); }
a:hover { color: var(--color-link-hover, var(--color-accent)); }

h1, h2, h3, h4, h5, h6 { font-family: var(--font-heading); }
h1 { font-size: var(--font-size-h1); color: var(--color-heading, var(--color-text)); }
h2 { font-size: var(--font-size-h2); color: var(--color-subheading, var(--color-text)); }
h3 { font-size: var(--font-size-h3); color: var(--color-caption, var(--color-text)); }
h4 { font-size: var(--font-size-h4); }
h5 { font-size: var(--font-size-h5); }
h6 { font-size: var(--font-size-h6); }

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

.lb-share {
    display: flex;
    gap: 12px;
    justify-content: flex-start;
    margin-top: 10px;
}
.lb-share-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: rgba(255,255,255,.12);
    color: #fff;
    text-decoration: none;
    transition: background .2s;
}
.lb-share-btn:hover {
    background: rgba(255,255,255,.25);
    color: #fff;
}

.lb-image-wrap {
    position: relative;
    display: inline-block;
}

.lb-close {
    position: absolute;
    top: 16px; right: 20px;
    background: none; border: none;
    color: #fff; font-size: 32px;
    cursor: pointer;
    opacity: 0.7;
    z-index: 1001;
}
.lb-close:hover { opacity: 1; }

.lb-prev, .lb-next {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: rgba(0,0,0,0.4);
    border: none;
    border-radius: 50%;
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity .25s, background .25s;
    z-index: 10;
    padding: 0;
}
.lb-image-wrap:hover .lb-prev,
.lb-image-wrap:hover .lb-next { opacity: 0.8; }
.lb-prev:hover, .lb-next:hover { opacity: 1; background: rgba(0,0,0,0.65); }
.lb-prev { left: 12px; }
.lb-next { right: 12px; }

/* Lightbox buy button */
.lb-buy { text-align: center; }
.lb-buy-btn { transition: opacity .2s; }
.lb-buy-btn:hover { opacity: .85; }
.lb-sidebar { position: absolute; right: 20px; top: 50%; transform: translateY(-50%); width: 200px; text-align: center; }
.lb-content-sidebar { max-width: 65vw; }

/* ── Share Icons ── */
.share-icons {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: center;
    margin: 16px 0;
    padding: 12px 0;
    border-top: 1px solid rgba(0,0,0,.08);
}
.share-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text);
    text-decoration: none;
    transition: opacity .2s;
}
.share-icon:hover { opacity: .65; }

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
"#;

/// Oneguy shell HTML — the full page wrapper with {{placeholder}} tags.
/// Stored in designs.layout_html. The Rust renderer replaces placeholders with generated content.
pub const ONEGUY_SHELL_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {{seo_meta}}
    {{webmaster_meta}}
    {{favicon_link}}
{{font_links}}    <style>
        {{css_vars}}
        {{base_css}}
        {{design_css}}
    </style>
</head>
<body class="{{body_class}}" {{data_attrs}}>
    <div class="mobile-header">
        {{logo_html}}
        <button class="mobile-menu-btn" onclick="document.querySelector('.sidebar').classList.toggle('mobile-open')">&#9776;</button>
    </div>
    <div class="site-wrapper{{wrapper_classes}}">
        <aside class="sidebar">
            <div class="sidebar-top">
                <div class="site-logo">
                    {{logo_html}}
                    <h1 class="site-name"><a href="/">{{site_name}}</a></h1>
                    <p class="site-tagline">{{tagline}}</p>
                </div>
                <nav class="category-nav">
                    {{nav_links}}
                </nav>
                {{share_sidebar}}
                {{custom_sidebar_html}}
            </div>
            <div class="sidebar-bottom">
                {{social_sidebar}}
                {{footer_legal_links}}
            </div>
        </aside>
        <div class="content-column">
            <main class="content">
                {{body_content}}
            </main>
            <footer class="site-footer">
                {{footer_inner}}
            </footer>
        </div>
    </div>
    {{back_to_top}}
    <script>{{lightbox_js}}</script>
    {{image_protection_js}}
    {{analytics_scripts}}
    {{cookie_consent}}
</body>
</html>"#;

/// Oneguy top-bar shell HTML — horizontal header layout with {{placeholder}} tags.
/// Used when layout_header_type = "topbar".
pub const ONEGUY_TOPBAR_SHELL_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {{seo_meta}}
    {{webmaster_meta}}
    {{favicon_link}}
{{font_links}}    <style>
        {{css_vars}}
        {{base_css}}
        {{design_css}}
    </style>
</head>
<body class="topbar-layout {{body_class}}" {{data_attrs}}>
    <header class="topbar{{wrapper_classes}}">
        <div class="topbar-inner">
            <div class="topbar-brand">
                {{logo_html}}
                <h1 class="site-name"><a href="/">{{site_name}}</a></h1>
                <p class="site-tagline">{{tagline}}</p>
            </div>
            <nav class="topbar-nav">
                {{nav_links}}
            </nav>
            <div class="topbar-right">
                {{social_sidebar}}
                {{share_sidebar}}
            </div>
            <button class="topbar-hamburger" onclick="document.querySelector('.topbar-nav').classList.toggle('mobile-open');this.classList.toggle('active')" aria-label="Menu">
                <span></span><span></span><span></span>
            </button>
        </div>
    </header>
    {{categories_below_menu}}
    <div class="topbar-page{{wrapper_classes}}">
        <main class="content">
            {{body_content}}
        </main>
        <footer class="site-footer">
            {{footer_inner}}
        </footer>
    </div>
    {{back_to_top}}
    <script>{{lightbox_js}}</script>
    {{image_protection_js}}
    {{analytics_scripts}}
    {{cookie_consent}}
</body>
</html>"#;

/// Oneguy design CSS — layout-specific, seeded into DB on first run.
/// Contains: sidebar, nav, footer, grids, blog, portfolio, mobile.
pub const ONEGUY_DESIGN_CSS: &str = r#"
.site-wrapper {
    display: flex;
    flex-direction: var(--sidebar-direction, row);
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
    font-family: var(--font-logo, var(--font-primary));
    font-size: var(--font-size-logo, 22px);
    font-weight: 700;
    margin-bottom: 2px;
    line-height: 1.2;
}
.site-name a { color: var(--color-logo-text, var(--color-text)); text-decoration: none; }

.site-tagline {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-tagline, var(--color-text-secondary));
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
    font-size: var(--font-size-nav, 13px);
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

.social-links { margin-bottom: 8px; display: flex; align-items: center; flex-wrap: wrap; gap: 4px; }
.share-label {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    margin-right: 4px;
    white-space: nowrap;
}
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
    font-family: var(--font-footer, var(--font-captions));
    font-size: var(--font-size-footer, 10px);
    color: var(--color-footer, var(--color-text-secondary));
    margin-top: 6px;
    line-height: 1.5;
}

.site-footer {
    padding: 24px var(--grid-gap);
    border-top: 1px solid rgba(0,0,0,.08);
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
}
.footer-cell { display: flex; align-items: center; gap: 8px; }
.footer-left { justify-self: start; }
.footer-center { justify-self: center; }
.footer-right { justify-self: end; }
.footer-copyright {
    font-family: var(--font-footer, var(--font-captions));
    font-size: var(--font-size-footer, 12px);
    color: var(--color-footer, var(--color-text));
    line-height: 1;
    white-space: nowrap;
}
.footer-copyright a { color: inherit; }
.footer-social {
    display: flex;
    align-items: center;
    gap: 6px;
}
.social-icon-footer { display: flex; align-items: center; color: inherit; text-decoration: none; }
.social-icon-footer svg { width: 14px; height: 14px; }

.sidebar-top .share-icons {
    margin: 16px 0 0;
    padding: 0;
    border-top: none;
}

/* Custom sidebar text */
.sidebar-custom {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid rgba(128,128,128,.15);
}
.sidebar-custom-heading {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: .5px;
    margin-bottom: 8px;
    color: var(--color-text);
}
.sidebar-custom-text {
    font-size: 13px;
    line-height: 1.6;
    color: var(--color-text-secondary);
}

.content-column {
    margin-left: var(--sidebar-width);
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    max-width: var(--content-max-width, none);
    padding-top: var(--content-margin-top, 0);
    padding-bottom: var(--content-margin-bottom, 0);
    padding-left: var(--content-margin-left, 0);
    padding-right: var(--content-margin-right, 0);
}
/* Right sidebar: flip margin + sidebar position */
.sidebar-right { flex-direction: row-reverse; }
.sidebar-right .sidebar { left: auto; right: 0; }
.sidebar-right .content-column {
    margin-left: 0;
    margin-right: var(--sidebar-width);
}
/* Boxed mode: entire site constrained + centered */
body.boxed-mode {
    background: #ededed;
}
.layout-boxed {
    max-width: var(--content-max-width, 1200px);
    margin: 0 auto;
    background: var(--color-bg);
    box-shadow: 0 0 60px rgba(0,0,0,.07);
}
.layout-boxed .sidebar {
    left: calc((100vw - var(--content-max-width, 1200px)) / 2);
}
.layout-boxed.sidebar-right .sidebar {
    left: auto;
    right: calc((100vw - var(--content-max-width, 1200px)) / 2);
}
/* ── Footer Behavior: Fixed Reveal ── */
body.footer-fixed-reveal .site-footer {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 1;
    background: var(--color-bg);
}
body.footer-fixed-reveal .content-column {
    position: relative;
    z-index: 2;
    background: var(--color-bg);
    margin-bottom: 80px;
}
body.footer-fixed-reveal .topbar-page {
    position: relative;
    z-index: 2;
    background: var(--color-bg);
    margin-bottom: 80px;
}
body.footer-fixed-reveal .sidebar {
    z-index: 3;
}
/* ── Footer Behavior: Always Visible ── */
body.footer-always-visible .site-footer {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 999;
    background: var(--color-bg);
    box-shadow: 0 -2px 8px rgba(0,0,0,.08);
}
body.footer-always-visible .content-column {
    padding-bottom: 80px;
}
body.footer-always-visible .topbar-page {
    padding-bottom: 80px;
}
/* Sidebar layout: offset fixed footer from sidebar */
body.footer-fixed-reveal .site-wrapper .site-footer {
    left: var(--sidebar-width);
}
body.footer-always-visible .site-wrapper .site-footer {
    left: var(--sidebar-width);
}
body.footer-fixed-reveal .sidebar-right .site-footer {
    left: 0;
    right: var(--sidebar-width);
}
body.footer-always-visible .sidebar-right .site-footer {
    left: 0;
    right: var(--sidebar-width);
}
/* Topbar layout: footer spans full width */
body.topbar-layout.footer-fixed-reveal .site-footer,
body.topbar-layout.footer-always-visible .site-footer {
    left: 0;
    right: 0;
}

/* ── Content ── */
.content {
    flex: 1;
    padding: 0;
}

/* ── Portfolio Masonry Grid ── */
.masonry-grid {
    position: relative;
    padding: var(--grid-gap);
}
.masonry-grid .grid-item {
    position: absolute;
    width: calc((100% - var(--grid-gap) * (var(--grid-columns) - 1)) / var(--grid-columns));
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
    position: relative;
    overflow: hidden;
    border-radius: 6px;
    transition: transform 0.3s ease, box-shadow 0.3s ease;
}

.grid-item:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgba(0,0,0,.12);
}

.css-grid .grid-item { margin-bottom: 0; }

.grid-item img {
    width: 100%;
    display: block;
    border-radius: 6px;
}

.css-grid .grid-item .portfolio-link img {
    aspect-ratio: 1 / 1;
    object-fit: cover;
}

.grid-item .portfolio-link {
    display: block;
    border-radius: 6px;
    overflow: hidden;
}

.grid-item .portfolio-link img {
    transition: transform 0.4s ease, opacity 0.3s ease;
}

.grid-item:hover .portfolio-link img {
    transform: scale(1.03);
    opacity: 0.92;
}

/* ── Item categories & tags position modes ── */
.item-categories {
    font-family: var(--font-categories, var(--font-captions));
    font-size: var(--font-size-categories, 11px);
    color: var(--color-categories, var(--color-text-secondary));
}
.item-categories a { color: inherit; text-decoration: none; }
.item-categories a:hover { text-decoration: underline; }

/* Hover overlay: dimmed background, centered content */
.item-hover-overlay {
    position: absolute;
    inset: 0;
    z-index: 3;
    background: rgba(0,0,0,.6);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.3s ease;
    pointer-events: none;
}
.grid-item:hover .item-hover-overlay {
    opacity: 1;
    pointer-events: auto;
}
.item-hover-content {
    text-align: center;
    padding: 16px;
    max-width: 90%;
}
.item-hover-content .item-categories {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: #fff;
}
.item-hover-content .item-tags {
    font-size: 11px;
    color: rgba(255,255,255,.75);
    margin-top: 8px;
}
.item-hover-content a { color: #fff; text-decoration: none; }
.item-hover-content a:hover { text-decoration: underline; }

/* Corner overlays (bottom_left, bottom_right) — always visible */
.item-meta-bottom_left,
.item-meta-bottom_right {
    position: absolute;
    z-index: 2;
    padding: 5px 10px;
    font-size: 11px;
    background: rgba(0,0,0,.55);
    color: #fff;
    pointer-events: auto;
}
.item-meta-bottom_left a,
.item-meta-bottom_right a { color: #fff; text-decoration: none; }
.item-meta-bottom_left a:hover,
.item-meta-bottom_right a:hover { text-decoration: underline; }
.item-meta-bottom_left { bottom: 0; left: 0; border-radius: 0 4px 0 0; }
.item-meta-bottom_right { bottom: 0; right: 0; border-radius: 4px 0 0 0; }

/* Below modes (outside portfolio-link, under image) */
.item-meta-below_left { text-align: left; }
.item-meta-below_right { text-align: right; }
.item-categories.item-meta-below_left,
.item-categories.item-meta-below_right {
    padding: 4px 0 2px;
}
.item-tags.item-meta-below_left,
.item-tags.item-meta-below_right {
    padding: 4px 0 8px;
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
.grid-item.slide-up {
    opacity: 0;
    transform: translateY(40px);
    transition: opacity 0.6s ease, transform 0.6s ease;
}
.grid-item.slide-up.visible {
    opacity: 1;
    transform: translateY(0);
}

/* Border styles */
.grid-item.border-standard .portfolio-link {
    border: 10px solid var(--color-border, #e5e7eb);
}
.grid-item.border-polaroid {
    background: #fff;
    padding: 10px 10px 40px 10px;
    box-shadow: 0 2px 8px rgba(0,0,0,.12);
}
.grid-item.border-polaroid .portfolio-link {
    overflow: hidden;
}

/* Title below image */
.item-title {
    font-family: var(--font-body, var(--font-primary));
    font-size: 13px;
    color: var(--color-text);
    padding: 6px 0 2px;
    text-align: left;
}

.item-tags {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    padding: 4px 0 8px;
}
.item-tags a { color: var(--color-text-secondary); text-decoration: none; }
.item-tags a:hover { text-decoration: underline; }

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
.like-btn { cursor: pointer; font-size: 18px; user-select: none; transition: color 0.2s; }
.like-btn.liked { color: #e74c3c; }
.lb-likes.liked { color: #e74c3c !important; }
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
    .css-grid { grid-template-columns: repeat(2, 1fr); }
}

@media (max-width: 768px) {
    .css-grid { grid-template-columns: 1fr; }
    .blog-item { flex-direction: column; }
    .blog-thumb img { width: 100%; height: auto; }
    .blog-single { padding: 20px 16px; }
    .portfolio-single { padding: 20px 16px; }
}

/* ═══════════════════════════════════════════════════
   TOP BAR LAYOUT
   ═══════════════════════════════════════════════════ */

body.topbar-layout .sidebar,
body.topbar-layout .mobile-header,
body.topbar-layout .site-wrapper { display: none; }

.topbar {
    background: var(--color-bg);
    border-bottom: 1px solid rgba(0,0,0,.08);
    padding: 0 32px;
    position: sticky;
    top: 0;
    z-index: 100;
}
.topbar.layout-boxed {
    max-width: var(--content-max-width, 1200px);
    margin: 0 auto;
}

.topbar-inner {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    min-height: 60px;
    gap: 8px;
}

/* Brand area: logo + name + tagline */
.topbar-brand {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
}
.topbar-brand .site-name { margin-bottom: 0; }
.topbar-brand .site-tagline { margin-bottom: 0; font-size: 11px; }
.topbar-brand .logo-img { max-height: 40px; margin-bottom: 0; }

/* Navigation */
.topbar-nav {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-nav);
    font-size: 13px;
    margin-left: 32px;
}
.topbar-nav .nav-link {
    padding: 6px 12px;
    color: var(--color-text);
    text-decoration: none;
    white-space: nowrap;
    border-radius: 4px;
    transition: background 0.15s, color 0.15s;
}
.topbar-nav .nav-link:hover {
    color: var(--color-accent);
    background: rgba(0,0,0,.04);
}

/* Nav right of logo: push nav + right section to the right */
.topbar-nav-right .topbar-nav { margin-left: auto; }
.topbar-nav-right .topbar-right { margin-left: 16px; }

/* Below logo: nav wraps to second row */
.topbar:not(.topbar-nav-right) .topbar-inner {
    flex-wrap: wrap;
}
.topbar:not(.topbar-nav-right) .topbar-brand {
    width: 100%;
    padding-top: 12px;
}
.topbar:not(.topbar-nav-right) .topbar-nav {
    margin-left: 0;
    padding-bottom: 8px;
}
.topbar:not(.topbar-nav-right) .topbar-right {
    margin-left: auto;
    padding-bottom: 8px;
}

/* Right section: social + share icons */
.topbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-left: auto;
}
.topbar-right .social-links {
    margin-bottom: 0;
}
.topbar-right .share-icons {
    margin: 0;
    padding: 0;
    border: none;
}

/* Page Top categories (sidebar layout) */
.categories-page-top {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 0 16px;
    font-family: var(--font-nav);
    font-size: 13px;
}
.categories-page-top.cats-right {
    justify-content: flex-end;
}
.categories-page-top .cat-link {
    padding: 4px 12px;
    color: var(--color-text);
    text-decoration: none;
    border-radius: 4px;
    transition: background 0.15s, color 0.15s;
}
.categories-page-top .cat-link:hover {
    color: var(--color-accent);
    background: rgba(0,0,0,.04);
}
.categories-page-top .cat-link.active {
    color: var(--color-accent);
    font-weight: 600;
}

/* Below Main Menu categories (topbar layout) */
.categories-below-menu {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 32px 10px;
    font-family: var(--font-nav);
    font-size: 12px;
    background: var(--color-bg);
    border-bottom: 1px solid rgba(0,0,0,.05);
}
.categories-below-menu .cat-link {
    padding: 3px 10px;
    color: var(--color-text-secondary, var(--color-text));
    text-decoration: none;
    border-radius: 4px;
    transition: background 0.15s, color 0.15s;
}
.categories-below-menu .cat-link:hover {
    color: var(--color-accent);
    background: rgba(0,0,0,.04);
}
.categories-below-menu .cat-link.active {
    color: var(--color-accent);
    font-weight: 600;
}

/* Submenu dropdown for categories */
.topbar-nav .nav-category-group {
    position: relative;
}
.topbar-nav .nav-category-toggle {
    padding: 6px 12px;
    font-size: 13px;
    border-radius: 4px;
    cursor: pointer;
}
.topbar-nav .nav-category-toggle:hover {
    background: rgba(0,0,0,.04);
}
.topbar-nav .nav-subcategories {
    display: none;
    position: absolute;
    top: 100%;
    left: 0;
    background: var(--color-bg);
    border: 1px solid rgba(0,0,0,.1);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0,0,0,.1);
    padding: 6px 0;
    min-width: 160px;
    z-index: 200;
    flex-direction: column;
}
.topbar-nav .nav-category-toggle.open + .nav-subcategories {
    display: flex;
}
.topbar-nav .nav-subcategories .cat-link {
    padding: 6px 16px;
    color: var(--color-text);
    text-decoration: none;
    font-size: 13px;
    white-space: nowrap;
}
.topbar-nav .nav-subcategories .cat-link:hover {
    background: rgba(0,0,0,.04);
    color: var(--color-accent);
}
.topbar-nav .nav-subcategories .cat-link.active {
    color: var(--color-accent);
    font-weight: 600;
}

/* Page content area */
.topbar-page {
    min-height: calc(100vh - 60px);
    display: flex;
    flex-direction: column;
}
.topbar-page.layout-boxed {
    max-width: var(--content-max-width, 1200px);
    margin: 0 auto;
}
.topbar-page .content {
    flex: 1;
    margin-left: 0;
    padding: var(--grid-gap);
}
.topbar-page .site-footer {
    margin-top: auto;
}

/* Hamburger button */
.topbar-hamburger {
    display: none;
    flex-direction: column;
    justify-content: center;
    gap: 5px;
    background: none;
    border: none;
    cursor: pointer;
    padding: 8px;
    margin-left: auto;
}
.topbar-hamburger span {
    display: block;
    width: 22px;
    height: 2px;
    background: var(--color-text);
    border-radius: 2px;
    transition: transform 0.2s, opacity 0.2s;
}
.topbar-hamburger.active span:nth-child(1) { transform: translateY(7px) rotate(45deg); }
.topbar-hamburger.active span:nth-child(2) { opacity: 0; }
.topbar-hamburger.active span:nth-child(3) { transform: translateY(-7px) rotate(-45deg); }

/* Topbar responsive */
@media (max-width: 900px) {
    body.topbar-layout .topbar-hamburger { display: flex; }
    body.topbar-layout .topbar-nav {
        display: none;
        flex-direction: column;
        width: 100%;
        padding: 8px 0 12px;
        margin-left: 0;
        gap: 2px;
    }
    body.topbar-layout .topbar-nav.mobile-open {
        display: flex;
    }
    body.topbar-layout .topbar-nav .nav-link {
        padding: 8px 12px;
        width: 100%;
    }
    body.topbar-layout .topbar-right {
        display: none;
    }
    body.topbar-layout .topbar-nav.mobile-open ~ .topbar-right {
        display: flex;
        width: 100%;
        padding: 0 12px 12px;
    }
    body.topbar-layout .topbar-inner {
        flex-wrap: wrap;
    }
    body.topbar-layout .topbar-brand {
        flex: 1;
    }
    /* Submenu in mobile: inline instead of dropdown */
    body.topbar-layout .topbar-nav .nav-subcategories {
        position: static;
        box-shadow: none;
        border: none;
        padding: 0 0 0 16px;
    }
}
"#;

fn build_commerce_html(
    price: f64,
    purchase_note: &str,
    item_id: i64,
    settings: &Value,
    payment_provider: &str,
) -> String {
    let gs = |key: &str| -> &str { settings.get(key).and_then(|v| v.as_str()).unwrap_or("") };
    let enabled = |key: &str| -> bool { gs(key) == "true" };

    let currency = {
        let c = gs("commerce_currency");
        if c.is_empty() {
            "USD"
        } else {
            c
        }
    };

    // Determine which single provider to use for this item
    let provider = if !payment_provider.is_empty() {
        payment_provider.to_string()
    } else {
        // Fallback for legacy items: use first enabled provider
        let providers = [
            ("paypal", "commerce_paypal_enabled", "paypal_client_id"),
            (
                "stripe",
                "commerce_stripe_enabled",
                "stripe_publishable_key",
            ),
            ("razorpay", "commerce_razorpay_enabled", "razorpay_key_id"),
            ("mollie", "commerce_mollie_enabled", "mollie_api_key"),
            ("square", "commerce_square_enabled", "square_access_token"),
            (
                "2checkout",
                "commerce_2checkout_enabled",
                "twocheckout_merchant_code",
            ),
            (
                "payoneer",
                "commerce_payoneer_enabled",
                "payoneer_client_id",
            ),
        ];
        providers
            .iter()
            .find(|(_, en, key)| enabled(en) && !gs(key).is_empty())
            .map(|(name, _, _)| name.to_string())
            .unwrap_or_default()
    };

    if provider.is_empty() {
        return String::new();
    }

    // Designer settings
    let btn_alignment = {
        let a = gs("commerce_button_alignment");
        if a.is_empty() {
            "full_width"
        } else {
            a
        }
    };
    let btn_radius = {
        let r = gs("commerce_button_radius");
        match r {
            "square" => "0",
            "pill" => "999px",
            _ => "8px", // "rounded" default
        }
    };
    let custom_color = gs("commerce_button_color");
    let custom_label = gs("commerce_button_label");

    let width_style = if btn_alignment == "full_width" {
        "display:block;width:100%"
    } else {
        "display:inline-block"
    };
    let align_style = match btn_alignment {
        "left" => "text-align:left",
        "right" => "text-align:right",
        "center" => "text-align:center",
        _ => "", // full_width needs no text-align
    };
    let btn_style = format!(
        "{};padding:12px 24px;border:none;border-radius:{};font-size:15px;font-weight:600;cursor:pointer;margin-bottom:8px;text-align:center",
        width_style, btn_radius
    );

    let btn_position = {
        let p = gs("commerce_button_position");
        if p.is_empty() {
            "below_description"
        } else {
            p
        }
    };

    let mut s = String::new();
    s.push_str(&format!(
        r#"<div class="commerce-section" data-position="{}" style="margin-top:32px;padding:24px;border-radius:12px;border:1px solid #e0e0e0{}">"#,
        btn_position,
        if !align_style.is_empty() { format!(";{}", align_style) } else { String::new() }
    ));

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
        _ => {
            // All non-PayPal providers use HTML buttons with customizable styling
            let (default_bg, default_label, btn_id, onclick) = match provider.as_str() {
                "stripe" => (
                    "#635BFF",
                    "Pay with Stripe",
                    "stripe-buy-btn",
                    "commerceRedirect('stripe')",
                ),
                "razorpay" => (
                    "#072654",
                    "Pay with Razorpay",
                    "razorpay-buy-btn",
                    "commerceRazorpay()",
                ),
                "mollie" => (
                    "#0a0a0a",
                    "Pay with Mollie",
                    "mollie-buy-btn",
                    "commerceRedirect('mollie')",
                ),
                "square" => (
                    "#006AFF",
                    "Pay with Square",
                    "square-buy-btn",
                    "commerceRedirect('square')",
                ),
                "2checkout" => (
                    "#F36F21",
                    "Pay with 2Checkout",
                    "2co-buy-btn",
                    "commerceRedirect('2checkout')",
                ),
                "payoneer" => (
                    "#FF6C00",
                    "Pay with Payoneer",
                    "payoneer-buy-btn",
                    "commerceRedirect('payoneer')",
                ),
                _ => return String::new(),
            };
            let bg = if custom_color.is_empty() {
                default_bg
            } else {
                custom_color
            };
            let label = if custom_label.is_empty() {
                default_label
            } else {
                custom_label
            };
            s.push_str(&format!(
                r#"<button type="button" id="{}" style="{};background:{};color:#fff" onclick="{}">{}</button>"#,
                btn_id, btn_style, bg, onclick, html_escape(label)
            ));
        }
    }

    s.push_str("</div>"); // end commerce-email-step

    // Processing state
    s.push_str(r#"<div id="commerce-processing" style="display:none;text-align:center;padding:20px"><p style="color:#888">Processing your purchase...</p></div>"#);

    // Success state
    s.push_str(
        r#"<div id="commerce-success" style="display:none;text-align:center;padding:20px">"#,
    );
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
    s.push_str(
        "document.getElementById('commerce-download-link').href='/download/'+d.download_token;",
    );
    s.push_str("if(d.license_key){document.getElementById('commerce-license').style.display='';document.getElementById('commerce-license-key').textContent=d.license_key;}}\n");

    // Shared: show processing
    s.push_str("function _vProc(){document.getElementById('commerce-email-step').style.display='none';document.getElementById('commerce-processing').style.display='';}\n");

    // Redirect-based providers (Stripe, Mollie, Square, 2Checkout, Payoneer)
    if matches!(
        provider.as_str(),
        "stripe" | "mollie" | "square" | "2checkout" | "payoneer"
    ) {
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
        s.push_str(
            "})}).then(function(r){return r.json()}).then(function(v){_vSuccess(v);});\n},\n",
        );
        s.push_str("modal:{ondismiss:function(){document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';}}\n");
        s.push_str("};\nnew Razorpay(opts).open();\n");
        s.push_str("}).catch(function(e){alert('Error: '+e.message);document.getElementById('commerce-processing').style.display='none';document.getElementById('commerce-email-step').style.display='';});\n}\n");
    }

    // PayPal JS SDK
    if provider == "paypal" {
        let pp_id = gs("paypal_client_id");
        let pp_cur = {
            let c = gs("paypal_currency");
            if c.is_empty() {
                currency
            } else {
                c
            }
        };
        s.push_str("</script>\n");
        s.push_str(&format!("<script src=\"https://www.paypal.com/sdk/js?client-id={}&currency={}\"></script>\n<script>\n", html_escape(pp_id), html_escape(pp_cur)));
        let pp_color = {
            let c = gs("paypal_button_color");
            if c.is_empty() {
                "gold"
            } else {
                c
            }
        };
        let pp_shape = {
            let c = gs("paypal_button_shape");
            if c.is_empty() {
                "rect"
            } else {
                c
            }
        };
        let pp_label = {
            let c = gs("paypal_button_label");
            if c.is_empty() {
                "pay"
            } else {
                c
            }
        };
        s.push_str("paypal.Buttons({\n");
        s.push_str(&format!(
            "style:{{layout:'vertical',color:'{}',shape:'{}',label:'{}'}},\n",
            pp_color, pp_shape, pp_label
        ));
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
