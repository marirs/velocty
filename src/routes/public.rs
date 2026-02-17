use rocket::response::content::{RawHtml, RawXml};
use rocket::State;
use serde_json::json;

use crate::db::DbPool;
use crate::models::category::Category;
use crate::models::comment::Comment;
use crate::models::portfolio::PortfolioItem;
use crate::models::post::Post;
use crate::models::settings::{Setting, SettingsCache};
use crate::models::tag::Tag;
use crate::render;
use crate::seo;

// ── Dynamic catch-all router ─────────────────────────────
// Reads slugs and enabled flags from the in-memory SettingsCache
// and dispatches to the right handler. No restart needed on settings change.

#[get("/?<page>", rank = 89)]
pub fn dynamic_route_index(
    pool: &State<DbPool>,
    cache: &State<SettingsCache>,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    dispatch_root(pool, cache, None, page)
}

#[get("/<first>/<rest..>?<page>", rank = 90)]
pub fn dynamic_route_sub(
    pool: &State<DbPool>,
    cache: &State<SettingsCache>,
    first: &str,
    rest: std::path::PathBuf,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let rest_str = rest.to_string_lossy();
    dispatch_root(pool, cache, Some(&format!("{}/{}", first, rest_str)), page)
}

#[get("/<first>?<page>", rank = 91)]
pub fn dynamic_route_root(
    pool: &State<DbPool>,
    cache: &State<SettingsCache>,
    first: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    dispatch_root(pool, cache, Some(first), page)
}

/// Core dispatcher: resolves the full path against cached slugs and enabled flags.
/// `path` is None for "/", or Some("journal"), Some("journal/my-post"), Some("category/foo"), etc.
fn dispatch_root(
    pool: &DbPool,
    cache: &SettingsCache,
    path: Option<&str>,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let blog_slug = cache.get_or("blog_slug", "journal");
    let portfolio_slug = cache.get_or("portfolio_slug", "portfolio");
    let journal_enabled = cache.get_or("journal_enabled", "true") != "false";
    let portfolio_enabled = cache.get_or("portfolio_enabled", "false") == "true";

    let path = path.unwrap_or("");
    let path = path.trim_end_matches('/');

    // Skip reserved paths so admin, static files, API, etc. are never intercepted
    let admin_slug = cache.get_or("admin_slug", "admin");
    let first_segment = path.split('/').next().unwrap_or("");
    let reserved = [admin_slug.as_str(), "static", "uploads", "api", "archives", "rss", "sitemap.xml", "robots.txt", "super", ".well-known", "favicon.ico"];
    if reserved.contains(&first_segment) {
        return None;
    }

    // Try blog: strip blog_slug prefix
    if journal_enabled {
        if let Some(rest) = strip_slug_prefix(path, &blog_slug) {
            return dispatch_blog(pool, rest, page);
        }
    }

    // Try portfolio: strip portfolio_slug prefix
    if portfolio_enabled {
        if let Some(rest) = strip_slug_prefix(path, &portfolio_slug) {
            return dispatch_portfolio(pool, rest, page);
        }
    }

    None
}

/// If slug is empty, the feature claims "/" and all sub-paths → returns Some("") or Some(rest).
/// If slug is non-empty, checks if path starts with that slug → returns Some(rest-after-slug) or None.
fn strip_slug_prefix<'a>(path: &'a str, slug: &str) -> Option<&'a str> {
    if slug.is_empty() {
        // Empty slug means this feature owns "/"
        return Some(path);
    }
    if path == slug {
        return Some("");
    }
    if let Some(rest) = path.strip_prefix(slug) {
        if rest.starts_with('/') {
            return Some(&rest[1..]);
        }
    }
    None
}

fn dispatch_blog(pool: &DbPool, rest: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    if rest.is_empty() {
        return Some(do_blog_list(pool, page));
    }
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    match parts.as_slice() {
        ["category", slug] => do_blog_by_category(pool, slug, page),
        ["tag", slug] => do_blog_by_tag(pool, slug, page),
        [slug] => do_blog_single(pool, slug),
        _ => None,
    }
}

fn dispatch_portfolio(pool: &DbPool, rest: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    if rest.is_empty() {
        return Some(do_portfolio_grid(pool, page));
    }
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    match parts.as_slice() {
        ["category", slug] => do_portfolio_by_category(pool, slug, page),
        ["tag", slug] => do_portfolio_by_tag(pool, slug, page),
        [slug] => do_portfolio_single(pool, slug),
        _ => None,
    }
}

// ── Archives ──────────────────────────────────────────

#[get("/archives")]
pub fn archives(pool: &State<DbPool>) -> RawHtml<String> {
    let settings = Setting::all(pool);

    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return RawHtml(render::render_page(pool, "archives", &json!({
                "settings": settings,
                "archives": [],
                "page_type": "archives",
                "seo": seo::build_meta(pool, Some("Archives"), None, "/archives"),
            })));
        }
    };

    let mut stmt = match conn.prepare(
        "SELECT strftime('%Y', published_at) as year, strftime('%m', published_at) as month,
                COUNT(*) as count
         FROM posts WHERE status = 'published' AND published_at IS NOT NULL
         GROUP BY year, month ORDER BY year DESC, month DESC",
    ) {
        Ok(s) => s,
        Err(_) => {
            return RawHtml(render::render_page(pool, "archives", &json!({
                "settings": settings,
                "archives": [],
                "page_type": "archives",
                "seo": seo::build_meta(pool, Some("Archives"), None, "/archives"),
            })));
        }
    };

    let archive_entries: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            let year: String = row.get(0)?;
            let month: String = row.get(1)?;
            let count: i64 = row.get(2)?;
            Ok(json!({
                "year": year,
                "month": month,
                "count": count,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let context = json!({
        "settings": settings,
        "archives": archive_entries,
        "page_type": "archives",
        "seo": seo::build_meta(pool, Some("Archives"), None, "/archives"),
    });

    RawHtml(render::render_page(pool, "archives", &context))
}

#[get("/archives/<year>/<month>?<page>")]
pub fn archives_month(
    pool: &State<DbPool>,
    year: &str,
    month: &str,
    page: Option<i64>,
) -> RawHtml<String> {
    let per_page = Setting::get_i64(pool, "blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = Setting::all(pool);

    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return RawHtml(render::render_page(pool, "blog_list", &json!({
                "settings": settings,
                "posts": [],
                "current_page": 1,
                "total_pages": 1,
                "page_type": "blog_list",
                "seo": seo::build_meta(pool, Some("Archives"), None, "/archives"),
            })));
        }
    };

    let mut stmt = match conn.prepare(
        "SELECT * FROM posts
         WHERE status = 'published'
           AND strftime('%Y', published_at) = ?1
           AND strftime('%m', published_at) = ?2
         ORDER BY published_at DESC LIMIT ?3 OFFSET ?4",
    ) {
        Ok(s) => s,
        Err(_) => {
            return RawHtml(render::render_page(pool, "blog_list", &json!({
                "settings": settings,
                "posts": [],
                "current_page": 1,
                "total_pages": 1,
                "page_type": "blog_list",
                "seo": seo::build_meta(pool, Some("Archives"), None, "/archives"),
            })));
        }
    };

    let posts: Vec<Post> = stmt
        .query_map(
            rusqlite::params![year, month, per_page, offset],
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
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts
             WHERE status = 'published'
               AND strftime('%Y', published_at) = ?1
               AND strftime('%m', published_at) = ?2",
            rusqlite::params![year, month],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let title = format!("Archives: {}/{}", year, month);
    let context = json!({
        "settings": settings,
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "archive_year": year,
        "archive_month": month,
        "page_type": "blog_list",
        "seo": seo::build_meta(pool, Some(&title), None, &format!("/archives/{}/{}", year, month)),
    });

    RawHtml(render::render_page(pool, "blog_list", &context))
}

// ── RSS Feed ───────────────────────────────────────────

#[get("/feed")]
pub fn rss_feed(pool: &State<DbPool>) -> RawXml<String> {
    RawXml(crate::rss::generate_feed(pool))
}

// ── Sitemap ────────────────────────────────────────────

#[get("/sitemap.xml")]
pub fn sitemap(pool: &State<DbPool>) -> Option<RawXml<String>> {
    seo::sitemap::generate_sitemap(pool).map(RawXml)
}

// ── Robots.txt ─────────────────────────────────────────

#[get("/robots.txt")]
pub fn robots(pool: &State<DbPool>) -> String {
    seo::sitemap::generate_robots(pool)
}

// ── Privacy Policy ─────────────────────────────────────

#[get("/privacy")]
pub fn privacy_page(pool: &State<DbPool>) -> Option<RawHtml<String>> {
    let settings = Setting::all(pool);
    if settings.get("privacy_policy_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let html_body = settings
        .get("privacy_policy_content")
        .cloned()
        .unwrap_or_default();
    let page_html = render::render_legal_page(pool, &settings, "Privacy Policy", &html_body);
    Some(RawHtml(page_html))
}

// ── Terms of Use ──────────────────────────────────────

#[get("/terms")]
pub fn terms_page(pool: &State<DbPool>) -> Option<RawHtml<String>> {
    let settings = Setting::all(pool);
    if settings.get("terms_of_use_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let html_body = settings
        .get("terms_of_use_content")
        .cloned()
        .unwrap_or_default();
    let page_html = render::render_legal_page(pool, &settings, "Terms of Use", &html_body);
    Some(RawHtml(page_html))
}

pub fn root_routes() -> Vec<rocket::Route> {
    routes![
        dynamic_route_index,
        dynamic_route_sub,
        dynamic_route_root,
        archives,
        archives_month,
        rss_feed,
        sitemap,
        robots,
        privacy_page,
        terms_page,
    ]
}

// ── Internal dispatch functions (called by catch-all) ────

fn do_blog_list(pool: &DbPool, page: Option<i64>) -> RawHtml<String> {
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

fn do_blog_single(pool: &DbPool, slug: &str) -> Option<RawHtml<String>> {
    let post = Post::find_by_slug(pool, slug)?;
    if post.status != "published" {
        return None;
    }

    let categories = Category::for_content(pool, post.id, "post");
    let tags = Tag::for_content(pool, post.id, "post");
    let settings = Setting::all(pool);
    let comments_enabled = settings.get("comments_enabled").map(|v| v.as_str()) == Some("true")
        && settings.get("comments_on_blog").map(|v| v.as_str()) == Some("true");
    let comments = if comments_enabled {
        Comment::for_post(pool, post.id, "post")
    } else {
        vec![]
    };

    let prev_post = post.published_at.as_ref().and_then(|pa| Post::prev_published(pool, pa));
    let next_post = post.published_at.as_ref().and_then(|pa| Post::next_published(pool, pa));

    let mut context = json!({
        "settings": settings,
        "post": post,
        "categories": categories,
        "tags": tags,
        "comments": comments,
        "comments_enabled": comments_enabled,
        "page_type": "blog_single",
        "seo": seo::build_meta(
            pool,
            post.meta_title.as_deref().or(Some(&post.title)),
            post.meta_description.as_deref(),
            &format!("/blog/{}", post.slug),
        ),
    });

    if let Some(prev) = prev_post {
        context["prev_post"] = json!({"title": prev.title, "slug": prev.slug});
    }
    if let Some(next) = next_post {
        context["next_post"] = json!({"title": next.title, "slug": next.slug});
    }

    Some(RawHtml(render::render_page(pool, "blog_single", &context)))
}

fn do_blog_by_category(pool: &DbPool, slug: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    let category = Category::find_by_slug(pool, slug)?;
    let per_page = Setting::get_i64(pool, "blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = Setting::all(pool);

    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT p.* FROM posts p
             JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'post'
             WHERE cc.category_id = ?1 AND p.status = 'published'
             ORDER BY p.published_at DESC LIMIT ?2 OFFSET ?3",
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

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts p
             JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'post'
             WHERE cc.category_id = ?1 AND p.status = 'published'",
            rusqlite::params![category.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "settings": settings,
        "posts": posts,
        "active_category": category,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(pool, Some(&category.name), None, &format!("/blog/category/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "blog_list", &context)))
}

fn do_blog_by_tag(pool: &DbPool, slug: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    let tag = Tag::find_by_slug(pool, slug)?;
    let per_page = Setting::get_i64(pool, "blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = Setting::all(pool);

    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT p.* FROM posts p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'post'
             WHERE ct.tag_id = ?1 AND p.status = 'published'
             ORDER BY p.published_at DESC LIMIT ?2 OFFSET ?3",
        )
        .ok()?;

    let posts: Vec<Post> = stmt
        .query_map(
            rusqlite::params![tag.id, per_page, offset],
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

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'post'
             WHERE ct.tag_id = ?1 AND p.status = 'published'",
            rusqlite::params![tag.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "settings": settings,
        "posts": posts,
        "active_tag": tag,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(pool, Some(&tag.name), None, &format!("/blog/tag/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "blog_list", &context)))
}

fn do_portfolio_grid(pool: &DbPool, page: Option<i64>) -> RawHtml<String> {
    let per_page = Setting::get_i64(pool, "portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let items = PortfolioItem::published(pool, per_page, offset);
    let total = PortfolioItem::count(pool, Some("published"));
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
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
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(pool, Some("Portfolio"), None, "/portfolio"),
    });

    RawHtml(render::render_page(pool, "portfolio_grid", &context))
}

fn do_portfolio_single(pool: &DbPool, slug: &str) -> Option<RawHtml<String>> {
    let item = PortfolioItem::find_by_slug(pool, slug)?;
    if item.status != "published" {
        return None;
    }

    let categories = Category::for_content(pool, item.id, "portfolio");
    let tags = Tag::for_content(pool, item.id, "portfolio");
    let settings = Setting::all(pool);
    let comments_enabled = settings.get("comments_enabled").map(|v| v.as_str()) == Some("true")
        && settings.get("comments_on_portfolio").map(|v| v.as_str()) == Some("true");
    let comments = if comments_enabled {
        Comment::for_post(pool, item.id, "portfolio")
    } else {
        vec![]
    };

    let any_commerce = [
        "commerce_paypal_enabled", "commerce_stripe_enabled", "commerce_payoneer_enabled",
        "commerce_2checkout_enabled", "commerce_square_enabled", "commerce_razorpay_enabled",
        "commerce_mollie_enabled",
    ].iter().any(|k| settings.get(*k).map(|v| v.as_str()) == Some("true"));

    let context = json!({
        "settings": settings,
        "item": item,
        "categories": categories,
        "tags": tags,
        "comments": comments,
        "comments_enabled": comments_enabled,
        "page_type": "portfolio_single",
        "commerce_enabled": any_commerce && item.sell_enabled && item.price.unwrap_or(0.0) > 0.0,
        "seo": seo::build_meta(
            pool,
            item.meta_title.as_deref().or(Some(&item.title)),
            item.meta_description.as_deref(),
            &format!("/portfolio/{}", item.slug),
        ),
    });

    Some(RawHtml(render::render_page(pool, "portfolio_single", &context)))
}

fn do_portfolio_by_category(pool: &DbPool, slug: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    let category = Category::find_by_slug(pool, slug)?;
    let per_page = Setting::get_i64(pool, "portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let items = PortfolioItem::by_category(pool, slug, per_page, offset);
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
        "current_page": current_page,
        "total_pages": ((PortfolioItem::count(pool, Some("published")) as f64 / per_page as f64).ceil() as i64),
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(pool, Some(&category.name), None, &format!("/portfolio/category/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "portfolio_grid", &context)))
}

fn do_portfolio_by_tag(pool: &DbPool, slug: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    let tag = Tag::find_by_slug(pool, slug)?;
    let per_page = Setting::get_i64(pool, "portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let categories = Category::list(pool, Some("portfolio"));
    let settings = Setting::all(pool);

    let conn = pool.get().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT p.* FROM portfolio p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'portfolio'
             WHERE ct.tag_id = ?1 AND p.status = 'published'
             ORDER BY p.created_at DESC LIMIT ?2 OFFSET ?3",
        )
        .ok()?;

    let items: Vec<PortfolioItem> = stmt
        .query_map(
            rusqlite::params![tag.id, per_page, offset],
            |row| {
                let sell_raw: i64 = row.get("sell_enabled")?;
                Ok(PortfolioItem {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    slug: row.get("slug")?,
                    description_json: row.get("description_json")?,
                    description_html: row.get("description_html")?,
                    image_path: row.get("image_path")?,
                    thumbnail_path: row.get("thumbnail_path")?,
                    meta_title: row.get("meta_title")?,
                    meta_description: row.get("meta_description")?,
                    sell_enabled: sell_raw != 0,
                    price: row.get("price")?,
                    purchase_note: row.get::<_, Option<String>>("purchase_note")?.unwrap_or_default(),
                    payment_provider: row.get::<_, Option<String>>("payment_provider")?.unwrap_or_default(),
                    download_file_path: row.get::<_, Option<String>>("download_file_path")?.unwrap_or_default(),
                    likes: row.get("likes")?,
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

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM portfolio p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'portfolio'
             WHERE ct.tag_id = ?1 AND p.status = 'published'",
            rusqlite::params![tag.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "settings": settings,
        "items": items_with_meta,
        "categories": categories,
        "active_tag": tag,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(pool, Some(&tag.name), None, &format!("/portfolio/tag/{}", slug)),
    });

    Some(RawHtml(render::render_page(pool, "portfolio_grid", &context)))
}
