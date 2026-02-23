use std::collections::HashMap;
use std::sync::Arc;

use rocket::form::Form;
use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use crate::models::settings::SettingsCache;
use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Settings ───────────────────────────────────────────

#[get("/settings/<section>")]
pub fn settings_page(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    section: &str,
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Result<Template, Redirect> {
    // Redirect Settings > Users to the standalone Users page
    if section == "users" {
        return Err(Redirect::to(format!("{}/users", admin_base(slug))));
    }
    // Redirect old blog/portfolio URLs to the unified Pages tab
    if section == "blog" {
        return Err(Redirect::to(format!(
            "{}/settings/pages#journal",
            admin_base(slug)
        )));
    }
    if section == "portfolio" {
        return Err(Redirect::to(format!(
            "{}/settings/pages#portfolio",
            admin_base(slug)
        )));
    }

    let valid_sections = [
        "general",
        "pages",
        "comments",
        "typography",
        "images",
        "seo",
        "security",
        "visitors",
        "social",
        "commerce",
        "paypal",
        "ai",
        "email",
        "tasks",
    ];

    if !valid_sections.contains(&section) {
        return Err(Redirect::to(format!(
            "{}/settings/general",
            admin_base(slug)
        )));
    }

    let section_label = match section {
        "general" => "Site".to_string(),
        "visitors" => "Visitors".to_string(),
        "pages" => "Pages".to_string(),
        "images" => "Media".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => other.to_string(),
                Some(f) => format!("{}{}", f.to_uppercase(), &other[f.len_utf8()..]),
            }
        }
    };

    let active_design_slug = store.design_active().map(|d| d.slug).unwrap_or_default();
    let mut context = json!({
        "page_title": format!("Settings — {}", section_label),
        "section": section,
        "admin_slug": slug.get(),
        "settings": store.setting_all(),
        "current_user": _admin.user.safe_json(),
        "active_design_slug": active_design_slug,
    });

    // For security page, include passkey count
    if section == "security" {
        let pk_count = store.passkey_count_for_user(_admin.user.id);
        context["passkey_count"] = json!(pk_count);
    }

    if let Some(ref f) = flash {
        context["flash_kind"] = json!(f.kind());
        context["flash_msg"] = json!(f.message());
    }

    let template_name: String = format!("admin/settings/{}", section);
    Ok(Template::render(template_name, &context))
}

// ── POST: Settings Save ────────────────────────────────

#[post("/settings/<section>", data = "<form>")]
pub fn settings_save(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    cache: &State<SettingsCache>,
    section: &str,
    form: Form<HashMap<String, String>>,
) -> Result<Flash<Redirect>, Flash<Redirect>> {
    let mut data = form.into_inner();

    // Validation rules: (enable_key, human_name, &[required_field_keys])
    let rules: Vec<(&str, &str, Vec<&str>)> = match section {
        "security" => vec![
            (
                "security_akismet_enabled",
                "Akismet",
                vec!["security_akismet_api_key"],
            ),
            (
                "security_cleantalk_enabled",
                "CleanTalk",
                vec!["security_cleantalk_api_key"],
            ),
            (
                "security_oopspam_enabled",
                "OOPSpam",
                vec!["security_oopspam_api_key"],
            ),
            (
                "security_recaptcha_enabled",
                "reCaptcha",
                vec![
                    "security_recaptcha_site_key",
                    "security_recaptcha_secret_key",
                ],
            ),
            (
                "security_turnstile_enabled",
                "Turnstile",
                vec![
                    "security_turnstile_site_key",
                    "security_turnstile_secret_key",
                ],
            ),
            (
                "security_hcaptcha_enabled",
                "hCaptcha",
                vec!["security_hcaptcha_site_key", "security_hcaptcha_secret_key"],
            ),
        ],
        "email" => vec![
            (
                "email_gmail_enabled",
                "Gmail",
                vec!["email_gmail_address", "email_gmail_app_password"],
            ),
            (
                "email_resend_enabled",
                "Resend",
                vec!["email_resend_api_key"],
            ),
            (
                "email_ses_enabled",
                "Amazon SES",
                vec![
                    "email_ses_access_key",
                    "email_ses_secret_key",
                    "email_ses_region",
                ],
            ),
            (
                "email_postmark_enabled",
                "Postmark",
                vec!["email_postmark_server_token"],
            ),
            ("email_brevo_enabled", "Brevo", vec!["email_brevo_api_key"]),
            (
                "email_sendpulse_enabled",
                "SendPulse",
                vec!["email_sendpulse_client_id", "email_sendpulse_client_secret"],
            ),
            (
                "email_mailgun_enabled",
                "Mailgun",
                vec!["email_mailgun_api_key", "email_mailgun_domain"],
            ),
            (
                "email_moosend_enabled",
                "Moosend",
                vec!["email_moosend_api_key"],
            ),
            (
                "email_mandrill_enabled",
                "Mandrill",
                vec!["email_mandrill_api_key"],
            ),
            (
                "email_sparkpost_enabled",
                "SparkPost",
                vec!["email_sparkpost_api_key"],
            ),
            (
                "email_smtp_enabled",
                "Custom SMTP",
                vec![
                    "email_smtp_host",
                    "email_smtp_port",
                    "email_smtp_username",
                    "email_smtp_password",
                ],
            ),
        ],
        "commerce" => vec![
            (
                "commerce_paypal_enabled",
                "PayPal",
                vec!["paypal_client_id", "paypal_secret"],
            ),
            (
                "commerce_payoneer_enabled",
                "Payoneer",
                vec![
                    "payoneer_program_id",
                    "payoneer_client_id",
                    "payoneer_client_secret",
                ],
            ),
            (
                "commerce_stripe_enabled",
                "Stripe",
                vec!["stripe_publishable_key", "stripe_secret_key"],
            ),
            (
                "commerce_2checkout_enabled",
                "2Checkout",
                vec!["twocheckout_merchant_code", "twocheckout_secret_key"],
            ),
            (
                "commerce_square_enabled",
                "Square",
                vec![
                    "square_application_id",
                    "square_access_token",
                    "square_location_id",
                ],
            ),
            (
                "commerce_razorpay_enabled",
                "Razorpay",
                vec!["razorpay_key_id", "razorpay_key_secret"],
            ),
            ("commerce_mollie_enabled", "Mollie", vec!["mollie_api_key"]),
        ],
        "seo" => vec![
            (
                "seo_ga_enabled",
                "Google Analytics",
                vec!["seo_ga_measurement_id"],
            ),
            (
                "seo_plausible_enabled",
                "Plausible",
                vec!["seo_plausible_domain"],
            ),
            ("seo_fathom_enabled", "Fathom", vec!["seo_fathom_site_id"]),
            (
                "seo_matomo_enabled",
                "Matomo",
                vec!["seo_matomo_url", "seo_matomo_site_id"],
            ),
            (
                "seo_cloudflare_analytics_enabled",
                "Cloudflare Analytics",
                vec!["seo_cloudflare_analytics_token"],
            ),
            ("seo_clicky_enabled", "Clicky", vec!["seo_clicky_site_id"]),
            ("seo_umami_enabled", "Umami", vec!["seo_umami_website_id"]),
        ],
        "ai" => vec![
            (
                "ai_ollama_enabled",
                "Ollama",
                vec!["ai_ollama_url", "ai_ollama_model"],
            ),
            ("ai_openai_enabled", "OpenAI", vec!["ai_openai_api_key"]),
            ("ai_gemini_enabled", "Gemini", vec!["ai_gemini_api_key"]),
            (
                "ai_cloudflare_enabled",
                "Cloudflare Workers AI",
                vec!["ai_cloudflare_account_id", "ai_cloudflare_api_token"],
            ),
            ("ai_groq_enabled", "Groq", vec!["ai_groq_api_key"]),
        ],
        _ => vec![],
    };

    // Check validation: if enabled, all required fields must be non-empty
    let mut errors: Vec<String> = Vec::new();
    for (enable_key, name, required_fields) in &rules {
        if data.get(*enable_key).map(|v| v.as_str()) == Some("true") {
            let missing: Vec<&&str> = required_fields
                .iter()
                .filter(|f| data.get(**f).map(|v| v.trim().is_empty()).unwrap_or(true))
                .collect();
            if !missing.is_empty() {
                errors.push(format!(
                    "{}: please fill in all required fields before enabling",
                    name
                ));
            }
        }
    }

    // Always-required fields (no enable toggle)
    let required_fields: Vec<(&str, &str)> = match section {
        "general" => vec![("site_name", "Site Name"), ("site_url", "Site URL")],
        "security" => vec![("admin_slug", "Admin Slug")],
        _ => vec![],
    };
    for (key, label) in &required_fields {
        if data.get(*key).map(|v| v.trim().is_empty()).unwrap_or(true) {
            errors.push(format!("{} is required", label));
        }
    }

    // Magic Link requires at least one email provider
    if section == "security" && data.get("login_method").map(|v| v.as_str()) == Some("magic_link") {
        let email_keys = [
            "email_gmail_enabled",
            "email_resend_enabled",
            "email_ses_enabled",
            "email_postmark_enabled",
            "email_brevo_enabled",
            "email_sendpulse_enabled",
            "email_mailgun_enabled",
            "email_moosend_enabled",
            "email_mandrill_enabled",
            "email_sparkpost_enabled",
            "email_smtp_enabled",
        ];
        let any_email = email_keys
            .iter()
            .any(|k| store.setting_get_or(k, "false") == "true");
        if !any_email {
            errors.push("Magic Link login requires at least one email provider to be enabled in Email settings".to_string());
        }
    }

    // Reserved system routes that cannot be used as slugs
    const RESERVED_SLUGS: &[&str] = &[
        "static",
        "uploads",
        "api",
        "super",
        "download",
        "feed",
        "sitemap.xml",
        "robots.txt",
        "privacy",
        "terms",
        "archives",
        "login",
        "logout",
        "setup",
        "mfa",
        "magic-link",
        "forgot-password",
        "reset-password",
        "passkey",
        "passkeys",
        "img",
        "tag",
        "category",
        "search",
        "contact",
        "change-password",
    ];

    fn is_reserved(s: &str) -> bool {
        RESERVED_SLUGS.contains(&s.to_lowercase().as_str())
    }

    // Admin slug validation (security section)
    if section == "security" {
        if let Some(new_admin) = data.get("admin_slug").map(|v| v.trim().to_string()) {
            if !new_admin.is_empty() {
                if is_reserved(&new_admin) {
                    errors.push(format!(
                        "Admin Slug '{}' conflicts with a reserved system route",
                        new_admin
                    ));
                }
                let cur_blog = store.setting_get_or("blog_slug", "journal");
                let cur_portfolio = store.setting_get_or("portfolio_slug", "portfolio");
                if new_admin == cur_blog {
                    errors.push("Admin Slug cannot be the same as the Journal Slug".to_string());
                }
                if new_admin == cur_portfolio {
                    errors.push("Admin Slug cannot be the same as the Portfolio Slug".to_string());
                }
            }
        }
    }

    // Blog/Portfolio slug validation
    if section == "blog" {
        let journal_enabled = data.get("journal_enabled").map(|v| v.as_str()) == Some("true");
        let blog_slug = data.get("blog_slug").map(|v| v.trim()).unwrap_or("");
        let portfolio_enabled = store.setting_get_or("portfolio_enabled", "false") == "true";
        let portfolio_slug = store.setting_get_or("portfolio_slug", "portfolio");
        let admin_slug_val = store.setting_get_or("admin_slug", "admin");

        if journal_enabled && blog_slug.is_empty() && portfolio_enabled && portfolio_slug.is_empty()
        {
            errors.push("Journal Slug cannot be empty while Portfolio Slug is also empty — at least one must have a slug".to_string());
        }
        if journal_enabled && !blog_slug.is_empty() {
            if is_reserved(blog_slug) {
                errors.push(format!(
                    "Journal Slug '{}' conflicts with a reserved system route",
                    blog_slug
                ));
            }
            if portfolio_enabled && blog_slug == portfolio_slug {
                errors.push("Journal Slug and Portfolio Slug cannot be the same".to_string());
            }
            if blog_slug == admin_slug_val {
                errors.push("Journal Slug cannot be the same as the Admin Slug".to_string());
            }
        }
    }
    if section == "portfolio" {
        let portfolio_enabled = data.get("portfolio_enabled").map(|v| v.as_str()) == Some("true");
        let portfolio_slug = data.get("portfolio_slug").map(|v| v.trim()).unwrap_or("");
        let journal_enabled = store.setting_get_or("journal_enabled", "true") == "true";
        let blog_slug = store.setting_get_or("blog_slug", "journal");
        let admin_slug_val = store.setting_get_or("admin_slug", "admin");

        if portfolio_enabled && portfolio_slug.is_empty() && journal_enabled && blog_slug.is_empty()
        {
            errors.push("Portfolio Slug cannot be empty while Journal Slug is also empty — at least one must have a slug".to_string());
        }
        if portfolio_enabled && !portfolio_slug.is_empty() {
            if is_reserved(portfolio_slug) {
                errors.push(format!(
                    "Portfolio Slug '{}' conflicts with a reserved system route",
                    portfolio_slug
                ));
            }
            if journal_enabled && portfolio_slug == blog_slug {
                errors.push("Portfolio Slug and Journal Slug cannot be the same".to_string());
            }
            if portfolio_slug == admin_slug_val {
                errors.push("Portfolio Slug cannot be the same as the Admin Slug".to_string());
            }
        }
    }

    if !errors.is_empty() {
        let tab_frag = data
            .get("_tab")
            .filter(|t| !t.is_empty())
            .map(|t| format!("#{}", t))
            .unwrap_or_default();
        let err_section = match section {
            "blog" | "portfolio" | "contact" => "pages",
            _ => section,
        };
        return Err(Flash::error(
            Redirect::to(format!(
                "{}/settings/{}{}",
                admin_base(slug),
                err_section,
                tab_frag
            )),
            errors.join(" | "),
        ));
    }

    // Checkboxes don't submit a value when unchecked, so we must
    // explicitly reset all known boolean keys for this section first.
    let checkbox_keys: &[&str] = match section {
        "ai" => &[
            "ai_ollama_enabled",
            "ai_openai_enabled",
            "ai_gemini_enabled",
            "ai_cloudflare_enabled",
            "ai_groq_enabled",
            "ai_suggest_meta",
            "ai_suggest_tags",
            "ai_suggest_categories",
            "ai_suggest_alt_text",
            "ai_suggest_slug",
            "ai_theme_generation",
            "ai_post_generation",
        ],
        "email" => &[
            "email_failover_enabled",
            "email_gmail_enabled",
            "email_resend_enabled",
            "email_ses_enabled",
            "email_postmark_enabled",
            "email_brevo_enabled",
            "email_sendpulse_enabled",
            "email_mailgun_enabled",
            "email_moosend_enabled",
            "email_mandrill_enabled",
            "email_sparkpost_enabled",
            "email_smtp_enabled",
            "email_builtin_enabled",
        ],
        "blog" => &[
            "journal_enabled",
            "blog_show_author",
            "blog_show_date",
            "blog_show_reading_time",
            "blog_featured_image_required",
        ],
        "portfolio" => &[
            "portfolio_enabled",
            "portfolio_enable_likes",
            "portfolio_image_protection",
        ],
        "contact" => &["contact_page_enabled", "contact_form_enabled"],
        "comments" => &[
            "comments_enabled",
            "comments_on_blog",
            "comments_on_portfolio",
            "comments_honeypot",
            "comments_require_name",
            "comments_require_email",
        ],
        "security" => &[
            "mfa_enabled",
            "login_captcha_enabled",
            "security_akismet_enabled",
            "security_cleantalk_enabled",
            "security_oopspam_enabled",
            "security_recaptcha_enabled",
            "security_turnstile_enabled",
            "security_hcaptcha_enabled",
        ],
        "commerce" => &[
            "commerce_paypal_enabled",
            "commerce_payoneer_enabled",
            "commerce_stripe_enabled",
            "commerce_2checkout_enabled",
            "commerce_square_enabled",
            "commerce_razorpay_enabled",
            "commerce_mollie_enabled",
        ],
        "seo" => &[
            "seo_sitemap_enabled",
            "seo_structured_data",
            "seo_open_graph",
            "seo_twitter_cards",
            "seo_ga_enabled",
            "seo_plausible_enabled",
            "seo_fathom_enabled",
            "seo_matomo_enabled",
            "seo_cloudflare_analytics_enabled",
            "seo_clicky_enabled",
            "seo_umami_enabled",
        ],
        "images" => &[
            "images_webp_convert",
            "images_reencode",
            "images_strip_metadata",
            "video_upload_enabled",
        ],
        "typography" => &["font_google_enabled", "font_adobe_enabled", "font_sitewide"],
        "visitors" => &[
            "design_site_search",
            "design_back_to_top",
            "design_powered_by",
            "cookie_consent_enabled",
            "cookie_consent_show_reject",
            "privacy_policy_enabled",
            "terms_of_use_enabled",
        ],
        "social" => &["social_brand_colors", "share_enabled"],
        _ => &[],
    };
    for key in checkbox_keys {
        let _ = store.setting_set(key, "false");
    }

    // Sanitize SVG data URLs in branding fields (logo, favicon)
    if section == "general" {
        for key in &["site_logo", "site_favicon"] {
            if let Some(val) = data.get(*key) {
                if let Some(sanitized) = sanitize_svg_data_url(val) {
                    data.insert(key.to_string(), sanitized);
                }
            }
        }
    }

    // Server-side clamp for video settings
    if section == "images" {
        if let Some(val) = data.get("video_max_duration") {
            let n: i64 = val.parse().unwrap_or(1800);
            let clamped = n.clamp(1, 1800);
            data.insert("video_max_duration".to_string(), clamped.to_string());
        }
        if let Some(val) = data.get("video_max_upload_mb") {
            let n: i64 = val.parse().unwrap_or(100);
            let clamped = n.clamp(1, 2048);
            data.insert("video_max_upload_mb".to_string(), clamped.to_string());
        }
    }

    // If environment switched to production, auto-generate deploy receive key
    if section == "general" {
        if let Some(env) = data.get("site_environment") {
            if env == "production" {
                let existing = store.setting_get("deploy_receive_key").unwrap_or_default();
                if existing.is_empty() {
                    use rand::Rng;
                    let mut rng = rand::thread_rng();
                    let bytes: [u8; 32] = rng.gen();
                    data.insert("deploy_receive_key".to_string(), hex::encode(bytes));
                }
            }
        }
    }

    let _ = store.setting_set_many(&data);

    // If email settings changed and no providers remain enabled, revert magic link to password
    if section == "email" {
        let email_keys = [
            "email_gmail_enabled",
            "email_resend_enabled",
            "email_ses_enabled",
            "email_postmark_enabled",
            "email_brevo_enabled",
            "email_sendpulse_enabled",
            "email_mailgun_enabled",
            "email_moosend_enabled",
            "email_mandrill_enabled",
            "email_sparkpost_enabled",
            "email_smtp_enabled",
        ];
        let any_email = email_keys
            .iter()
            .any(|k| store.setting_get_or(k, "false") == "true");
        if !any_email && store.setting_get_or("login_method", "password") == "magic_link" {
            let _ = store.setting_set("login_method", "password");
        }
    }

    let tab_fragment = data
        .get("_tab")
        .filter(|t| !t.is_empty())
        .map(|t| format!("#{}", t))
        .unwrap_or_default();

    // Refresh in-memory settings cache so dynamic routing picks up changes immediately
    let s: &dyn Store = &**store.inner();
    cache.refresh_from_store(s);

    // If admin_slug changed, update the RwLock so the rewriter fairing uses the new slug
    if section == "security" {
        let new_slug = s.setting_get_or("admin_slug", "admin");
        slug.set(&new_slug);
    }

    store.audit_log(
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "settings_change",
        Some("settings"),
        None,
        Some(section),
        None,
        None,
    );

    // blog/portfolio sections now live under the unified "pages" tab
    let redirect_section = match section {
        "blog" | "portfolio" | "contact" => "pages",
        _ => section,
    };

    Ok(Flash::success(
        Redirect::to(format!(
            "{}/settings/{}{}",
            admin_base(slug),
            redirect_section,
            tab_fragment
        )),
        "Settings saved successfully",
    ))
}

/// If the value is an SVG data URL, decode → sanitize → re-encode.
/// Returns Some(sanitized_data_url) if it was an SVG, None otherwise (leave as-is).
fn sanitize_svg_data_url(value: &str) -> Option<String> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;

    // Check for base64-encoded SVG data URL
    if let Some(b64) = value.strip_prefix("data:image/svg+xml;base64,") {
        if let Ok(raw) = engine.decode(b64.trim()) {
            if let Some(clean) = crate::svg_sanitizer::sanitize_svg(&raw) {
                let encoded = engine.encode(&clean);
                return Some(format!("data:image/svg+xml;base64,{}", encoded));
            }
        }
    }
    None
}
