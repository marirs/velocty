use std::sync::Arc;

use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use super::admin_base;
use super::save_upload;
use crate::models::post::PostForm;
use crate::security::auth::{AuthorUser, EditorUser};
use crate::store::Store;
use crate::AdminSlug;

// ── Posts ───────────────────────────────────────────────

#[get("/posts?<status>&<page>")]
pub fn posts_list(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    status: Option<String>,
    page: Option<i64>,
) -> Template {
    let per_page = 20i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let posts = store.post_list(status.as_deref(), per_page, offset);
    let total = store.post_count(status.as_deref());
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "page_title": "Journal",
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "status_filter": status,
        "count_all": store.post_count(None),
        "count_published": store.post_count(Some("published")),
        "count_draft": store.post_count(Some("draft")),
        "count_scheduled": store.post_count(Some("scheduled")),
        "count_archived": store.post_count(Some("archived")),
        "admin_slug": slug.get(),
        "settings": store.setting_all(),
    });

    Template::render("admin/posts/list", &context)
}

#[get("/posts/new")]
pub fn posts_new(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
) -> Template {
    let categories = store.category_list(Some("post"));
    let tags = store.tag_list();

    let ai_enabled = crate::ai::is_enabled(&**store.inner());
    let ai_has_vision = crate::ai::has_vision_provider(&**store.inner());
    let context = json!({
        "page_title": "New Post",
        "admin_slug": slug.get(),
        "categories": categories,
        "tags": tags,
        "settings": store.setting_all(),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Template::render("admin/posts/edit", &context)
}

#[get("/posts/<id>/edit")]
pub fn posts_edit(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Option<Template> {
    let post = store.post_find_by_id(id)?;
    let categories = store.category_list(Some("post"));
    let tags = store.tag_list();
    let post_categories = store.category_for_content(id, "post");
    let post_tags = store.tag_for_content(id, "post");

    let ai_enabled = crate::ai::is_enabled(&**store.inner());
    let ai_has_vision = crate::ai::has_vision_provider(&**store.inner());
    let context = json!({
        "page_title": "Edit Post",
        "post": post,
        "categories": categories,
        "tags": tags,
        "post_categories": post_categories.iter().map(|c| c.id).collect::<Vec<_>>(),
        "post_tags": post_tags.iter().map(|t| t.id).collect::<Vec<_>>(),
        "admin_slug": slug.get(),
        "settings": store.setting_all(),
        "ai_enabled": ai_enabled,
        "ai_has_vision": ai_has_vision,
    });

    Some(Template::render("admin/posts/edit", &context))
}

#[post("/posts/<id>/delete")]
pub fn posts_delete(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
) -> Redirect {
    let title = store
        .post_find_by_id(id)
        .map(|p| p.title)
        .unwrap_or_default();
    let _ = store.post_delete(id);
    store.search_remove_item("post", id);
    store.audit_log(
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
    pub uploaded_featured_path: Option<String>,
}

#[post("/posts/new", data = "<form>")]
pub async fn posts_create(
    _admin: AuthorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    mut form: Form<PostFormData<'_>>,
) -> Redirect {
    let featured = if form
        .uploaded_featured_path
        .as_ref()
        .is_some_and(|p| !p.is_empty())
    {
        Some(form.uploaded_featured_path.clone().unwrap())
    } else {
        match form.featured_image.as_mut() {
            Some(f) if f.len() > 0 => {
                if !super::is_allowed_image(f, &**store.inner()) {
                    return Redirect::to(format!("{}/posts/new", admin_base(slug)));
                }
                save_upload(f, "post", &**store.inner()).await
            }
            _ => None,
        }
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
        status: "pending".to_string(), // placeholder, overwritten below
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
    let final_status = super::resolve_status(&form.status, &post_form.published_at);
    let post_form = PostForm {
        status: final_status.clone(),
        ..post_form
    };

    match store.post_create(&post_form) {
        Ok(id) => {
            // Auto-compute SEO score
            {
                let seo_input = crate::seo::audit::SeoInput {
                    title: &form.title,
                    slug: &form.slug,
                    meta_title: form.meta_title.as_deref().unwrap_or(""),
                    meta_description: form.meta_description.as_deref().unwrap_or(""),
                    body_html: &form.content_html,
                    featured_image: post_form.featured_image.as_deref().unwrap_or(""),
                    content_type: "post",
                };
                let audit = crate::seo::audit::compute_score(&seo_input);
                let _ = store.post_update_seo_score(
                    id,
                    audit.score,
                    &crate::seo::audit::issues_to_json(&audit.issues),
                );
            }
            if let Some(ref cat_ids) = form.category_ids {
                let _ = store.category_set_for_content(id, "post", cat_ids);
            }
            if let Some(ref names) = form.tag_names {
                let tag_ids: Vec<i64> = names
                    .split(',')
                    .filter_map(|n| {
                        let n = n.trim();
                        if n.is_empty() {
                            return None;
                        }
                        store.tag_find_or_create(n).ok()
                    })
                    .collect();
                let _ = store.tag_set_for_content(id, "post", &tag_ids);
            }
            // Update search index
            store.search_upsert_item(
                "post",
                id,
                &form.title,
                &form.content_html,
                &form.slug,
                post_form.featured_image.as_deref(),
                post_form.published_at.as_deref(),
                final_status == "published",
            );
            store.audit_log(
                Some(_admin.user.id),
                Some(&_admin.user.display_name),
                "create",
                Some("post"),
                Some(id),
                Some(&form.title),
                Some(&final_status),
                None,
            );
            if final_status == "draft" {
                Redirect::to(format!(
                    "{}/posts/{}/edit?saved=draft",
                    admin_base(slug),
                    id
                ))
            } else if final_status == "scheduled" {
                Redirect::to(format!("{}/posts?saved=scheduled", admin_base(slug)))
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
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    id: i64,
    mut form: Form<PostFormData<'_>>,
) -> Redirect {
    let featured = if form
        .uploaded_featured_path
        .as_ref()
        .is_some_and(|p| !p.is_empty())
    {
        Some(form.uploaded_featured_path.clone().unwrap())
    } else {
        match form.featured_image.as_mut() {
            Some(f) if f.len() > 0 => {
                if !super::is_allowed_image(f, &**store.inner()) {
                    return Redirect::to(format!("{}/posts/{}/edit", admin_base(slug), id));
                }
                save_upload(f, "post", &**store.inner()).await
            }
            _ => store.post_find_by_id(id).and_then(|p| p.featured_image),
        }
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
        status: "pending".to_string(), // placeholder, overwritten below
        published_at: if form.status == "published" || form.status == "scheduled" {
            form.published_at
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    store
                        .post_find_by_id(id)
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
    let final_status = super::resolve_status(&form.status, &post_form.published_at);
    let post_form = PostForm {
        status: final_status.clone(),
        ..post_form
    };

    let _ = store.post_update(id, &post_form);
    // Auto-compute SEO score
    {
        let seo_input = crate::seo::audit::SeoInput {
            title: &form.title,
            slug: &form.slug,
            meta_title: form.meta_title.as_deref().unwrap_or(""),
            meta_description: form.meta_description.as_deref().unwrap_or(""),
            body_html: &form.content_html,
            featured_image: post_form.featured_image.as_deref().unwrap_or(""),
            content_type: "post",
        };
        let audit = crate::seo::audit::compute_score(&seo_input);
        let _ = store.post_update_seo_score(
            id,
            audit.score,
            &crate::seo::audit::issues_to_json(&audit.issues),
        );
    }
    if let Some(ref cat_ids) = form.category_ids {
        let _ = store.category_set_for_content(id, "post", cat_ids);
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
                store.tag_find_or_create(n).ok()
            })
            .collect();
        let _ = store.tag_set_for_content(id, "post", &tag_ids);
    }
    // Update search index
    store.search_upsert_item(
        "post",
        id,
        &form.title,
        &form.content_html,
        &form.slug,
        post_form.featured_image.as_deref(),
        post_form.published_at.as_deref(),
        final_status == "published",
    );
    store.audit_log(
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "update",
        Some("post"),
        Some(id),
        Some(&form.title),
        Some(&final_status),
        None,
    );
    if final_status == "draft" {
        Redirect::to(format!(
            "{}/posts/{}/edit?saved=draft",
            admin_base(slug),
            id
        ))
    } else if final_status == "scheduled" {
        Redirect::to(format!("{}/posts?saved=scheduled", admin_base(slug)))
    } else {
        Redirect::to(format!("{}/posts", admin_base(slug)))
    }
}
