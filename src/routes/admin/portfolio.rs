use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::response::Redirect;
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AuthorUser;
use crate::db::DbPool;
use crate::models::audit::AuditEntry;
use crate::models::category::Category;
use crate::models::portfolio::{PortfolioForm, PortfolioItem};
use crate::models::settings::Setting;
use crate::models::tag::Tag;
use crate::AdminSlug;
use super::admin_base;
use super::save_upload;

// ── Portfolio ──────────────────────────────────────────

#[get("/portfolio?<status>&<page>")]
pub fn portfolio_list(
    _admin: AuthorUser,
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
        "count_scheduled": PortfolioItem::count(pool, Some("scheduled")),
        "admin_slug": slug.0,
        "settings": Setting::all(pool),
    });

    Template::render("admin/portfolio/list", &context)
}

#[get("/portfolio/new")]
pub fn portfolio_new(_admin: AuthorUser, pool: &State<DbPool>, slug: &State<AdminSlug>) -> Template {
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
pub fn portfolio_edit(_admin: AuthorUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Option<Template> {
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
pub fn portfolio_delete(_admin: AuthorUser, pool: &State<DbPool>, slug: &State<AdminSlug>, id: i64) -> Redirect {
    let title = PortfolioItem::find_by_id(pool, id).map(|p| p.title).unwrap_or_default();
    let _ = PortfolioItem::delete(pool, id);
    AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "delete", Some("portfolio"), Some(id), Some(&title), None, None);
    Redirect::to(format!("{}/portfolio", admin_base(slug)))
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
    pub published_at: Option<String>,
    pub category_ids: Option<Vec<i64>>,
    pub tag_names: Option<String>,
    pub image: Option<TempFile<'f>>,
}

#[post("/portfolio/new", data = "<form>")]
pub async fn portfolio_create(
    _admin: AuthorUser,
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
        published_at: if form.status == "published" || form.status == "scheduled" {
            form.published_at.clone().filter(|s| !s.is_empty()).or_else(|| Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string()))
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
            if let Some(ref names) = form.tag_names {
                let tag_ids: Vec<i64> = names.split(',').filter_map(|n| {
                    let n = n.trim();
                    if n.is_empty() { return None; }
                    Tag::find_or_create(pool, n).ok()
                }).collect();
                let _ = Tag::set_for_content(pool, id, "portfolio", &tag_ids);
            }
            AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "create", Some("portfolio"), Some(id), Some(&form.title), Some(&form.status), None);
            if form.status == "draft" {
                Redirect::to(format!("{}/portfolio/{}/edit?saved=draft", admin_base(slug), id))
            } else {
                Redirect::to(format!("{}/portfolio", admin_base(slug)))
            }
        }
        Err(_) => Redirect::to(format!("{}/portfolio", admin_base(slug))),
    }
}

#[post("/portfolio/<id>/edit", data = "<form>")]
pub async fn portfolio_update(
    _admin: AuthorUser,
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
        published_at: if form.status == "published" || form.status == "scheduled" {
            form.published_at.clone().filter(|s| !s.is_empty()).or_else(|| {
                PortfolioItem::find_by_id(pool, id).and_then(|p| p.published_at).map(|d| d.format("%Y-%m-%dT%H:%M").to_string())
                    .or_else(|| Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string()))
            })
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
    {
        let tag_names_str = form.tag_names.as_deref().unwrap_or("");
        let tag_ids: Vec<i64> = tag_names_str.split(',').filter_map(|n| {
            let n = n.trim();
            if n.is_empty() { return None; }
            Tag::find_or_create(pool, n).ok()
        }).collect();
        let _ = Tag::set_for_content(pool, id, "portfolio", &tag_ids);
    }
    AuditEntry::log(pool, Some(_admin.user.id), Some(&_admin.user.display_name), "update", Some("portfolio"), Some(id), Some(&form.title), Some(&form.status), None);
    if form.status == "draft" {
        Redirect::to(format!("{}/portfolio/{}/edit?saved=draft", admin_base(slug), id))
    } else {
        Redirect::to(format!("{}/portfolio", admin_base(slug)))
    }
}
