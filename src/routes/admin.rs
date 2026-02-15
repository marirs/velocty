use rocket::data::{Data, ToByteUnit};
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::{Flash, Redirect};
use rocket::serde::json::Json;
use rocket::State;
use rocket_dyn_templates::Template;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::security::auth::AdminUser;
use crate::db::DbPool;
use crate::AdminSlug;

/// Helper: get the admin base path from managed state
fn admin_base(slug: &AdminSlug) -> String {
    format!("/{}", slug.0)
}
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
pub fn dashboard(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let posts_count = Post::count(pool, None);
    let posts_draft = Post::count(pool, Some("draft"));
    let portfolio_count = PortfolioItem::count(pool, None);
    let comments_pending = Comment::count(pool, Some("pending"));

    let context = json!({
        "page_title": "Dashboard",
        "admin_slug": slug.0,
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
    slug: &State<AdminSlug>,
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
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/posts/list", &context)
}

#[get("/posts/new")]
pub fn posts_new(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let categories = Category::list(pool, Some("post"));
    let tags = Tag::list(pool);

    let ai_enabled = crate::ai::is_enabled(pool);
    let ai_has_vision = crate::ai::has_vision_provider(pool);
    let context = json!({
        "page_title": "New Post",
        "admin_slug": slug.0,
        "categories": categories,
        "tags": tags,
        "settings": Setting::all(pool),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Template::render("admin/posts/edit", &context)
}

#[get("/posts/<id>/edit")]
pub fn posts_edit(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Option<Template> {
    let post = Post::find_by_id(pool, id)?;
    let categories = Category::list(pool, Some("post"));
    let tags = Tag::list(pool);
    let post_categories = Category::for_content(pool, id, "post");
    let post_tags = Tag::for_content(pool, id, "post");

    let ai_enabled = crate::ai::is_enabled(pool);
    let ai_has_vision = crate::ai::has_vision_provider(pool);
    let context = json!({
        "page_title": "Edit Post",
        "post": post,
        "categories": categories,
        "tags": tags,
        "post_categories": post_categories.iter().map(|c| c.id).collect::<Vec<_>>(),
        "post_tags": post_tags.iter().map(|t| t.id).collect::<Vec<_>>(),
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Some(Template::render("admin/posts/edit", &context))
}

#[post("/posts/<id>/delete")]
pub fn posts_delete(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Post::delete(pool, id);
    Redirect::to(format!("{}/posts", admin_base(slug)))
}

// ── Portfolio ──────────────────────────────────────────

#[get("/portfolio?<status>&<page>")]
pub fn portfolio_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/portfolio/list", &context)
}

#[get("/portfolio/new")]
pub fn portfolio_new(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let categories = Category::list(pool, Some("portfolio"));
    let tags = Tag::list(pool);

    let ai_enabled = crate::ai::is_enabled(pool);
    let ai_has_vision = crate::ai::has_vision_provider(pool);
    let context = json!({
        "page_title": "New Portfolio Item",
        "admin_slug": slug.0,
        "categories": categories,
        "tags": tags,
        "settings": Setting::all(pool),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Template::render("admin/portfolio/edit", &context)
}

#[get("/portfolio/<id>/edit")]
pub fn portfolio_edit(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Option<Template> {
    let item = PortfolioItem::find_by_id(pool, id)?;
    let categories = Category::list(pool, Some("portfolio"));
    let tags = Tag::list(pool);
    let item_categories = Category::for_content(pool, id, "portfolio");
    let item_tags = Tag::for_content(pool, id, "portfolio");

    let ai_enabled = crate::ai::is_enabled(pool);
    let ai_has_vision = crate::ai::has_vision_provider(pool);
    let context = json!({
        "page_title": "Edit Portfolio Item",
        "item": item,
        "categories": categories,
        "tags": tags,
        "item_categories": item_categories.iter().map(|c| c.id).collect::<Vec<_>>(),
        "item_tags": item_tags.iter().map(|t| t.id).collect::<Vec<_>>(),
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Some(Template::render("admin/portfolio/edit", &context))
}

#[post("/portfolio/<id>/delete")]
pub fn portfolio_delete(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = PortfolioItem::delete(pool, id);
    Redirect::to(format!("{}/portfolio", admin_base(slug)))
}

// ── Comments ───────────────────────────────────────────

#[get("/comments?<status>&<page>")]
pub fn comments_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/comments/list", &context)
}

#[post("/comments/<id>/approve")]
pub fn comment_approve(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "approved");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/spam")]
pub fn comment_spam(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::update_status(pool, id, "spam");
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

#[post("/comments/<id>/delete")]
pub fn comment_delete(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Comment::delete(pool, id);
    Redirect::to(format!("{}/comments", admin_base(slug)))
}

// ── Categories ─────────────────────────────────────────

#[get("/categories?<type_filter>")]
pub fn categories_list(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/categories/list", &context)
}

// ── Tags ───────────────────────────────────────────────

#[get("/tags")]
pub fn tags_list(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
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
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/tags/list", &context)
}

// ── Designs ────────────────────────────────────────────

#[get("/designer")]
pub fn designs_list(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let designs = Design::list(pool);

    let context = json!({
        "page_title": "Designer",
        "designs": designs,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/designs/list", &context)
}

#[post("/designer/<id>/activate")]
pub fn design_activate(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Design::activate(pool, id);
    Redirect::to(format!("{}/designer", admin_base(slug)))
}

// ── Import ─────────────────────────────────────────────

#[get("/import")]
pub fn import_page(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let history = Import::list(pool);

    let context = json!({
        "page_title": "Import",
        "history": history,
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/import/index", &context)
}

// ── Settings ───────────────────────────────────────────

#[get("/settings/<section>")]
pub fn settings_page(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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

    let section_label = match section {
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
    let dest = std::path::Path::new("website/site/uploads").join(&filename);
    let _ = std::fs::create_dir_all("website/site/uploads");
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

// ── Font Upload API ─────────────────────────────────────

#[derive(FromForm)]
pub struct FontUploadForm<'f> {
    pub file: TempFile<'f>,
    pub font_name: String,
}

#[post("/upload/font", data = "<form>")]
pub async fn upload_font(
    _admin: AdminUser,
    pool: &State<DbPool>,
    mut form: Form<FontUploadForm<'_>>,
) -> Json<Value> {
    let font_name = form.font_name.trim().to_string();
    if font_name.is_empty() {
        return Json(json!({ "error": "Font name is required" }));
    }

    let raw_name = form.file.raw_name()
        .map(|n| n.dangerous_unsafe_unsanitized_raw().to_string())
        .unwrap_or_default();
    let ext = raw_name.rsplit('.').next().unwrap_or("woff2").to_lowercase();
    let valid_exts = ["woff2", "woff", "ttf", "otf"];
    if !valid_exts.contains(&ext.as_str()) {
        return Json(json!({ "error": "Invalid font file type. Use .woff2, .woff, .ttf, or .otf" }));
    }

    let filename = format!("{}_{}.{}", font_name.to_lowercase().replace(' ', "-"), uuid::Uuid::new_v4(), ext);
    let fonts_dir = std::path::Path::new("website/site/uploads/fonts");
    let _ = std::fs::create_dir_all(fonts_dir);
    let dest = fonts_dir.join(&filename);

    match form.file.persist_to(&dest).await {
        Ok(_) => {
            let _ = Setting::set(pool, "font_custom_name", &font_name);
            let _ = Setting::set(pool, "font_custom_filename", &filename);
            Json(json!({ "success": true, "font_name": font_name, "filename": filename }))
        }
        Err(e) => Json(json!({ "error": format!("Upload failed: {}", e) })),
    }
}

#[post("/posts/new", data = "<form>")]
pub async fn posts_create(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
            Redirect::to(format!("{}/posts/{}/edit", admin_base(slug), id))
        }
        Err(_) => Redirect::to(format!("{}/posts", admin_base(slug))),
    }
}

#[post("/posts/<id>/edit", data = "<form>")]
pub async fn posts_update(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
    Redirect::to(format!("{}/posts/{}/edit", admin_base(slug), id))
}

// ── POST: Create/Update Portfolio ──────────────────────

#[derive(FromForm)]
pub struct PortfolioFormData<'f> {
    pub title: String,
    pub slug: String,
    pub description_html: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub sell_enabled: Option<String>,
    pub price: Option<f64>,
    pub purchase_note: Option<String>,
    pub payment_provider: Option<String>,
    pub download_file_path: Option<String>,
    pub status: String,
    pub category_ids: Option<Vec<i64>>,
    pub image: Option<TempFile<'f>>,
}

#[post("/portfolio/new", data = "<form>")]
pub async fn portfolio_create(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
        sell_enabled: Some(form.sell_enabled.is_some()),
        price: form.price,
        purchase_note: form.purchase_note.clone(),
        payment_provider: form.payment_provider.clone(),
        download_file_path: form.download_file_path.clone(),
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
            Redirect::to(format!("{}/portfolio/{}/edit", admin_base(slug), id))
        }
        Err(_) => Redirect::to(format!("{}/portfolio", admin_base(slug))),
    }
}

#[post("/portfolio/<id>/edit", data = "<form>")]
pub async fn portfolio_update(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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
        sell_enabled: Some(form.sell_enabled.is_some()),
        price: form.price,
        purchase_note: form.purchase_note.clone(),
        payment_provider: form.payment_provider.clone(),
        download_file_path: form.download_file_path.clone(),
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
    Redirect::to(format!("{}/portfolio/{}/edit", admin_base(slug), id))
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
    admin_slug: &State<AdminSlug>,
    form: Form<CategoryFormData>,
) -> Redirect {
    let cat_slug = if form.slug.is_empty() {
        slug::slugify(&form.name)
    } else {
        form.slug.clone()
    };
    let _ = Category::create(
        pool,
        &CategoryForm {
            name: form.name.clone(),
            slug: cat_slug,
            r#type: form.r#type.clone(),
        },
    );
    Redirect::to(format!("{}/categories", admin_base(admin_slug)))
}

#[post("/categories/<id>/delete")]
pub fn category_delete(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Category::delete(pool, id);
    Redirect::to(format!("{}/categories", admin_base(slug)))
}

// ── POST: Tag Delete ───────────────────────────────────

#[post("/tags/<id>/delete")]
pub fn tag_delete(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let _ = Tag::delete(pool, id);
    Redirect::to(format!("{}/tags", admin_base(slug)))
}

// ── POST: Settings Save ────────────────────────────────

#[post("/settings/<section>", data = "<form>")]
pub fn settings_save(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
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

    // Blog/Portfolio slug validation:
    // - Only validate slug emptiness for *enabled* modules
    // - Both enabled modules cannot have empty slugs simultaneously
    // - Both enabled modules cannot share the same slug
    if section == "blog" {
        let journal_enabled = data.get("journal_enabled").map(|v| v.as_str()) == Some("true");
        let blog_slug = data.get("blog_slug").map(|v| v.trim()).unwrap_or("");
        let portfolio_enabled = Setting::get_or(pool, "portfolio_enabled", "false") == "true";
        let portfolio_slug = Setting::get_or(pool, "portfolio_slug", "portfolio");

        if journal_enabled && blog_slug.is_empty() && portfolio_enabled && portfolio_slug.is_empty() {
            errors.push("Journal Slug cannot be empty while Portfolio Slug is also empty — at least one must have a slug".to_string());
        }
        if journal_enabled && !blog_slug.is_empty() && portfolio_enabled && blog_slug == portfolio_slug {
            errors.push("Journal Slug and Portfolio Slug cannot be the same".to_string());
        }
    }
    if section == "portfolio" {
        let portfolio_enabled = data.get("portfolio_enabled").map(|v| v.as_str()) == Some("true");
        let portfolio_slug = data.get("portfolio_slug").map(|v| v.trim()).unwrap_or("");
        let journal_enabled = Setting::get_or(pool, "journal_enabled", "true") == "true";
        let blog_slug = Setting::get_or(pool, "blog_slug", "journal");

        if portfolio_enabled && portfolio_slug.is_empty() && journal_enabled && blog_slug.is_empty() {
            errors.push("Portfolio Slug cannot be empty while Journal Slug is also empty — at least one must have a slug".to_string());
        }
        if portfolio_enabled && !portfolio_slug.is_empty() && journal_enabled && portfolio_slug == blog_slug {
            errors.push("Portfolio Slug and Journal Slug cannot be the same".to_string());
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
            "design_back_to_top",
            "cookie_consent_enabled", "cookie_consent_show_reject",
            "privacy_policy_enabled", "terms_of_use_enabled",
        ],
        "social" => &["social_brand_colors"],
        _ => &[],
    };
    for key in checkbox_keys {
        let _ = Setting::set(pool, key, "false");
    }

    let _ = Setting::set_many(pool, &data);

    // Proactive slug defaults: when re-enabling a module whose slug is empty,
    // auto-fill the default slug to prevent future conflicts
    if section == "blog" {
        let journal_enabled = Setting::get_or(pool, "journal_enabled", "true") == "true";
        let blog_slug = Setting::get_or(pool, "blog_slug", "");
        if journal_enabled && blog_slug.is_empty() {
            let portfolio_slug = Setting::get_or(pool, "portfolio_slug", "");
            if portfolio_slug.is_empty() || portfolio_slug != "journal" {
                let _ = Setting::set(pool, "blog_slug", "journal");
            } else {
                let _ = Setting::set(pool, "blog_slug", "blog");
            }
        }
    }
    if section == "portfolio" {
        let portfolio_enabled = Setting::get_or(pool, "portfolio_enabled", "false") == "true";
        let portfolio_slug = Setting::get_or(pool, "portfolio_slug", "");
        if portfolio_enabled && portfolio_slug.is_empty() {
            let blog_slug = Setting::get_or(pool, "blog_slug", "");
            if blog_slug.is_empty() || blog_slug != "portfolio" {
                let _ = Setting::set(pool, "portfolio_slug", "portfolio");
            } else {
                let _ = Setting::set(pool, "portfolio_slug", "gallery");
            }
        }
    }

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

    Ok(Flash::success(
        Redirect::to(format!("{}/settings/{}{}", admin_base(slug), section, tab_fragment)),
        "Settings saved successfully",
    ))
}

// ── POST: WordPress Import ─────────────────────────────

#[post("/import/wordpress", data = "<data>")]
pub async fn import_wordpress(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Redirect {
    // Read up to 50MB of upload data
    let bytes = match data.open(50.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => return Redirect::to(format!("{}/import", admin_base(slug))),
    };

    let xml_content = String::from_utf8_lossy(&bytes).to_string();
    let _ = crate::import::wordpress::import_wxr(pool, &xml_content);
    Redirect::to(format!("{}/import", admin_base(slug)))
}

// ── Health ─────────────────────────────────────────────────

#[get("/health")]
pub fn health_page(_admin: AdminUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
    let report = crate::health::gather(pool);
    let context = json!({
        "page_title": "Health",
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
        "report": report,
    });
    Template::render("admin/health", &context)
}

#[post("/health/vacuum")]
pub fn health_vacuum(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_vacuum(pool);
    json_tool_result(r)
}

#[post("/health/wal-checkpoint")]
pub fn health_wal_checkpoint(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_wal_checkpoint(pool);
    json_tool_result(r)
}

#[post("/health/integrity-check")]
pub fn health_integrity_check(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_integrity_check(pool);
    json_tool_result(r)
}

#[post("/health/session-cleanup")]
pub fn health_session_cleanup(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_session_cleanup(pool);
    json_tool_result(r)
}

#[post("/health/orphan-scan")]
pub fn health_orphan_scan(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_orphan_scan(pool, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/orphan-delete")]
pub fn health_orphan_delete(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_orphan_delete(pool, "website/site/uploads");
    json_tool_result(r)
}

#[post("/health/unused-tags")]
pub fn health_unused_tags(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::run_unused_tags_cleanup(pool);
    json_tool_result(r)
}

#[derive(Debug, serde::Deserialize)]
pub struct AnalyticsPruneForm {
    pub days: u64,
}

#[post("/health/analytics-prune", format = "json", data = "<body>")]
pub fn health_analytics_prune(_admin: AdminUser, pool: &State<DbPool>, body: Json<AnalyticsPruneForm>) -> Json<Value> {
    let r = crate::health::run_analytics_prune(pool, body.days);
    json_tool_result(r)
}

#[post("/health/export-db")]
pub fn health_export_db(_admin: AdminUser) -> Json<Value> {
    let r = crate::health::export_database();
    json_tool_result(r)
}

#[post("/health/export-content")]
pub fn health_export_content(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let r = crate::health::export_content(pool);
    json_tool_result(r)
}

#[post("/health/mongo-ping")]
pub fn health_mongo_ping(_admin: AdminUser) -> Json<Value> {
    let uri = crate::health::read_db_backend();
    if uri != "mongodb" {
        return Json(json!({ "ok": false, "message": "Not using MongoDB backend.", "details": null }));
    }
    let mongo_uri = std::fs::read_to_string("velocty.toml")
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|v| v.get("database")?.get("uri")?.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "mongodb://localhost:27017".to_string());

    let start = std::time::Instant::now();
    let report = crate::health::gather_mongo_ping(&mongo_uri);
    let latency = start.elapsed().as_millis();

    if report.0 {
        Json(json!({ "ok": true, "message": format!("MongoDB is reachable. Latency: {} ms", report.1), "details": null }))
    } else {
        Json(json!({ "ok": false, "message": format!("MongoDB unreachable ({}ms timeout)", latency), "details": null }))
    }
}

fn json_tool_result(r: crate::health::ToolResult) -> Json<Value> {
    Json(json!({
        "ok": r.ok,
        "message": r.message,
        "details": r.details,
    }))
}

// ── Import: Velocty ───────────────────────────────────────

#[post("/import/velocty", data = "<data>")]
pub async fn import_velocty(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    data: Data<'_>,
) -> Flash<Redirect> {
    let bytes = match data.open(100.mebibytes()).into_bytes().await {
        Ok(b) if b.is_complete() => b.into_inner(),
        _ => return Flash::error(
            Redirect::to(format!("{}/import", admin_base(slug))),
            "Failed to read upload data.",
        ),
    };

    let json_str = String::from_utf8_lossy(&bytes).to_string();
    let export: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => return Flash::error(
            Redirect::to(format!("{}/import", admin_base(slug))),
            format!("Invalid JSON: {}", e),
        ),
    };

    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => return Flash::error(
            Redirect::to(format!("{}/import", admin_base(slug))),
            format!("DB error: {}", e),
        ),
    };

    let mut imported_posts = 0u64;
    let mut imported_portfolio = 0u64;
    let mut imported_comments = 0u64;
    let mut imported_categories = 0u64;
    let mut imported_tags = 0u64;

    // Import categories
    if let Some(cats) = export.get("categories").and_then(|v| v.as_array()) {
        for cat in cats {
            let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO categories (name, slug) VALUES (?1, ?2)",
                    rusqlite::params![name, slug_val],
                );
                imported_categories += 1;
            }
        }
    }

    // Import tags
    if let Some(tags) = export.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = tag.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO tags (name, slug) VALUES (?1, ?2)",
                    rusqlite::params![name, slug_val],
                );
                imported_tags += 1;
            }
        }
    }

    // Import posts
    if let Some(posts) = export.get("posts").and_then(|v| v.as_array()) {
        for post in posts {
            let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = post.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let body = post.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let excerpt = post.get("excerpt").and_then(|v| v.as_str()).unwrap_or("");
            let featured_image = post.get("featured_image").and_then(|v| v.as_str()).unwrap_or("");
            let status = post.get("status").and_then(|v| v.as_str()).unwrap_or("draft");
            let created_at = post.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let updated_at = post.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
            if !title.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO posts (title, slug, body, excerpt, featured_image, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![title, slug_val, body, excerpt, featured_image, status, created_at, updated_at],
                );
                imported_posts += 1;
            }
        }
    }

    // Import portfolio items
    if let Some(items) = export.get("portfolio_items").and_then(|v| v.as_array()) {
        for item in items {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug_val = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let image_path = item.get("image_path").and_then(|v| v.as_str()).unwrap_or("");
            let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("draft");
            let created_at = item.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let updated_at = item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
            if !title.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO portfolio_items (title, slug, description, image_path, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![title, slug_val, description, image_path, status, created_at, updated_at],
                );
                imported_portfolio += 1;
            }
        }
    }

    // Import comments
    if let Some(comments) = export.get("comments").and_then(|v| v.as_array()) {
        for comment in comments {
            let post_id = comment.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let author_name = comment.get("author_name").and_then(|v| v.as_str()).unwrap_or("");
            let author_email = comment.get("author_email").and_then(|v| v.as_str()).unwrap_or("");
            let body = comment.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let status = comment.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
            let created_at = comment.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            if post_id > 0 && !body.is_empty() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO comments (post_id, author_name, author_email, body, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![post_id, author_name, author_email, body, status, created_at],
                );
                imported_comments += 1;
            }
        }
    }

    // Import settings (skip sensitive ones)
    let skip_keys = ["admin_password_hash", "setup_completed", "admin_slug", "admin_email", "session_secret"];
    if let Some(settings) = export.get("settings").and_then(|v| v.as_array()) {
        for setting in settings {
            let key = setting.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let value = setting.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if !key.is_empty() && !skip_keys.contains(&key) {
                let _ = Setting::set(pool, key, value);
            }
        }
    }

    // Log import
    let _ = conn.execute(
        "INSERT INTO imports (source, filename, posts_count, portfolio_count, comments_count, skipped_count, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
        rusqlite::params![
            "velocty",
            "velocty_export.json",
            imported_posts,
            imported_portfolio,
            imported_comments,
            0i64,
        ],
    );

    Flash::success(
        Redirect::to(format!("{}/import", admin_base(slug))),
        format!(
            "Imported {} posts, {} portfolio items, {} comments, {} categories, {} tags.",
            imported_posts, imported_portfolio, imported_comments, imported_categories, imported_tags
        ),
    )
}

// ── MFA Setup / Disable ─────────────────────────────────

#[post("/mfa/setup", format = "json")]
pub fn mfa_setup(
    _admin: AdminUser,
    pool: &State<DbPool>,
) -> Json<Value> {
    let site_name = Setting::get_or(pool, "site_name", "Velocty");
    let admin_email = Setting::get_or(pool, "admin_email", "admin");

    let secret = crate::security::mfa::generate_secret();
    let qr = match crate::security::mfa::qr_data_uri(&secret, &site_name, &admin_email) {
        Ok(uri) => uri,
        Err(e) => return Json(json!({ "ok": false, "error": e })),
    };

    // Store the pending secret temporarily (not yet confirmed)
    let _ = Setting::set(pool, "mfa_pending_secret", &secret);

    Json(json!({ "ok": true, "qr": qr, "secret": secret }))
}

#[derive(Debug, FromForm, Deserialize)]
pub struct MfaVerifyForm {
    pub code: String,
}

#[post("/mfa/verify", format = "json", data = "<body>")]
pub fn mfa_verify(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    let pending = Setting::get_or(pool, "mfa_pending_secret", "");
    if pending.is_empty() {
        return Json(json!({ "ok": false, "error": "No pending MFA setup. Start setup first." }));
    }

    if !crate::security::mfa::verify_code(&pending, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. Please try again." }));
    }

    // Code verified — activate MFA
    let recovery_codes = crate::security::mfa::generate_recovery_codes();
    let codes_json = serde_json::to_string(&recovery_codes).unwrap_or_else(|_| "[]".to_string());

    let _ = Setting::set(pool, "mfa_secret", &pending);
    let _ = Setting::set(pool, "mfa_enabled", "true");
    let _ = Setting::set(pool, "mfa_recovery_codes", &codes_json);
    let _ = Setting::set(pool, "mfa_pending_secret", "");

    Json(json!({ "ok": true, "recovery_codes": recovery_codes }))
}

#[post("/mfa/disable", format = "json", data = "<body>")]
pub fn mfa_disable(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<MfaVerifyForm>,
) -> Json<Value> {
    let secret = Setting::get_or(pool, "mfa_secret", "");
    if secret.is_empty() {
        return Json(json!({ "ok": false, "error": "MFA is not enabled." }));
    }

    // Verify current code before disabling
    if !crate::security::mfa::verify_code(&secret, &body.code) {
        return Json(json!({ "ok": false, "error": "Invalid code. MFA was not disabled." }));
    }

    let _ = Setting::set(pool, "mfa_enabled", "false");
    let _ = Setting::set(pool, "mfa_secret", "");
    let _ = Setting::set(pool, "mfa_recovery_codes", "[]");

    Json(json!({ "ok": true }))
}

#[get("/mfa/recovery-codes")]
pub fn mfa_recovery_codes(
    _admin: AdminUser,
    pool: &State<DbPool>,
) -> Json<Value> {
    let codes_json = Setting::get_or(pool, "mfa_recovery_codes", "[]");
    let codes: Vec<String> = serde_json::from_str(&codes_json).unwrap_or_default();
    Json(json!({ "ok": true, "codes": codes }))
}

// ── Sales ──────────────────────────────────────────────

#[get("/sales")]
pub fn sales_dashboard(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
) -> Template {
    use crate::models::order::Order;
    let settings = Setting::all(pool);
    let total_revenue = Order::total_revenue(pool);
    let revenue_30d = Order::revenue_by_period(pool, 30);
    let revenue_7d = Order::revenue_by_period(pool, 7);
    let total_orders = Order::count(pool);
    let completed_orders = Order::count_by_status(pool, "completed");
    let pending_orders = Order::count_by_status(pool, "pending");
    let recent_orders = Order::list(pool, 10, 0);
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());

    let context = json!({
        "page_title": "Sales Dashboard",
        "admin_slug": &slug.0,
        "settings": &settings,
        "total_revenue": total_revenue,
        "revenue_30d": revenue_30d,
        "revenue_7d": revenue_7d,
        "total_orders": total_orders,
        "completed_orders": completed_orders,
        "pending_orders": pending_orders,
        "recent_orders": recent_orders,
        "currency": currency,
    });
    Template::render("admin/sales/dashboard", &context)
}

#[get("/sales/orders?<page>&<status>")]
pub fn sales_orders(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    page: Option<i64>,
    status: Option<String>,
) -> Template {
    use crate::models::order::Order;
    let settings = Setting::all(pool);
    let per_page: i64 = 25;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let (orders, total) = match status.as_deref() {
        Some(s) if !s.is_empty() => (
            Order::list_by_status(pool, s, per_page, offset),
            Order::count_by_status(pool, s),
        ),
        _ => (Order::list(pool, per_page, offset), Order::count(pool)),
    };

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());

    let context = json!({
        "page_title": "Orders",
        "admin_slug": &slug.0,
        "settings": &settings,
        "orders": orders,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "per_page": per_page,
        "filter_status": status.unwrap_or_default(),
        "currency": currency,
    });
    Template::render("admin/sales/orders", &context)
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
        import_velocty,
        settings_page,
        settings_save,
        upload_image,
        upload_font,
        health_page,
        health_vacuum,
        health_wal_checkpoint,
        health_integrity_check,
        health_session_cleanup,
        health_orphan_scan,
        health_orphan_delete,
        health_unused_tags,
        health_analytics_prune,
        health_export_db,
        health_export_content,
        health_mongo_ping,
        mfa_setup,
        mfa_verify,
        mfa_disable,
        mfa_recovery_codes,
        sales_dashboard,
        sales_orders,
    ]
}
