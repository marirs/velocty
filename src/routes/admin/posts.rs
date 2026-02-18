use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use super::save_upload;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::category::Category;
use crate::models::post::{Post, PostForm};
use crate::models::settings::Setting;
use crate::models::tag::Tag;
use crate::security::auth::{AuthorUser, EditorUser};
use crate::AdminSlug;

// ── Posts ───────────────────────────────────────────────

#[get("/posts?<status>&<page>")]
pub fn posts_list(
    _admin: AuthorUser,
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
        "count_scheduled": Post::count(pool, Some("scheduled")),
        "count_archived": Post::count(pool, Some("archived")),
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/posts/list", &context)
}

#[get("/posts/new")]
pub fn posts_new(_admin: AuthorUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
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
pub fn posts_edit(
    _admin: AuthorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Option<Template> {
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
pub fn posts_delete(
    _admin: EditorUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let title = Post::find_by_id(pool, id)
        .map(|p| p.title)
        .unwrap_or_default();
    let _ = Post::delete(pool, id);
    AuditEntry::log(
        pool,
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "delete",
        Some("post"),
        Some(id),
        Some(&title),
        None,
        None,
    );
    Redirect::to(format!("{}/posts", admin_base(slug)))
}

// ── POST: Create/Update Post ──────────────────────────

#[derive(FromForm)]
pub struct PostFormData<'f> {
    pub title: String,
    pub slug: String,
    pub content_html: String,
    pub excerpt: Option<String>,
    pub meta_title: Option<String>,
    pub meta_description: Option<String>,
    pub status: String,
    pub published_at: Option<String>,
    pub category_ids: Option<Vec<i64>>,
    pub tag_names: Option<String>,
    pub featured_image: Option<TempFile<'f>>,
}

#[post("/posts/new", data = "<form>")]
pub async fn posts_create(
    _admin: AuthorUser,
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
        published_at: if form.status == "published" || form.status == "scheduled" {
            form.published_at
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string()))
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
            if let Some(ref names) = form.tag_names {
                let tag_ids: Vec<i64> = names
                    .split(',')
                    .filter_map(|n| {
                        let n = n.trim();
                        if n.is_empty() {
                            return None;
                        }
                        Tag::find_or_create(pool, n).ok()
                    })
                    .collect();
                let _ = Tag::set_for_content(pool, id, "post", &tag_ids);
            }
            AuditEntry::log(
                pool,
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "create",
                Some("post"),
                Some(id),
                Some(&form.title),
                Some(&form.status),
                None,
            );
            if form.status == "draft" {
                Redirect::to(format!(
                    "{}/posts/{}/edit?saved=draft",
                    admin_base(slug),
                    id
                ))
            } else {
                Redirect::to(format!("{}/posts", admin_base(slug)))
            }
        }
        Err(_) => Redirect::to(format!("{}/posts", admin_base(slug))),
    }
}

#[post("/posts/<id>/edit", data = "<form>")]
pub async fn posts_update(
    _admin: AuthorUser,
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
        published_at: if form.status == "published" || form.status == "scheduled" {
            form.published_at
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    Post::find_by_id(pool, id)
                        .and_then(|p| p.published_at)
                        .map(|d| d.format("%Y-%m-%dT%H:%M").to_string())
                        .or_else(|| Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string()))
                })
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
    {
        let tag_names_str = form.tag_names.as_deref().unwrap_or("");
        let tag_ids: Vec<i64> = tag_names_str
            .split(',')
            .filter_map(|n| {
                let n = n.trim();
                if n.is_empty() {
                    return None;
                }
                Tag::find_or_create(pool, n).ok()
            })
            .collect();
        let _ = Tag::set_for_content(pool, id, "post", &tag_ids);
    }
    AuditEntry::log(
        pool,
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "update",
        Some("post"),
        Some(id),
        Some(&form.title),
        Some(&form.status),
        None,
    );
    if form.status == "draft" {
        Redirect::to(format!(
            "{}/posts/{}/edit?saved=draft",
            admin_base(slug),
            id
        ))
    } else {
        Redirect::to(format!("{}/posts", admin_base(slug)))
    }
}
