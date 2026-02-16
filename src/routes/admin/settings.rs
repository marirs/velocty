use rocket::form::Form;
use rocket::response::{Flash, Redirect};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;
use std::collections::HashMap;

use crate::security::auth::AdminUser;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::settings::{Setting, SettingsCache};
use crate::AdminSlug;
use super::admin_base;

// ── Settings ───────────────────────────────────────────

#[get("/settings/<section>")]
pub fn settings_page(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    section: &str,
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Result<Template, Redirect> {
    // Redirect Settings > Users to the standalone Users page
    if section == "users" {
        return Err(Redirect::to(format!("{}/users", admin_base(slug))));
    }

    let valid_sections = [
        "general", "blog", "portfolio", "comments", "typography", "images", "seo", "security",
        "design", "social", "commerce", "paypal", "ai", "email", "tasks",
    ];

    if !valid_sections.contains(&section) {
        return Err(Redirect::to(format!("{}/settings/general", admin_base(slug))));
    }

    let section_label = match section {
        "general" => "Site".to_string(),
        "design" => "Visitors".to_string(),
        "blog" => "Journal".to_string(),
        "images" => "Media".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => other.to_string(),
                Some(f) => format!("{}{}", f.to_uppercase(), &other[f.len_utf8()..]),
            }
        }
    };

    let mut context = json!({
        "page_title": format!("Settings — {}", section_label),
        "section": section,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

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
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    cache: &State<SettingsCache>,
    section: &str,
    form: Form<HashMap<String, String>>,
) -> Result<Flash<Redirect>, Flash<Redirect>> {
    let data = form.into_inner();

    // Validation rules: (enable_key, human_name, &[required_field_keys])
    let rules: Vec<(&str, &str, Vec<&str>)> = match section {
        "security" => vec![
            ("security_akismet_enabled", "Akismet", vec!["security_akismet_api_key"]),
            ("security_cleantalk_enabled", "CleanTalk", vec!["security_cleantalk_api_key"]),
            ("security_oopspam_enabled", "OOPSpam", vec!["security_oopspam_api_key"]),
            ("security_recaptcha_enabled", "reCaptcha", vec!["security_recaptcha_site_key", "security_recaptcha_secret_key"]),
            ("security_turnstile_enabled", "Turnstile", vec!["security_turnstile_site_key", "security_turnstile_secret_key"]),
            ("security_hcaptcha_enabled", "hCaptcha", vec!["security_hcaptcha_site_key", "security_hcaptcha_secret_key"]),
        ],
        "email" => vec![
            ("email_gmail_enabled", "Gmail", vec!["email_gmail_address", "email_gmail_app_password"]),
            ("email_resend_enabled", "Resend", vec!["email_resend_api_key"]),
            ("email_ses_enabled", "Amazon SES", vec!["email_ses_access_key", "email_ses_secret_key", "email_ses_region"]),
            ("email_postmark_enabled", "Postmark", vec!["email_postmark_server_token"]),
            ("email_brevo_enabled", "Brevo", vec!["email_brevo_api_key"]),
            ("email_sendpulse_enabled", "SendPulse", vec!["email_sendpulse_client_id", "email_sendpulse_client_secret"]),
            ("email_mailgun_enabled", "Mailgun", vec!["email_mailgun_api_key", "email_mailgun_domain"]),
            ("email_moosend_enabled", "Moosend", vec!["email_moosend_api_key"]),
            ("email_mandrill_enabled", "Mandrill", vec!["email_mandrill_api_key"]),
            ("email_sparkpost_enabled", "SparkPost", vec!["email_sparkpost_api_key"]),
            ("email_smtp_enabled", "Custom SMTP", vec!["email_smtp_host", "email_smtp_port", "email_smtp_username", "email_smtp_password"]),
        ],
        "commerce" => vec![
            ("commerce_paypal_enabled", "PayPal", vec!["paypal_client_id", "paypal_secret"]),
            ("commerce_payoneer_enabled", "Payoneer", vec!["payoneer_program_id", "payoneer_client_id", "payoneer_client_secret"]),
            ("commerce_stripe_enabled", "Stripe", vec!["stripe_publishable_key", "stripe_secret_key"]),
            ("commerce_2checkout_enabled", "2Checkout", vec!["twocheckout_merchant_code", "twocheckout_secret_key"]),
            ("commerce_square_enabled", "Square", vec!["square_application_id", "square_access_token", "square_location_id"]),
            ("commerce_razorpay_enabled", "Razorpay", vec!["razorpay_key_id", "razorpay_key_secret"]),
            ("commerce_mollie_enabled", "Mollie", vec!["mollie_api_key"]),
        ],
        "seo" => vec![
            ("seo_ga_enabled", "Google Analytics", vec!["seo_ga_measurement_id"]),
            ("seo_plausible_enabled", "Plausible", vec!["seo_plausible_domain"]),
            ("seo_fathom_enabled", "Fathom", vec!["seo_fathom_site_id"]),
            ("seo_matomo_enabled", "Matomo", vec!["seo_matomo_url", "seo_matomo_site_id"]),
            ("seo_cloudflare_analytics_enabled", "Cloudflare Analytics", vec!["seo_cloudflare_analytics_token"]),
            ("seo_clicky_enabled", "Clicky", vec!["seo_clicky_site_id"]),
            ("seo_umami_enabled", "Umami", vec!["seo_umami_website_id"]),
        ],
        "ai" => vec![
            ("ai_ollama_enabled", "Ollama", vec!["ai_ollama_url", "ai_ollama_model"]),
            ("ai_openai_enabled", "OpenAI", vec!["ai_openai_api_key"]),
            ("ai_gemini_enabled", "Gemini", vec!["ai_gemini_api_key"]),
            ("ai_cloudflare_enabled", "Cloudflare Workers AI", vec!["ai_cloudflare_account_id", "ai_cloudflare_api_token"]),
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
                errors.push(format!("{}: please fill in all required fields before enabling", name));
            }
        }
    }

    // Always-required fields (no enable toggle)
    let required_fields: Vec<(&str, &str)> = match section {
        "general" => vec![
            ("site_name", "Site Name"),
            ("site_url", "Site URL"),
        ],
        "security" => vec![
            ("admin_slug", "Admin Slug"),
        ],
        _ => vec![],
    };
    for (key, label) in &required_fields {
        if data.get(*key).map(|v| v.trim().is_empty()).unwrap_or(true) {
            errors.push(format!("{} is required", label));
        }
    }

    // Magic Link requires at least one email provider
    if section == "security" {
        if data.get("login_method").map(|v| v.as_str()) == Some("magic_link") {
            let email_keys = [
                "email_gmail_enabled", "email_resend_enabled", "email_ses_enabled",
                "email_postmark_enabled", "email_brevo_enabled", "email_sendpulse_enabled",
                "email_mailgun_enabled", "email_moosend_enabled", "email_mandrill_enabled",
                "email_sparkpost_enabled", "email_smtp_enabled",
            ];
            let any_email = email_keys.iter().any(|k| Setting::get_or(pool, k, "false") == "true");
            if !any_email {
                errors.push("Magic Link login requires at least one email provider to be enabled in Email settings".to_string());
            }
        }
    }

    // Reserved system routes that cannot be used as slugs
    const RESERVED_SLUGS: &[&str] = &[
        "static", "uploads", "api", "super", "download", "feed",
        "sitemap.xml", "robots.txt", "privacy", "terms", "archives",
        "login", "logout", "setup", "mfa", "magic-link",
        "forgot-password", "reset-password",
    ];

    fn is_reserved(s: &str) -> bool {
        RESERVED_SLUGS.contains(&s.to_lowercase().as_str())
    }

    // Admin slug validation (security section)
    if section == "security" {
        if let Some(new_admin) = data.get("admin_slug").map(|v| v.trim().to_string()) {
            if !new_admin.is_empty() {
                if is_reserved(&new_admin) {
                    errors.push(format!("Admin Slug '{}' conflicts with a reserved system route", new_admin));
                }
                let cur_blog = Setting::get_or(pool, "blog_slug", "journal");
                let cur_portfolio = Setting::get_or(pool, "portfolio_slug", "portfolio");
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
        let portfolio_enabled = Setting::get_or(pool, "portfolio_enabled", "false") == "true";
        let portfolio_slug = Setting::get_or(pool, "portfolio_slug", "portfolio");
        let admin_slug_val = Setting::get_or(pool, "admin_slug", "admin");

        if journal_enabled && blog_slug.is_empty() && portfolio_enabled && portfolio_slug.is_empty() {
            errors.push("Journal Slug cannot be empty while Portfolio Slug is also empty — at least one must have a slug".to_string());
        }
        if journal_enabled && !blog_slug.is_empty() {
            if is_reserved(blog_slug) {
                errors.push(format!("Journal Slug '{}' conflicts with a reserved system route", blog_slug));
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
        let journal_enabled = Setting::get_or(pool, "journal_enabled", "true") == "true";
        let blog_slug = Setting::get_or(pool, "blog_slug", "journal");
        let admin_slug_val = Setting::get_or(pool, "admin_slug", "admin");

        if portfolio_enabled && portfolio_slug.is_empty() && journal_enabled && blog_slug.is_empty() {
            errors.push("Portfolio Slug cannot be empty while Journal Slug is also empty — at least one must have a slug".to_string());
        }
        if portfolio_enabled && !portfolio_slug.is_empty() {
            if is_reserved(portfolio_slug) {
                errors.push(format!("Portfolio Slug '{}' conflicts with a reserved system route", portfolio_slug));
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
        let tab_frag = data.get("_tab")
            .filter(|t| !t.is_empty())
            .map(|t| format!("#{}", t))
            .unwrap_or_default();
        return Err(Flash::error(
            Redirect::to(format!("{}/settings/{}{}", admin_base(slug), section, tab_frag)),
            errors.join(" | "),
        ));
    }

    // Checkboxes don't submit a value when unchecked, so we must
    // explicitly reset all known boolean keys for this section first.
    let checkbox_keys: &[&str] = match section {
        "ai" => &[
            "ai_ollama_enabled", "ai_openai_enabled",
            "ai_gemini_enabled", "ai_cloudflare_enabled", "ai_groq_enabled",
            "ai_suggest_meta", "ai_suggest_tags", "ai_suggest_categories",
            "ai_suggest_alt_text", "ai_suggest_slug", "ai_theme_generation",
            "ai_post_generation",
        ],
        "email" => &[
            "email_failover_enabled",
            "email_gmail_enabled", "email_resend_enabled", "email_ses_enabled",
            "email_postmark_enabled", "email_brevo_enabled", "email_sendpulse_enabled",
            "email_mailgun_enabled", "email_moosend_enabled", "email_mandrill_enabled",
            "email_sparkpost_enabled", "email_smtp_enabled",
        ],
        "blog" => &[
            "journal_enabled",
            "blog_show_author", "blog_show_date", "blog_show_reading_time",
            "blog_featured_image_required",
        ],
        "portfolio" => &[
            "portfolio_enabled", "portfolio_enable_likes",
            "portfolio_image_protection", "portfolio_fade_animation",
            "portfolio_show_categories", "portfolio_show_tags",
            "portfolio_lightbox_show_title", "portfolio_lightbox_show_tags",
            "portfolio_lightbox_nav", "portfolio_lightbox_keyboard",
        ],
        "comments" => &[
            "comments_enabled", "comments_on_blog", "comments_on_portfolio",
            "comments_honeypot", "comments_require_name", "comments_require_email",
        ],
        "security" => &[
            "mfa_enabled", "login_captcha_enabled",
            "security_akismet_enabled", "security_cleantalk_enabled",
            "security_oopspam_enabled", "security_recaptcha_enabled",
            "security_turnstile_enabled", "security_hcaptcha_enabled",
        ],
        "commerce" => &[
            "commerce_paypal_enabled", "commerce_payoneer_enabled",
            "commerce_stripe_enabled", "commerce_2checkout_enabled",
            "commerce_square_enabled", "commerce_razorpay_enabled",
            "commerce_mollie_enabled",
        ],
        "seo" => &[
            "seo_sitemap_enabled", "seo_structured_data", "seo_open_graph", "seo_twitter_cards",
            "seo_ga_enabled", "seo_plausible_enabled", "seo_fathom_enabled",
            "seo_matomo_enabled", "seo_cloudflare_analytics_enabled",
            "seo_clicky_enabled", "seo_umami_enabled",
        ],
        "images" => &[
            "images_webp_convert", "video_upload_enabled", "video_generate_thumbnail",
        ],
        "typography" => &["font_google_enabled", "font_adobe_enabled", "font_sitewide"],
        "design" => &[
            "design_site_search", "design_back_to_top",
            "cookie_consent_enabled", "cookie_consent_show_reject",
            "privacy_policy_enabled", "terms_of_use_enabled",
        ],
        "social" => &[
            "social_brand_colors",
            "share_enabled", "share_facebook", "share_x", "share_linkedin",
        ],
        _ => &[],
    };
    for key in checkbox_keys {
        let _ = Setting::set(pool, key, "false");
    }

    let _ = Setting::set_many(pool, &data);

    // If email settings changed and no providers remain enabled, revert magic link to password
    if section == "email" {
        let email_keys = [
            "email_gmail_enabled", "email_resend_enabled", "email_ses_enabled",
            "email_postmark_enabled", "email_brevo_enabled", "email_sendpulse_enabled",
            "email_mailgun_enabled", "email_moosend_enabled", "email_mandrill_enabled",
            "email_sparkpost_enabled", "email_smtp_enabled",
        ];
        let any_email = email_keys.iter().any(|k| Setting::get_or(pool, k, "false") == "true");
        if !any_email && Setting::get_or(pool, "login_method", "password") == "magic_link" {
            let _ = Setting::set(pool, "login_method", "password");
        }
    }

    let tab_fragment = data.get("_tab")
        .filter(|t| !t.is_empty())
        .map(|t| format!("#{}", t))
        .unwrap_or_default();

    // Refresh in-memory settings cache so dynamic routing picks up changes immediately
    cache.refresh(pool);

    AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "settings_change", Some("settings"), None, Some(section), None, None);

    Ok(Flash::success(
        Redirect::to(format!("{}/settings/{}{}", admin_base(slug), section, tab_fragment)),
        "Settings saved successfully",
    ))
}
