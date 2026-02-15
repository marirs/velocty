use serde_json::Value;

use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::seo;
use crate::typography;

/// Renders a full page by merging the active design template with content data.
/// In Phase 1, this uses hardcoded default templates.
/// In Phase 3, this will load from the GrapesJS design system.
pub fn render_page(pool: &DbPool, template_type: &str, context: &Value) -> String {
    let settings = context.get("settings").cloned().unwrap_or_default();

    // Build CSS variables from settings
    let css_vars = typography::build_css_variables(&settings);

    // Get the page-specific HTML
    let body_html = match template_type {
        "homepage" | "portfolio_grid" => render_portfolio_grid(context),
        "portfolio_single" => render_portfolio_single(context),
        "blog_list" => render_blog_list(context),
        "blog_single" => render_blog_single(context),
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
        .get("site_tagline")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Build the sidebar categories
    let categories_html = build_categories_sidebar(context);

    // Build social links
    let social_html = build_social_links(&settings);

    // Build font loading tags
    let font_links = typography::build_font_links(&settings);

    // Full page shell — the default "Sidebar Portfolio" design
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {seo_meta}
    {webmaster_meta}
{font_links}    <style>
        {css_vars}
        {base_css}
    </style>
</head>
<body>
    <div class="site-wrapper">
        <aside class="sidebar">
            <div class="sidebar-top">
                <div class="site-logo">
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
                <div class="footer-text">
                    <p>&copy; {year} {site_name}</p>
                </div>
            </div>
        </aside>
        <main class="content">
            {body_html}
        </main>
    </div>
    <script>{lightbox_js}</script>
    {image_protection_js}
    {analytics_scripts}
    {cookie_consent}
</body>
</html>"#,
        seo_meta = seo_meta,
        webmaster_meta = seo::build_webmaster_meta(&settings),
        font_links = font_links,
        css_vars = css_vars,
        base_css = DEFAULT_CSS,
        site_name = html_escape(site_name),
        tagline = html_escape(site_tagline),
        categories_html = categories_html,
        social_html = social_html,
        year = chrono::Utc::now().format("%Y"),
        body_html = body_html,
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
    let site_tagline = settings.get("site_tagline").map(|s| s.as_str()).unwrap_or("");

    let categories = crate::models::category::Category::list(pool, Some("portfolio"));
    let cats_json = serde_json::to_value(&categories).unwrap_or_default();
    let ctx = serde_json::json!({ "categories": cats_json });
    let categories_html = build_categories_sidebar(&ctx);

    let analytics_scripts = seo::build_analytics_scripts(&settings_json);
    let font_links = typography::build_font_links(&settings_json);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} — {site_name}</title>
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
    {analytics_scripts}
    {cookie_consent}
</body>
</html>"#,
        title = html_escape(title),
        site_name = html_escape(site_name),
        tagline = html_escape(site_tagline),
        font_links = font_links,
        css_vars = css_vars,
        base_css = DEFAULT_CSS,
        categories_html = categories_html,
        social_html = social_html,
        year = chrono::Utc::now().format("%Y"),
        body = html_body,
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

    let active_slug = context
        .get("active_category")
        .and_then(|c| c.get("slug"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let mut html = String::new();
    for cat in categories {
        let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let active_class = if slug == active_slug { " active" } else { "" };
        html.push_str(&format!(
            r#"<a href="/portfolio/category/{slug}" class="cat-link{active}" data-category="{slug}">{name}</a>"#,
            slug = slug,
            name = html_escape(name),
            active = active_class,
        ));
        html.push('\n');
    }
    html
}

fn build_social_links(settings: &Value) -> String {
    let links_json = settings
        .get("social_links")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");

    // Social links are stored as JSON array of {platform, url, icon}
    // For now, return empty — will be populated when user configures
    let _links: Vec<Value> = serde_json::from_str(links_json).unwrap_or_default();
    String::from(r#"<div class="social-links"></div>"#)
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


fn render_portfolio_grid(context: &Value) -> String {
    let items = match context.get("items") {
        Some(Value::Array(items)) => items,
        _ => return "<p>No portfolio items yet.</p>".to_string(),
    };

    if items.is_empty() {
        return "<p>No portfolio items yet.</p>".to_string();
    }

    let mut html = String::from(r#"<div class="masonry-grid">"#);

    for entry in items {
        let item = entry.get("item").unwrap_or(entry);
        let tags = entry.get("tags").and_then(|t| t.as_array());

        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let image = item
            .get("thumbnail_path")
            .or_else(|| item.get("image_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);

        html.push_str(&format!(
            r#"<div class="grid-item" data-categories="{cats_data}">
    <a href="/portfolio/{slug}" class="portfolio-link" data-title="{title}" data-likes="{likes}">
        <img src="/uploads/{image}" alt="{title}" loading="lazy">
    </a>"#,
            slug = slug,
            title = html_escape(title),
            image = image,
            likes = likes,
            cats_data = entry
                .get("categories")
                .and_then(|c| c.as_array())
                .map(|cats| cats
                    .iter()
                    .filter_map(|c| c.get("slug").and_then(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" "))
                .unwrap_or_default(),
        ));

        // Tags below image
        if let Some(tag_list) = tags {
            if !tag_list.is_empty() {
                html.push_str(r#"<div class="item-tags">"#);
                let tag_strs: Vec<String> = tag_list
                    .iter()
                    .filter_map(|t| {
                        let name = t.get("name").and_then(|v| v.as_str())?;
                        let slug = t.get("slug").and_then(|v| v.as_str())?;
                        Some(format!(
                            r#"<a href="/portfolio/tag/{}">{}</a>"#,
                            slug,
                            html_escape(name)
                        ))
                    })
                    .collect();
                html.push_str(&tag_strs.join(" · "));
                html.push_str("</div>");
            }
        }

        html.push_str("</div>\n");
    }

    html.push_str("</div>");
    html
}

fn render_portfolio_single(context: &Value) -> String {
    let item = match context.get("item") {
        Some(i) => i,
        None => return render_404(context),
    };

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let image = item.get("image_path").and_then(|v| v.as_str()).unwrap_or("");
    let desc = item
        .get("description_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);

    let tags = context.get("tags").and_then(|t| t.as_array());
    let categories = context.get("categories").and_then(|c| c.as_array());

    let mut html = format!(
        r#"<article class="portfolio-single">
    <div class="portfolio-image">
        <img src="/uploads/{image}" alt="{title}">
    </div>
    <div class="portfolio-meta">
        <h1>{title}</h1>
        <span class="like-btn" data-id="{id}">♥ <span class="like-count">{likes}</span></span>
    </div>"#,
        image = image,
        title = html_escape(title),
        id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
        likes = format_likes(likes),
    );

    if let Some(cats) = categories {
        if !cats.is_empty() {
            html.push_str(r#"<div class="portfolio-categories">"#);
            for cat in cats {
                let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                html.push_str(&format!(
                    r#"<a href="/portfolio/category/{}">{}</a>"#,
                    slug,
                    html_escape(name)
                ));
            }
            html.push_str("</div>");
        }
    }

    if !desc.is_empty() {
        html.push_str(&format!(r#"<div class="portfolio-description">{}</div>"#, desc));
    }

    // Commerce: Buy / Download section
    let commerce_enabled = context.get("commerce_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if commerce_enabled {
        let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let purchase_note = item.get("purchase_note").and_then(|v| v.as_str()).unwrap_or("");
        let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let settings = context.get("settings").cloned().unwrap_or_default();
        let payment_provider = item.get("payment_provider").and_then(|v| v.as_str()).unwrap_or("");

        html.push_str(&build_commerce_html(price, purchase_note, item_id, &settings, payment_provider));
    }

    // Comments on portfolio (gated on comments_enabled flag from route)
    let comments_on = context.get("comments_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if comments_on {
        let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let settings = context.get("settings").cloned().unwrap_or_default();
        html.push_str(&build_comments_section(context, &settings, item_id, "portfolio"));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    let settings = context.get("settings").cloned().unwrap_or_default();
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
    "url": "{}/{}/{}",
    "publisher": {{ "@type": "Organization", "name": "{}" }}
}}
</script>"#,
            html_escape(title), html_escape(meta_desc),
            site_url, image,
            site_url, portfolio_slug, slug,
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

    let mut html = String::from(r#"<div class="blog-list">"#);

    for post in posts {
        let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
        let date = post
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let thumb = post
            .get("featured_image")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        html.push_str(&format!(
            r#"<article class="blog-item">
    {thumb_html}
    <div class="blog-item-content">
        <h2><a href="/blog/{slug}">{title}</a></h2>
        <time>{date}</time>
        <p>{excerpt}</p>
    </div>
</article>"#,
            slug = slug,
            title = html_escape(title),
            date = date,
            excerpt = html_escape(excerpt),
            thumb_html = if !thumb.is_empty() {
                format!(
                    r#"<div class="blog-thumb"><img src="/uploads/{}" alt="{}"></div>"#,
                    thumb,
                    html_escape(title)
                )
            } else {
                String::new()
            },
        ));
    }

    html.push_str("</div>");

    // Pagination
    if let (Some(current), Some(total)) = (
        context.get("current_page").and_then(|v| v.as_i64()),
        context.get("total_pages").and_then(|v| v.as_i64()),
    ) {
        if total > 1 {
            html.push_str(r#"<nav class="pagination">"#);
            if current > 1 {
                html.push_str(&format!(r#"<a href="/blog?page={}">&laquo; Prev</a>"#, current - 1));
            }
            for p in 1..=total {
                if p == current {
                    html.push_str(&format!(r#"<span class="current">{}</span>"#, p));
                } else {
                    html.push_str(&format!(r#"<a href="/blog?page={}">{}</a>"#, p, p));
                }
            }
            if current < total {
                html.push_str(&format!(r#"<a href="/blog?page={}">Next &raquo;</a>"#, current + 1));
            }
            html.push_str("</nav>");
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

    let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let content = post
        .get("content_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let date = post
        .get("published_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let featured = post
        .get("featured_image")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut html = format!(
        r#"<article class="blog-single">
    <h1>{title}</h1>
    <time>{date}</time>"#,
        title = html_escape(title),
        date = date,
    );

    if !featured.is_empty() {
        html.push_str(&format!(
            r#"<div class="featured-image"><img src="/uploads/{}" alt="{}"></div>"#,
            featured,
            html_escape(title)
        ));
    }

    html.push_str(&format!(r#"<div class="post-content">{}</div>"#, content));

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
        let published = post.get("published_at").and_then(|v| v.as_str()).unwrap_or("");
        let modified = post.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
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

fn render_404(_context: &Value) -> String {
    r#"<div class="error-page">
    <h1>404</h1>
    <p>Page not found.</p>
    <a href="/">← Back to home</a>
</div>"#
        .to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    const mode = document.body.dataset.clickMode || 'lightbox';
    if (mode !== 'lightbox') return;

    const links = document.querySelectorAll('.portfolio-link');
    let overlay, img, titleEl, closeBtn, prevBtn, nextBtn;
    let currentIndex = 0;
    const items = Array.from(links);

    function createOverlay() {
        overlay = document.createElement('div');
        overlay.className = 'lightbox-overlay';
        overlay.innerHTML = `
            <button class="lb-close">&times;</button>
            <button class="lb-prev">&lsaquo;</button>
            <button class="lb-next">&rsaquo;</button>
            <div class="lb-content">
                <img class="lb-image" src="" alt="">
                <div class="lb-title"></div>
            </div>
        `;
        document.body.appendChild(overlay);
        img = overlay.querySelector('.lb-image');
        titleEl = overlay.querySelector('.lb-title');
        closeBtn = overlay.querySelector('.lb-close');
        prevBtn = overlay.querySelector('.lb-prev');
        nextBtn = overlay.querySelector('.lb-next');

        closeBtn.addEventListener('click', close);
        prevBtn.addEventListener('click', () => navigate(-1));
        nextBtn.addEventListener('click', () => navigate(1));
        overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
    }

    function open(index) {
        if (!overlay) createOverlay();
        currentIndex = index;
        const link = items[index];
        const imgSrc = link.querySelector('img').src;
        const title = link.dataset.title || '';
        img.src = imgSrc;
        titleEl.textContent = title;
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

    items.forEach((link, i) => {
        link.addEventListener('click', (e) => {
            e.preventDefault();
            open(i);
        });
    });

    document.addEventListener('keydown', (e) => {
        if (!overlay || !overlay.classList.contains('active')) return;
        if (e.key === 'Escape') close();
        if (e.key === 'ArrowLeft') navigate(-1);
        if (e.key === 'ArrowRight') navigate(1);
    });
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

/* Sidebar */
.sidebar {
    width: var(--sidebar-width);
    position: fixed;
    top: 0;
    left: 0;
    height: 100vh;
    padding: 30px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    overflow-y: auto;
    border-right: 1px solid #eee;
}

.site-name {
    font-size: 24px;
    font-weight: 700;
    margin-bottom: 4px;
}

.site-tagline {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-style: italic;
    margin-bottom: 24px;
}

.category-nav {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
}

.cat-link {
    font-family: var(--font-nav);
    font-size: 13px;
    color: var(--color-text);
    text-decoration: none;
    padding: 2px 0;
}

.cat-link:hover { text-decoration: underline; }
.cat-link.active { font-weight: 700; color: var(--color-accent); }

.archives-link {
    font-family: var(--font-nav);
    font-size: 13px;
    color: var(--color-text);
    text-decoration: none;
    margin-top: 8px;
}

.sidebar-bottom {
    margin-top: auto;
}

.footer-text {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    margin-top: 16px;
}

/* Content */
.content {
    margin-left: var(--sidebar-width);
    flex: 1;
    padding: 0;
}

/* Masonry Grid */
.masonry-grid {
    column-count: var(--grid-columns);
    column-gap: var(--grid-gap);
    padding: var(--grid-gap);
}

.grid-item {
    break-inside: avoid;
    margin-bottom: var(--grid-gap);
}

.grid-item img {
    width: 100%;
    display: block;
}

.grid-item .portfolio-link {
    display: block;
    overflow: hidden;
}

.grid-item img:hover {
    opacity: 0.85;
    transition: opacity 0.2s;
}

.item-tags {
    font-family: var(--font-captions);
    font-size: 11px;
    color: var(--color-text-secondary);
    padding: 4px 0 8px;
}

.item-tags a {
    color: var(--color-text-secondary);
    text-decoration: none;
}

.item-tags a:hover { text-decoration: underline; }

/* Lightbox */
.lightbox-overlay {
    display: none;
    position: fixed;
    top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(0,0,0,0.85);
    z-index: 1000;
    justify-content: center;
    align-items: center;
}

.lightbox-overlay.active {
    display: flex;
}

.lb-content {
    text-align: center;
    max-width: 80vw;
    max-height: 80vh;
}

.lb-image {
    max-width: 80vw;
    max-height: 75vh;
    object-fit: contain;
    border: 8px solid var(--lightbox-border-color);
}

.lb-title {
    color: #fff;
    font-size: 16px;
    margin-top: 12px;
}

.lb-close {
    position: absolute;
    top: 20px; right: 20px;
    background: none; border: none;
    color: #fff; font-size: 30px;
    cursor: pointer;
}

.lb-prev, .lb-next {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: none; border: none;
    color: rgba(255,255,255,0.5);
    font-size: 40px;
    cursor: pointer;
    padding: 20px;
}

.lb-prev:hover, .lb-next:hover { color: #fff; }
.lb-prev { left: 10px; }
.lb-next { right: 10px; }

/* Blog */
.blog-list { max-width: 900px; padding: 30px; }

.blog-item {
    display: flex;
    gap: 16px;
    margin-bottom: 24px;
    padding-bottom: 24px;
    border-bottom: 1px solid #eee;
}

.blog-thumb img { width: 170px; height: 170px; object-fit: cover; }

.blog-item h2 { font-size: 18px; margin-bottom: 4px; }
.blog-item h2 a { color: var(--color-text); text-decoration: none; }
.blog-item h2 a:hover { text-decoration: underline; }
.blog-item time { font-size: 12px; color: var(--color-text-secondary); }
.blog-item p { font-size: 14px; color: var(--color-text-secondary); margin-top: 8px; }

/* Blog Single */
.blog-single { max-width: 800px; padding: 30px; }
.blog-single h1 { font-size: var(--font-size-h1); margin-bottom: 8px; }
.blog-single time { font-size: 13px; color: var(--color-text-secondary); display: block; margin-bottom: 20px; }
.featured-image img { width: 100%; margin-bottom: 24px; }
.post-content { line-height: 1.8; }

/* Portfolio Single */
.portfolio-single { max-width: 1000px; padding: 30px; }
.portfolio-image img { width: 100%; }
.portfolio-meta { display: flex; justify-content: space-between; align-items: center; margin: 16px 0; }
.portfolio-meta h1 { font-size: var(--font-size-h1); }
.like-btn { cursor: pointer; font-size: 18px; }
.portfolio-categories a { font-size: 13px; color: var(--color-text-secondary); margin-right: 8px; }

/* Comments */
.comments { margin-top: 40px; }
.comments h3 { margin-bottom: 16px; }
.comment { margin-bottom: 16px; padding-bottom: 16px; border-bottom: 1px solid #eee; }
.comment strong { font-size: 14px; }
.comment time { font-size: 12px; color: var(--color-text-secondary); margin-left: 8px; }
.comment p { margin-top: 4px; font-size: 14px; }

.comment-form { margin-top: 30px; }
.comment-form input, .comment-form textarea {
    display: block; width: 100%; max-width: 500px;
    padding: 8px 12px; margin-bottom: 12px;
    border: 1px solid #ddd; border-radius: 4px;
    font-family: inherit; font-size: 14px;
}
.comment-form textarea { min-height: 100px; resize: vertical; }
.comment-form button {
    font-family: var(--font-buttons);
    padding: 8px 24px; background: var(--color-accent);
    color: #fff; border: none; border-radius: 4px;
    cursor: pointer; font-size: 14px;
}

/* Pagination */
.pagination { display: flex; gap: 8px; padding: 20px 0; }
.pagination a, .pagination .current {
    font-family: var(--font-buttons);
    padding: 6px 12px; border: 1px solid #ddd; border-radius: 4px;
    text-decoration: none; color: var(--color-text); font-size: 13px;
}
.pagination .current { background: var(--color-accent); color: #fff; border-color: var(--color-accent); }

/* Error */
.error-page { padding: 60px 30px; text-align: center; }
.error-page h1 { font-size: 72px; color: var(--color-text-secondary); }
.error-page a { color: var(--color-accent); }

/* Responsive */
@media (max-width: 1024px) {
    .sidebar { width: 100%; height: auto; position: relative; flex-direction: row; padding: 16px 20px; }
    .content { margin-left: 0; }
    .masonry-grid { column-count: 2; }
    .sidebar-bottom { display: none; }
    .category-nav { flex-direction: row; flex-wrap: wrap; gap: 8px; }
}

@media (max-width: 768px) {
    .masonry-grid { column-count: 1; }
    .blog-item { flex-direction: column; }
    .blog-thumb img { width: 100%; height: auto; }
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
