use serde_json::Value;

use crate::render::{format_date, html_escape};

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
