use rocket::response::content::{RawHtml, RawXml};
use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::db::DbPool;
use crate::models::category::Category;
use crate::models::comment::Comment;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::Setting;
use crate::models::tag::Tag;
use crate::render;
use crate::seo;

// ── Homepage ───────────────────────────────────────────

#[get("/")]
pub fn homepage(pool: &State<DbPool>) -> RawHtml<String> {
    let items = PortfolioItem::published(pool, 100, 0);
    let categories = Category::list(pool, Some("portfolio"));
    let settings = Setting::all(pool);

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = Tag::for_content(pool, item.id, "portfolio");
            let cats = Category::for_content(pool, item.id, "portfolio");
            json!({
                "item": item,
                "tags": tags,
                "categories": cats,
            })
        })
        .collect();

    let context = json!({
        "settings": settings,
        "items": items_with_meta,
        "categories": categories,
        "page_type": "homepage",
        "seo": seo::build_meta(pool, None, None, "/"),
    });

    RawHtml(render::render_page(pool, "homepage", &context))
}

// ── Blog ───────────────────────────────────────────────

#[get("/blog?<page>")]
pub fn blog_list(pool: &State<DbPool>, page: Option<i64>) -> RawHtml<String> {
    let per_page = Setting::get_i64(pool, "blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let posts = Post::published(pool, per_page, offset);
    let total = Post::count(pool, Some("published"));
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let settings = Setting::all(pool);

    let context = json!({
        "settings": settings,
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(pool, Some("Blog"), None, "/blog"),
    });

    RawHtml(render::render_page(pool, "blog_list", &context))
}

#[get("/blog/<slug>")]
pub fn blog_single(pool: &State<DbPool>, slug: &str) -> Option<RawHtml<String>> {
    let post = Post::find_by_slug(pool, slug)?;
    if post.status != "published" {
        return None;
    }

    let categories = Category::for_content(pool, post.id, "post");
    let tags = Tag::for_content(pool, post.id, "post");
    let comments = Comment::for_post(pool, post.id, "post");
    let settings = Setting::all(pool);

    let context = json!({
        "settings": settings,
        "post": post,
        "categories": categories,
        "tags": tags,
        "comments": comments,
        "page_type": "blog_single",
        "seo": seo::build_meta(
            pool,
            post.meta_title.as_deref().or(Some(&post.title)),
            post.meta_description.as_deref(),
            &format!("/blog/{}", post.slug),
        ),
    });

    Some(RawHtml(render::render_page(pool, "blog_single", &context)))
}

#[get("/blog/category/<slug>?<page>")]
pub fn blog_by_category(
    pool: &State<DbPool>,
    slug: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let category = Category::find_by_slug(pool, slug)?;
    let per_page = Setting::get_i64(pool, "blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = Setting::all(pool);

    // Get posts for this category
    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT p.* FROM posts p
             JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'post'
             WHERE cc.category_id = ?1 AND p.status = 'published'
             ORDER BY p.created_at DESC LIMIT ?2 OFFSET ?3",
        )
        .ok()?;

    let posts: Vec<Post> = stmt
        .query_map(
            rusqlite::params![category.id, per_page, offset],
            |row| {
                Ok(Post {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    slug: row.get("slug")?,
                    content_json: row.get("content_json")?,
                    content_html: row.get("content_html")?,
                    excerpt: row.get("excerpt")?,
                    featured_image: row.get("featured_image")?,
                    meta_title: row.get("meta_title")?,
                    meta_description: row.get("meta_description")?,
                    status: row.get("status")?,
                    published_at: row.get("published_at")?,
                    created_at: row.get("created_at")?,
                    updated_at: row.get("updated_at")?,
                })
            },
        )
        .ok()?
        .filter_map(|r| r.ok())
        .collect();

    let context = json!({
        "settings": settings,
        "posts": posts,
        "category": category,
        "current_page": current_page,
        "page_type": "blog_list",
        "seo": seo::build_meta(pool, Some(&category.name), None, &format!("/blog/category/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "blog_list", &context)))
}

// ── Portfolio ──────────────────────────────────────────

#[get("/portfolio")]
pub fn portfolio_grid(pool: &State<DbPool>) -> RawHtml<String> {
    let items = PortfolioItem::published(pool, 100, 0);
    let categories = Category::list(pool, Some("portfolio"));
    let settings = Setting::all(pool);

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = Tag::for_content(pool, item.id, "portfolio");
            let cats = Category::for_content(pool, item.id, "portfolio");
            json!({
                "item": item,
                "tags": tags,
                "categories": cats,
            })
        })
        .collect();

    let context = json!({
        "settings": settings,
        "items": items_with_meta,
        "categories": categories,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(pool, Some("Portfolio"), None, "/portfolio"),
    });

    RawHtml(render::render_page(pool, "portfolio_grid", &context))
}

#[get("/portfolio/<slug>")]
pub fn portfolio_single(pool: &State<DbPool>, slug: &str) -> Option<RawHtml<String>> {
    let item = PortfolioItem::find_by_slug(pool, slug)?;
    if item.status != "published" {
        return None;
    }

    let categories = Category::for_content(pool, item.id, "portfolio");
    let tags = Tag::for_content(pool, item.id, "portfolio");
    let comments_enabled = Setting::get_bool(pool, "comments_on_portfolio");
    let comments = if comments_enabled {
        Comment::for_post(pool, item.id, "portfolio")
    } else {
        vec![]
    };
    let settings = Setting::all(pool);

    let context = json!({
        "settings": settings,
        "item": item,
        "categories": categories,
        "tags": tags,
        "comments": comments,
        "comments_enabled": comments_enabled,
        "page_type": "portfolio_single",
        "seo": seo::build_meta(
            pool,
            item.meta_title.as_deref().or(Some(&item.title)),
            item.meta_description.as_deref(),
            &format!("/portfolio/{}", item.slug),
        ),
    });

    Some(RawHtml(render::render_page(pool, "portfolio_single", &context)))
}

#[get("/portfolio/category/<slug>")]
pub fn portfolio_by_category(pool: &State<DbPool>, slug: &str) -> Option<RawHtml<String>> {
    let category = Category::find_by_slug(pool, slug)?;
    let items = PortfolioItem::by_category(pool, slug, 100, 0);
    let categories = Category::list(pool, Some("portfolio"));
    let settings = Setting::all(pool);

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = Tag::for_content(pool, item.id, "portfolio");
            let cats = Category::for_content(pool, item.id, "portfolio");
            json!({
                "item": item,
                "tags": tags,
                "categories": cats,
            })
        })
        .collect();

    let context = json!({
        "settings": settings,
        "items": items_with_meta,
        "categories": categories,
        "active_category": category,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(pool, Some(&category.name), None, &format!("/portfolio/category/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "portfolio_grid", &context)))
}

// ── RSS Feed ───────────────────────────────────────────

#[get("/feed")]
pub fn rss_feed(pool: &State<DbPool>) -> RawXml<String> {
    RawXml(crate::rss::generate_feed(pool))
}

// ── Sitemap ────────────────────────────────────────────

#[get("/sitemap.xml")]
pub fn sitemap(pool: &State<DbPool>) -> RawXml<String> {
    RawXml(seo::generate_sitemap(pool))
}

// ── Robots.txt ─────────────────────────────────────────

#[get("/robots.txt")]
pub fn robots(pool: &State<DbPool>) -> String {
    let mut content = Setting::get_or(pool, "seo_robots_txt", "User-agent: *\nAllow: /");
    let site_url = Setting::get_or(pool, "site_url", "http://localhost:8000");
    content.push_str(&format!("\nSitemap: {}/sitemap.xml", site_url));
    content
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        homepage,
        blog_list,
        blog_single,
        blog_by_category,
        portfolio_grid,
        portfolio_single,
        portfolio_by_category,
        rss_feed,
        sitemap,
        robots,
    ]
}
