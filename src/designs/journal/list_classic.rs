use serde_json::Value;

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
        let initials = author_initials(author);
        let hue = name_hue(author);
        html.push_str(&format!(
            "<div class=\"bsc-author-box\">\
             <div class=\"bsc-author-avatar\" style=\"background:hsl({},45%,55%)\">{}</div>\
             <div class=\"bsc-author-info\">Written by<br><strong>{}</strong></div>\
             </div>",
            hue,
            html_escape(&initials),
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

/// Build the classic-style comments section.
/// Uses letter-square avatars and a simplified form (name + comment only).
pub(crate) fn build_classic_comments(context: &Value, settings: &Value, post_id: i64) -> String {
    let mut html = String::new();

    // Render existing comments
    if let Some(Value::Array(comments)) = context.get("comments") {
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
                "<section class=\"bsc-comments\"><h3>{} Comment{}</h3>",
                comments.len(),
                if comments.len() == 1 { "" } else { "s" }
            ));
            for comment in &top {
                render_classic_comment(&mut html, comment, &replies, settings, 0);
            }
            html.push_str("</section>");
        }
    }

    // Comment form — name + comment only
    let sg = |key: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let require_name = sg("comments_require_name") != "false";
    let name_req = if require_name { " required" } else { "" };
    let require_email = sg("comments_require_email") == "true";
    let email_field = if require_email {
        "\n        <input type=\"email\" name=\"author_email\" placeholder=\"Email\" required>"
            .to_string()
    } else {
        String::new()
    };

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
        "<section class=\"bsc-comment-form\">\
\n    <h3>Leave a Reply</h3>\
\n    {captcha_script}\
\n    <form id=\"comment-form\" data-post-id=\"{post_id}\" data-content-type=\"post\">\
\n        <input type=\"hidden\" name=\"parent_id\" value=\"\">\
\n        <div id=\"reply-indicator\" style=\"display:none;margin-bottom:8px;font-size:13px;color:var(--color-text-secondary)\">\
\n            Replying to <strong id=\"reply-to-name\"></strong> <a href=\"#\" id=\"cancel-reply\" style=\"margin-left:8px\">Cancel</a>\
\n        </div>\
\n        <textarea name=\"body\" placeholder=\"Your comment...\" required></textarea>\
\n        <input type=\"text\" name=\"author_name\" placeholder=\"Name\"{name_req}>{email_field}
\n        <div style=\"display:none\"><input type=\"text\" name=\"honeypot\"></div>\
\n        {captcha_html}\
\n        <button type=\"submit\">Post Comment</button>\
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
        author_email:(f.querySelector('[name=author_email]')||{{}}).value||null,\
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
\n        .finally(function(){{btn.disabled=false;btn.textContent='Post Comment';}});\
\n    }};\
\n    if(typeof getToken==='function'){{\
\n        Promise.resolve(getToken()).then(go).catch(function(){{go(null)}});\
\n    }}else{{go(null);}}\
\n}});\
\n}})();\
\n</script>",
        captcha_script = captcha_script,
        post_id = post_id,
        name_req = name_req,
        captcha_html = captcha_html,
        captcha_get_token_js = captcha_get_token_js,
    ));

    html
}

/// Render a single comment with letter-square avatar.
pub(crate) fn render_classic_comment(
    html: &mut String,
    comment: &Value,
    all_replies: &[&Value],
    settings: &Value,
    depth: usize,
) {
    let id = comment.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let name = comment
        .get("author_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Anonymous");
    let body = comment.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let cdate_raw = comment
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cdate = format_date(cdate_raw, settings);

    let indent = if depth > 0 {
        format!(" style=\"margin-left:{}px\"", depth.min(3) * 32)
    } else {
        String::new()
    };

    let initials = author_initials(name);
    let hue = name_hue(name);
    let escaped_name = html_escape(name);
    let escaped_body = html_escape(body);

    html.push_str(&format!(
        "<div class=\"bsc-comment\"{}>\
         <div class=\"bsc-comment-avatar\" style=\"background:hsl({},45%,55%)\">{}</div>\
         <div class=\"bsc-comment-body\">\
         <div class=\"bsc-comment-header\">\
         <strong class=\"bsc-comment-name\">{}</strong>\
         <time class=\"bsc-comment-date\">{}</time>\
         </div>\
         <p>{}</p>\
         <a href=\"#\" class=\"reply-btn\" data-id=\"{}\" data-name=\"{}\">Reply</a>\
         </div>\
         </div>",
        indent,
        hue,
        html_escape(&initials),
        escaped_name,
        cdate,
        escaped_body,
        id,
        escaped_name,
    ));

    // Render child replies
    let children: Vec<&&Value> = all_replies
        .iter()
        .filter(|r| r.get("parent_id").and_then(|v| v.as_i64()) == Some(id))
        .collect();
    for child in children {
        render_classic_comment(html, child, all_replies, settings, depth + 1);
    }
}

/// Extract initials from a name: "John Smith" → "JS", "Alice" → "A".
pub(crate) fn author_initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    match parts.len() {
        0 => "?".to_string(),
        1 => parts[0]
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string(),
        _ => {
            let first = parts[0].chars().next().unwrap_or('?');
            let last = parts[parts.len() - 1].chars().next().unwrap_or('?');
            format!("{}{}", first.to_uppercase(), last.to_uppercase())
        }
    }
}

/// Derive a consistent hue (0–360) from a name string for avatar color.
pub(crate) fn name_hue(name: &str) -> u32 {
    let mut hash: u32 = 0;
    for b in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as u32);
    }
    hash % 360
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
