use rocket::data::{Data, ToByteUnit};
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;
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
        "page_title": "Posts",
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": Post::count(pool, None),
        "count_published": Post::count(pool, Some("published")),
        "count_draft": Post::count(pool, Some("draft")),
        "count_archived": Post::count(pool, Some("archived")),
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
    });

    Template::render("admin/import/index", &context)
}

// ── Settings ───────────────────────────────────────────

#[get("/settings/<section>")]
pub fn settings_page(
    _admin: AdminUser,
    pool: &State<DbPool>,
    section: &str,
) -> Option<Template> {
    let valid_sections = [
        "general", "blog", "portfolio", "comments", "typography", "images", "seo", "security",
        "design", "paypal", "users", "ai",
    ];

    if !valid_sections.contains(&section) {
        return None;
    }

    let context = json!({
        "page_title": format!("Settings — {}", section.chars().next().unwrap().to_uppercase().to_string() + &section[1..]),
        "section": section,
        "settings": Setting::all(pool),
    });

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
) -> Redirect {
    let data = form.into_inner();
    let _ = Setting::set_many(pool, &data);
    Redirect::to(format!("/admin/settings/{}", section))
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
    ]
}
