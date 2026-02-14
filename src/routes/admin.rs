use rocket::data::{Data, ToByteUnit};
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::{Flash, Redirect};
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::auth::AdminUser;
use crate::db::DbPool;
use crate::models::category::{Category, CategoryForm};
use crate::models::comment::Comment;
use crate::models::design::Design;
use crate::models::import::Import;
use crate::models::portfolio::{PortfolioForm, PortfolioItem};
use crate::models::post::{Post, PostForm};
use crate::models::settings::Setting;
use crate::models::tag::Tag;

// ── Dashboard ──────────────────────────────────────────

#[get("/")]
pub fn dashboard(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let posts_count = Post::count(pool, None);
    let posts_draft = Post::count(pool, Some("draft"));
    let portfolio_count = PortfolioItem::count(pool, None);
    let comments_pending = Comment::count(pool, Some("pending"));

    let context = json!({
        "page_title": "Dashboard",
        "posts_count": posts_count,
        "posts_draft": posts_draft,
        "portfolio_count": portfolio_count,
        "comments_pending": comments_pending,
        "settings": Setting::all(pool),
    });

    Template::render("admin/dashboard", &context)
}

// ── Posts ───────────────────────────────────────────────

#[get("/posts?<status>&<page>")]
pub fn posts_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let posts = Post::list(pool, status.as_deref(), per_page, offset);
    let total = Post::count(pool, status.as_deref());
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "page_title": "Journal",
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": Post::count(pool, None),
        "count_published": Post::count(pool, Some("published")),
        "count_draft": Post::count(pool, Some("draft")),
        "count_archived": Post::count(pool, Some("archived")),
        "settings": Setting::all(pool),
    });

    Template::render("admin/posts/list", &context)
}

#[get("/posts/new")]
pub fn posts_new(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let categories = Category::list(pool, Some("post"));
    let tags = Tag::list(pool);

    let context = json!({
        "page_title": "New Post",
        "categories": categories,
        "tags": tags,
        "settings": Setting::all(pool),
    });

    Template::render("admin/posts/edit", &context)
}

#[get("/posts/<id>/edit")]
pub fn posts_edit(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Option<Template> {
    let post = Post::find_by_id(pool, id)?;
    let categories = Category::list(pool, Some("post"));
    let tags = Tag::list(pool);
    let post_categories = Category::for_content(pool, id, "post");
    let post_tags = Tag::for_content(pool, id, "post");

    let context = json!({
        "page_title": "Edit Post",
        "post": post,
        "categories": categories,
        "tags": tags,
        "post_categories": post_categories.iter().map(|c| c.id).collect::<Vec<_>>(),
        "post_tags": post_tags.iter().map(|t| t.id).collect::<Vec<_>>(),
        "settings": Setting::all(pool),
    });

    Some(Template::render("admin/posts/edit", &context))
}

#[post("/posts/<id>/delete")]
pub fn posts_delete(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Post::delete(pool, id);
    Redirect::to("/admin/posts")
}

// ── Portfolio ──────────────────────────────────────────

#[get("/portfolio?<status>&<page>")]
pub fn portfolio_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let items = PortfolioItem::list(pool, status.as_deref(), per_page, offset);
    let total = PortfolioItem::count(pool, status.as_deref());
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "page_title": "Portfolio",
        "items": items,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": PortfolioItem::count(pool, None),
        "count_published": PortfolioItem::count(pool, Some("published")),
        "count_draft": PortfolioItem::count(pool, Some("draft")),
        "settings": Setting::all(pool),
    });

    Template::render("admin/portfolio/list", &context)
}

#[get("/portfolio/new")]
pub fn portfolio_new(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let categories = Category::list(pool, Some("portfolio"));
    let tags = Tag::list(pool);

    let context = json!({
        "page_title": "New Portfolio Item",
        "categories": categories,
        "tags": tags,
        "settings": Setting::all(pool),
    });

    Template::render("admin/portfolio/edit", &context)
}

#[get("/portfolio/<id>/edit")]
pub fn portfolio_edit(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Option<Template> {
    let item = PortfolioItem::find_by_id(pool, id)?;
    let categories = Category::list(pool, Some("portfolio"));
    let tags = Tag::list(pool);
    let item_categories = Category::for_content(pool, id, "portfolio");
    let item_tags = Tag::for_content(pool, id, "portfolio");

    let context = json!({
        "page_title": "Edit Portfolio Item",
        "item": item,
        "categories": categories,
        "tags": tags,
        "item_categories": item_categories.iter().map(|c| c.id).collect::<Vec<_>>(),
        "item_tags": item_tags.iter().map(|t| t.id).collect::<Vec<_>>(),
        "settings": Setting::all(pool),
    });

    Some(Template::render("admin/portfolio/edit", &context))
}

#[post("/portfolio/<id>/delete")]
pub fn portfolio_delete(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = PortfolioItem::delete(pool, id);
    Redirect::to("/admin/portfolio")
}

// ── Comments ───────────────────────────────────────────

#[get("/comments?<status>&<page>")]
pub fn comments_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let comments = Comment::list(pool, status.as_deref(), per_page, offset);
    let total = Comment::count(pool, status.as_deref());

    let context = json!({
        "page_title": "Comments",
        "comments": comments,
        "current_page": current_page,
        "total": total,
        "status_filter": status,
        "count_all": Comment::count(pool, None),
        "count_pending": Comment::count(pool, Some("pending")),
        "count_approved": Comment::count(pool, Some("approved")),
        "count_spam": Comment::count(pool, Some("spam")),
        "settings": Setting::all(pool),
    });

    Template::render("admin/comments/list", &context)
}

#[post("/comments/<id>/approve")]
pub fn comment_approve(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "approved");
    Redirect::to("/admin/comments")
}

#[post("/comments/<id>/spam")]
pub fn comment_spam(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "spam");
    Redirect::to("/admin/comments")
}

#[post("/comments/<id>/delete")]
pub fn comment_delete(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Comment::delete(pool, id);
    Redirect::to("/admin/comments")
}

// ── Categories ─────────────────────────────────────────

#[get("/categories?<type_filter>")]
pub fn categories_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    type_filter: Option<String>,
) -> Template {
    let categories = Category::list(pool, type_filter.as_deref());

    let categories_with_count: Vec<serde_json::Value> = categories
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "slug": c.slug,
                "type": c.r#type,
                "count": Category::count_items(pool, c.id),
            })
        })
        .collect();

    let context = json!({
        "page_title": "Categories",
        "categories": categories_with_count,
        "type_filter": type_filter,
        "settings": Setting::all(pool),
    });

    Template::render("admin/categories/list", &context)
}

// ── Tags ───────────────────────────────────────────────

#[get("/tags")]
pub fn tags_list(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let tags = Tag::list(pool);

    let tags_with_count: Vec<serde_json::Value> = tags
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "slug": t.slug,
                "count": Tag::count_items(pool, t.id),
            })
        })
        .collect();

    let context = json!({
        "page_title": "Tags",
        "tags": tags_with_count,
        "settings": Setting::all(pool),
    });

    Template::render("admin/tags/list", &context)
}

// ── Designs ────────────────────────────────────────────

#[get("/designs")]
pub fn designs_list(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let designs = Design::list(pool);

    let context = json!({
        "page_title": "Designs",
        "designs": designs,
        "settings": Setting::all(pool),
    });

    Template::render("admin/designs/list", &context)
}

#[post("/designs/<id>/activate")]
pub fn design_activate(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Design::activate(pool, id);
    Redirect::to("/admin/designs")
}

// ── Import ─────────────────────────────────────────────

#[get("/import")]
pub fn import_page(_admin: AdminUser, pool: &State<DbPool>) -> Template {
    let history = Import::list(pool);

    let context = json!({
        "page_title": "Import",
        "history": history,
        "settings": Setting::all(pool),
    });

    Template::render("admin/import/index", &context)
}

// ── Settings ───────────────────────────────────────────

#[get("/settings/<section>")]
pub fn settings_page(
    _admin: AdminUser,
    pool: &State<DbPool>,
    section: &str,
    flash: Option<rocket::request::FlashMessage<'_>>,
) -> Option<Template> {
    let valid_sections = [
        "general", "blog", "portfolio", "comments", "typography", "images", "seo", "security",
        "design", "social", "commerce", "paypal", "users", "ai", "email",
    ];

    if !valid_sections.contains(&section) {
        return None;
    }

    let mut context = json!({
        "page_title": format!("Settings — {}", section.chars().next().unwrap().to_uppercase().to_string() + &section[1..]),
        "section": section,
        "settings": Setting::all(pool),
    });

    if let Some(ref f) = flash {
        context["flash_kind"] = json!(f.kind());
        context["flash_msg"] = json!(f.message());
    }

    let template_name: String = format!("admin/settings/{}", section);
    Some(Template::render(template_name, &context))
}

// ── POST: Create Post ──────────────────────────────────

#[derive(FromForm)]
pub struct PostFormData<'f> {
    pub title: String,
    pub slug: String,
    pub content_html: String,
    pub excerpt: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub category_ids: Option<Vec<i64>>,
    pub featured_image: Option<TempFile<'f>>,
}

async fn save_upload(file: &mut TempFile<'_>, prefix: &str) -> Option<String> {
    let ext = file
        .content_type()
        .and_then(|ct| ct.extension())
        .map(|e| e.to_string())
        .unwrap_or_else(|| "jpg".to_string());
    let filename = format!("{}_{}.{}", prefix, uuid::Uuid::new_v4(), ext);
    let dest = std::path::Path::new("website/uploads").join(&filename);
    let _ = std::fs::create_dir_all("website/uploads");
    match file.persist_to(&dest).await {
        Ok(_) => Some(filename),
        Err(_) => None,
    }
}

// ── Image Upload API (for TinyMCE) ─────────────────────

#[derive(FromForm)]
pub struct ImageUploadForm<'f> {
    pub file: TempFile<'f>,
}

#[post("/upload/image", data = "<form>")]
pub async fn upload_image(
    _admin: AdminUser,
    mut form: Form<ImageUploadForm<'_>>,
) -> Json<Value> {
    match save_upload(&mut form.file, "editor").await {
        Some(filename) => Json(json!({ "location": format!("/uploads/{}", filename) })),
        None => Json(json!({ "error": "Upload failed" })),
    }
}

#[post("/posts/new", data = "<form>")]
pub async fn posts_create(
    _admin: AdminUser,
    pool: &State<DbPool>,
    mut form: Form<PostFormData<'_>>,
) -> Redirect {
    let featured = match form.featured_image.as_mut() {
        Some(f) if f.len() > 0 => save_upload(f, "post").await,
        _ => None,
    };

    let post_form = PostForm {
        title: form.title.clone(),
        slug: form.slug.clone(),
        content_json: "{}".to_string(),
        content_html: form.content_html.clone(),
        excerpt: form.excerpt.clone(),
        featured_image: featured,
        meta_title: form.meta_title.clone(),
        meta_description: form.meta_description.clone(),
        status: form.status.clone(),
        published_at: if form.status == "published" {
            Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string())
        } else {
            None
        },
        category_ids: form.category_ids.clone(),
        tag_ids: None,
    };

    match Post::create(pool, &post_form) {
        Ok(id) => {
            if let Some(ref cat_ids) = form.category_ids {
                let _ = Category::set_for_content(pool, id, "post", cat_ids);
            }
            Redirect::to(format!("/admin/posts/{}/edit", id))
        }
        Err(_) => Redirect::to("/admin/posts"),
    }
}

#[post("/posts/<id>/edit", data = "<form>")]
pub async fn posts_update(
    _admin: AdminUser,
    pool: &State<DbPool>,
    id: i64,
    mut form: Form<PostFormData<'_>>,
) -> Redirect {
    let featured = match form.featured_image.as_mut() {
        Some(f) if f.len() > 0 => save_upload(f, "post").await,
        _ => Post::find_by_id(pool, id).and_then(|p| p.featured_image),
    };

    let post_form = PostForm {
        title: form.title.clone(),
        slug: form.slug.clone(),
        content_json: "{}".to_string(),
        content_html: form.content_html.clone(),
        excerpt: form.excerpt.clone(),
        featured_image: featured,
        meta_title: form.meta_title.clone(),
        meta_description: form.meta_description.clone(),
        status: form.status.clone(),
        published_at: if form.status == "published" {
            Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string())
        } else {
            None
        },
        category_ids: form.category_ids.clone(),
        tag_ids: None,
    };

    let _ = Post::update(pool, id, &post_form);
    if let Some(ref cat_ids) = form.category_ids {
        let _ = Category::set_for_content(pool, id, "post", cat_ids);
    }
    Redirect::to(format!("/admin/posts/{}/edit", id))
}

// ── POST: Create/Update Portfolio ──────────────────────

#[derive(FromForm)]
pub struct PortfolioFormData<'f> {
    pub title: String,
    pub slug: String,
    pub description_html: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub category_ids: Option<Vec<i64>>,
    pub image: Option<TempFile<'f>>,
}

#[post("/portfolio/new", data = "<form>")]
pub async fn portfolio_create(
    _admin: AdminUser,
    pool: &State<DbPool>,
    mut form: Form<PortfolioFormData<'_>>,
) -> Redirect {
    let image_path = match form.image.as_mut() {
        Some(f) if f.len() > 0 => save_upload(f, "portfolio").await.unwrap_or_else(|| "placeholder.jpg".to_string()),
        _ => "placeholder.jpg".to_string(),
    };

    let pf = PortfolioForm {
        title: form.title.clone(),
        slug: form.slug.clone(),
        description_json: None,
        description_html: form.description_html.clone(),
        image_path,
        thumbnail_path: None,
        meta_title: form.meta_title.clone(),
        meta_description: form.meta_description.clone(),
        sell_enabled: None,
        price: None,
        status: form.status.clone(),
        published_at: if form.status == "published" {
            Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string())
        } else {
            None
        },
        category_ids: form.category_ids.clone(),
        tag_ids: None,
    };

    match PortfolioItem::create(pool, &pf) {
        Ok(id) => {
            if let Some(ref cat_ids) = form.category_ids {
                let _ = Category::set_for_content(pool, id, "portfolio", cat_ids);
            }
            Redirect::to(format!("/admin/portfolio/{}/edit", id))
        }
        Err(_) => Redirect::to("/admin/portfolio"),
    }
}

#[post("/portfolio/<id>/edit", data = "<form>")]
pub async fn portfolio_update(
    _admin: AdminUser,
    pool: &State<DbPool>,
    id: i64,
    mut form: Form<PortfolioFormData<'_>>,
) -> Redirect {
    let image_path = match form.image.as_mut() {
        Some(f) if f.len() > 0 => save_upload(f, "portfolio").await.unwrap_or_else(|| "placeholder.jpg".to_string()),
        _ => PortfolioItem::find_by_id(pool, id)
            .map(|e| e.image_path)
            .unwrap_or_else(|| "placeholder.jpg".to_string()),
    };

    let pf = PortfolioForm {
        title: form.title.clone(),
        slug: form.slug.clone(),
        description_json: None,
        description_html: form.description_html.clone(),
        image_path,
        thumbnail_path: None,
        meta_title: form.meta_title.clone(),
        meta_description: form.meta_description.clone(),
        sell_enabled: None,
        price: None,
        status: form.status.clone(),
        published_at: if form.status == "published" {
            Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string())
        } else {
            None
        },
        category_ids: form.category_ids.clone(),
        tag_ids: None,
    };

    let _ = PortfolioItem::update(pool, id, &pf);
    if let Some(ref cat_ids) = form.category_ids {
        let _ = Category::set_for_content(pool, id, "portfolio", cat_ids);
    }
    Redirect::to(format!("/admin/portfolio/{}/edit", id))
}

// ── POST: Category Create/Delete ───────────────────────

#[derive(FromForm)]
pub struct CategoryFormData {
    pub name: String,
    pub slug: String,
    pub r#type: String,
}

#[post("/categories/new", data = "<form>")]
pub fn category_create(
    _admin: AdminUser,
    pool: &State<DbPool>,
    form: Form<CategoryFormData>,
) -> Redirect {
    let slug = if form.slug.is_empty() {
        slug::slugify(&form.name)
    } else {
        form.slug.clone()
    };
    let _ = Category::create(
        pool,
        &CategoryForm {
            name: form.name.clone(),
            slug,
            r#type: form.r#type.clone(),
        },
    );
    Redirect::to("/admin/categories")
}

#[post("/categories/<id>/delete")]
pub fn category_delete(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Category::delete(pool, id);
    Redirect::to("/admin/categories")
}

// ── POST: Tag Delete ───────────────────────────────────

#[post("/tags/<id>/delete")]
pub fn tag_delete(_admin: AdminUser, pool: &State<DbPool>, id: i64) -> Redirect {
    let _ = Tag::delete(pool, id);
    Redirect::to("/admin/tags")
}

// ── POST: Settings Save ────────────────────────────────

#[post("/settings/<section>", data = "<form>")]
pub fn settings_save(
    _admin: AdminUser,
    pool: &State<DbPool>,
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
        "ai" => vec![
            ("ai_ollama_enabled", "Ollama", vec!["ai_ollama_url", "ai_ollama_model"]),
            ("ai_openai_enabled", "OpenAI", vec!["ai_openai_api_key"]),
            ("ai_gemini_enabled", "Gemini", vec!["ai_gemini_api_key"]),
            ("ai_cloudflare_enabled", "Cloudflare Workers AI", vec!["ai_cloudflare_account_id", "ai_cloudflare_api_token"]),
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

    if !errors.is_empty() {
        return Err(Flash::error(
            Redirect::to(format!("/admin/settings/{}", section)),
            errors.join(" | "),
        ));
    }

    // Checkboxes don't submit a value when unchecked, so we must
    // explicitly reset all known boolean keys for this section first.
    let checkbox_keys: &[&str] = match section {
        "ai" => &[
            "ai_local_enabled", "ai_ollama_enabled", "ai_openai_enabled",
            "ai_gemini_enabled", "ai_cloudflare_enabled",
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
        ],
        "comments" => &[
            "comments_enabled", "comments_on_blog", "comments_on_portfolio",
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
        "typography" => &["font_google_enabled", "font_adobe_enabled", "font_sitewide"],
        "design" => &["design_back_to_top"],
        "social" => &["social_brand_colors"],
        _ => &[],
    };
    for key in checkbox_keys {
        let _ = Setting::set(pool, key, "false");
    }

    let _ = Setting::set_many(pool, &data);
    Ok(Flash::success(
        Redirect::to(format!("/admin/settings/{}", section)),
        "Settings saved successfully",
    ))
}

// ── POST: WordPress Import ─────────────────────────────

#[post("/import/wordpress", data = "<data>")]
pub async fn import_wordpress(
    _admin: AdminUser,
    pool: &State<DbPool>,
    data: Data<'_>,
) -> Redirect {
    // Read up to 50MB of upload data
    let bytes = match data.open(50.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => return Redirect::to("/admin/import"),
    };

    let xml_content = String::from_utf8_lossy(&bytes).to_string();
    let _ = crate::import::wordpress::import_wxr(pool, &xml_content);
    Redirect::to("/admin/import")
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        dashboard,
        posts_list,
        posts_new,
        posts_edit,
        posts_delete,
        posts_create,
        posts_update,
        portfolio_list,
        portfolio_new,
        portfolio_edit,
        portfolio_delete,
        portfolio_create,
        portfolio_update,
        comments_list,
        comment_approve,
        comment_spam,
        comment_delete,
        categories_list,
        category_create,
        category_delete,
        tags_list,
        tag_delete,
        designs_list,
        design_activate,
        import_page,
        import_wordpress,
        settings_page,
        settings_save,
        upload_image,
    ]
}
