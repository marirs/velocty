use crate::render::html_escape;

/// Build the contact page body HTML (everything inside `{{body_content}}`).
/// Returns `(body_html, contact_css)`.
pub fn render_body(
    settings: &std::collections::HashMap<String, String>,
    social_links_inline: &str,
    flash: Option<(&str, &str)>,
) -> (String, String) {
    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .cloned()
            .unwrap_or_else(|| def.to_string())
    };

    let contact_label = sg("contact_label", "Contact");
    let layout = sg("contact_layout", "modern");
    let alignment = sg("contact_alignment", "left");
    let name = sg("contact_name", "");
    let address = sg("contact_address", "");
    let phone = sg("contact_phone", "");
    let email = sg("contact_email", "");
    let email = if email.is_empty() {
        sg("admin_email", "")
    } else {
        email
    };
    let text = sg("contact_text", "");
    let photo = sg("contact_photo", "");
    let form_enabled = sg("contact_form_enabled", "true") == "true";

    // Block-level alignment: positions the entire contact block within the page
    let align_margin = match alignment.as_str() {
        "center" => "margin:0 auto;",
        "right" => "margin-left:auto;margin-right:0;",
        _ => "margin-right:auto;margin-left:0;",
    };

    // Build info block
    let mut info_html = String::new();
    if !name.is_empty() {
        info_html.push_str(&format!(
            "<p class=\"contact-name\" style=\"font-size:1.15em;font-weight:700;margin-bottom:8px\">{}</p>",
            html_escape(&name)
        ));
    }
    if !text.is_empty() {
        info_html.push_str(&format!(
            "<p class=\"contact-text\">{}</p>",
            html_escape(&text).replace('\n', "<br>")
        ));
    }
    if !address.is_empty() {
        info_html.push_str(&format!(
            "<p class=\"contact-address\">{}</p>",
            html_escape(&address).replace('\n', "<br>")
        ));
    }
    let mut details = String::new();
    if !phone.is_empty() {
        details.push_str(&format!(
            "<div class=\"contact-detail\"><strong>Phone:</strong> {}</div>",
            html_escape(&phone)
        ));
    }
    if !email.is_empty() {
        details.push_str(&format!(
            "<div class=\"contact-detail\"><strong>Email:</strong> <a href=\"mailto:{}\">{}</a></div>",
            html_escape(&email),
            html_escape(&email)
        ));
    }
    if !details.is_empty() {
        info_html.push_str(&format!("<div class=\"contact-details\">{}</div>", details));
    }

    // Social links
    if !social_links_inline.is_empty() {
        info_html.push_str(&format!(
            "<div class=\"contact-social\" style=\"display:flex;gap:12px;margin-top:16px\">{}</div>",
            social_links_inline
        ));
    }

    // Build form block
    let form_html = if form_enabled {
        let flash_html = match flash {
            Some(("success", msg)) => format!(
                "<div class=\"contact-flash contact-flash-success\" style=\"padding:12px;margin-bottom:16px;border-radius:6px;background:rgba(34,197,94,.12);color:#16a34a;font-size:14px\">{}</div>",
                html_escape(msg)
            ),
            Some(("error", msg)) => format!(
                "<div class=\"contact-flash contact-flash-error\" style=\"padding:12px;margin-bottom:16px;border-radius:6px;background:rgba(239,68,68,.12);color:#ef4444;font-size:14px\">{}</div>",
                html_escape(msg)
            ),
            _ => String::new(),
        };
        format!(
            r#"{flash_html}<form method="post" action="/contact" class="contact-form">
<div class="contact-form-group"><label for="cf-name">Name <span style="color:#999">(required)</span></label><input type="text" id="cf-name" name="name" required placeholder="Your name"></div>
<div class="contact-form-group"><label for="cf-email">Email <span style="color:#999">(required)</span></label><input type="email" id="cf-email" name="email" required placeholder="your@email.com"></div>
<div class="contact-form-group"><label for="cf-message">Message <span style="color:#999">(required)</span></label><textarea id="cf-message" name="message" rows="6" required placeholder="Your message…"></textarea></div>
<div style="display:none"><input type="text" name="_honey" tabindex="-1" autocomplete="off"></div>
<button type="submit" class="contact-submit">Send Message</button>
</form>"#,
            flash_html = flash_html
        )
    } else {
        String::new()
    };

    // Photo HTML
    let photo_html = if !photo.is_empty() {
        format!(
            "<img src=\"/uploads/{}\" alt=\"{}\" class=\"contact-photo\">",
            html_escape(&photo),
            html_escape(&name)
        )
    } else {
        String::new()
    };

    // Title
    let title_html = format!(
        "<h1 class=\"contact-title\">{}</h1>",
        html_escape(&contact_label)
    );

    // Build body based on layout
    let body = match layout.as_str() {
        "compact" => render_compact(
            &title_html,
            &info_html,
            &form_html,
            form_enabled,
            align_margin,
        ),
        "wide" => render_wide(
            &title_html,
            &info_html,
            &form_html,
            &photo,
            &name,
            form_enabled,
            align_margin,
        ),
        "modern" => render_modern(
            &title_html,
            &info_html,
            &form_html,
            &photo_html,
            form_enabled,
            align_margin,
        ),
        _ => render_split(
            &title_html,
            &info_html,
            &form_html,
            form_enabled,
            align_margin,
        ),
    };

    (body, css().to_string())
}

fn render_compact(title: &str, info: &str, form: &str, form_enabled: bool, align: &str) -> String {
    let separator = if form_enabled && !info.is_empty() {
        "<hr class=\"contact-divider\">"
    } else {
        ""
    };
    format!(
        "<div class=\"contact-page contact-compact\">\
        <div class=\"contact-inner\" style=\"max-width:640px;{align}padding:40px 20px\">\
        {title}{info}{sep}{form}\
        </div></div>",
        title = title,
        align = align,
        info = info,
        sep = separator,
        form = form,
    )
}

fn render_wide(
    title: &str,
    info: &str,
    form: &str,
    photo: &str,
    name: &str,
    form_enabled: bool,
    align: &str,
) -> String {
    let hero = if !photo.is_empty() {
        format!(
            "<div class=\"contact-hero\" style=\"width:100%;max-height:500px;overflow:hidden\">\
            <img src=\"/uploads/{}\" alt=\"{}\" style=\"width:100%;height:auto;display:block;object-fit:cover;max-height:500px\">\
            </div>",
            html_escape(photo),
            html_escape(name)
        )
    } else {
        String::new()
    };
    let separator = if form_enabled && !info.is_empty() {
        "<hr class=\"contact-divider\">"
    } else {
        ""
    };
    format!(
        "<div class=\"contact-page contact-wide\">\
        {hero}\
        <div class=\"contact-inner\" style=\"max-width:640px;{align}padding:40px 20px\">\
        {title}{info}{sep}{form}\
        </div></div>",
        hero = hero,
        title = title,
        align = align,
        info = info,
        sep = separator,
        form = form,
    )
}

fn render_modern(
    title: &str,
    info: &str,
    form: &str,
    photo_html: &str,
    form_enabled: bool,
    align: &str,
) -> String {
    let left = format!(
        "<div class=\"contact-col-left\" style=\"flex:1;min-width:280px\">{photo}{info}</div>",
        photo = if !photo_html.is_empty() {
            format!(
                "<div class=\"contact-photo-wrap\" style=\"margin-bottom:20px\">{}</div>",
                photo_html
            )
        } else {
            String::new()
        },
        info = info,
    );
    let right = if form_enabled {
        format!(
            "<div class=\"contact-col-right\" style=\"flex:1;min-width:280px\">{}</div>",
            form
        )
    } else {
        String::new()
    };
    format!(
        "<div class=\"contact-page contact-modern\">\
        <div class=\"contact-inner\" style=\"max-width:900px;{align}padding:40px 20px\">\
        {title}\
        <div class=\"contact-columns\" style=\"display:flex;gap:40px;margin-top:24px;flex-wrap:wrap\">\
        {left}{right}\
        </div></div></div>",
        title = title,
        align = align,
        left = left,
        right = right,
    )
}

fn render_split(title: &str, info: &str, form: &str, form_enabled: bool, align: &str) -> String {
    let left = format!(
        "<div class=\"contact-col-left\" style=\"flex:1;min-width:280px\">{title}{info}</div>",
        title = title,
        info = info,
    );
    let right = if form_enabled {
        format!(
            "<div class=\"contact-col-right\" style=\"flex:1;min-width:280px\">{}</div>",
            form
        )
    } else {
        String::new()
    };
    format!(
        "<div class=\"contact-page contact-split\">\
        <div class=\"contact-inner\" style=\"max-width:900px;{align}padding:40px 20px\">\
        <div class=\"contact-columns\" style=\"display:flex;gap:40px;flex-wrap:wrap\">\
        {left}{right}\
        </div></div></div>",
        left = left,
        align = align,
        right = right,
    )
}

pub fn css() -> &'static str {
    r#"<style>
.contact-title { font-size:2em; margin-bottom:24px; }
.contact-text { font-size:1em; line-height:1.7; margin-bottom:16px; color:inherit; }
.contact-address { font-size:.95em; line-height:1.6; margin-bottom:12px; }
.contact-details { margin-bottom:16px; }
.contact-detail { font-size:.95em; line-height:1.8; }
.contact-detail a { color:inherit; text-decoration:underline; }
.contact-divider { border:none; border-top:1px solid rgba(128,128,128,.2); margin:32px 0; }
.contact-photo { width:100%; max-width:400px; height:auto; display:block; }
.contact-form-group { margin-bottom:16px; }
.contact-form-group label { display:block; font-size:.9em; font-weight:600; margin-bottom:6px; }
.contact-form-group input,
.contact-form-group textarea {
    width:100%; padding:10px 12px; font-size:.95em; border:1px solid rgba(128,128,128,.3);
    background:transparent; color:inherit; border-radius:4px; font-family:inherit;
    box-sizing:border-box;
}
.contact-form-group textarea { resize:vertical; }
.contact-submit {
    display:inline-block; padding:12px 28px; font-size:.95em; font-weight:600;
    background:#222; color:#fff; border:none; border-radius:4px; cursor:pointer;
    font-family:inherit; transition:background .15s;
}
.contact-submit:hover { background:#444; }
.contact-social a { opacity:.7; transition:opacity .15s; }
.contact-social a:hover { opacity:1; }
@media (max-width:640px) {
    .contact-columns { flex-direction:column !important; }
}
</style>"#
}
