use rocket::form::Form;
use rocket::http::{ContentType, CookieJar, Header, Status};
use rocket::response::content::{RawHtml, RawXml};
use rocket::response::{self, Responder, Response};
use rocket::{Request, State};
use serde_json::json;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use crate::image_proxy;
use crate::models::settings::SettingsCache;
use crate::render;
use crate::security::auth;
use crate::security::auth::ClientIp;
use crate::seo;
use crate::store::Store;

/// Response type for serving files with custom headers (caching, MIME, etc.)
pub struct FileResponse {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub cache_control: String,
    pub content_security_policy: Option<String>,
}

impl<'r> Responder<'r, 'static> for FileResponse {
    fn respond_to(self, _req: &'r Request<'_>) -> response::Result<'static> {
        let ct = ContentType::parse_flexible(&self.content_type).unwrap_or(ContentType::Binary);
        let mut resp = Response::build();
        resp.header(ct)
            .header(Header::new("Cache-Control", self.cache_control))
            .header(Header::new("X-Content-Type-Options", "nosniff"));
        if let Some(csp) = self.content_security_policy {
            resp.header(Header::new("Content-Security-Policy", csp));
        }
        resp.sized_body(self.bytes.len(), Cursor::new(self.bytes))
            .ok()
    }
}

// ── Dynamic catch-all router ─────────────────────────────
// Reads slugs and enabled flags from the in-memory SettingsCache
// and dispatches to the right handler. No restart needed on settings change.

#[get("/?<page>", rank = 89)]
pub fn dynamic_route_index(
    store: &State<Arc<dyn Store>>,
    cache: &State<SettingsCache>,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    dispatch_root(&**store.inner(), cache, None, page)
}

#[get("/<first>/<rest..>?<page>", rank = 90)]
pub fn dynamic_route_sub(
    store: &State<Arc<dyn Store>>,
    cache: &State<SettingsCache>,
    first: &str,
    rest: std::path::PathBuf,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let rest_str = rest.to_string_lossy();
    dispatch_root(
        &**store.inner(),
        cache,
        Some(&format!("{}/{}", first, rest_str)),
        page,
    )
}

#[get("/<first>?<page>", rank = 91)]
pub fn dynamic_route_root(
    store: &State<Arc<dyn Store>>,
    cache: &State<SettingsCache>,
    first: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    dispatch_root(&**store.inner(), cache, Some(first), page)
}

/// Core dispatcher: resolves the full path against cached slugs and enabled flags.
/// `path` is None for "/", or Some("journal"), Some("journal/my-post"), Some("category/foo"), etc.
fn dispatch_root(
    store: &dyn Store,
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
    let reserved = [
        admin_slug.as_str(),
        "__adm",
        "static",
        "uploads",
        "api",
        "archives",
        "rss",
        "feed",
        "download",
        "sitemap.xml",
        "robots.txt",
        "super",
        ".well-known",
        "favicon.ico",
        "contact",
        "search",
        "privacy",
        "terms",
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
        "change-password",
    ];
    if reserved.contains(&first_segment) {
        return None;
    }

    // Try blog: strip blog_slug prefix
    if journal_enabled {
        if let Some(rest) = strip_slug_prefix(path, &blog_slug) {
            return dispatch_blog(store, rest, page);
        }
    }

    // Try portfolio: strip portfolio_slug prefix
    if portfolio_enabled {
        if let Some(rest) = strip_slug_prefix(path, &portfolio_slug) {
            return dispatch_portfolio(store, rest, page);
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
        if let Some(stripped) = rest.strip_prefix('/') {
            return Some(stripped);
        }
    }
    None
}

fn dispatch_blog(store: &dyn Store, rest: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    if rest.is_empty() {
        return Some(do_blog_list(store, page));
    }
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    match parts.as_slice() {
        ["category", slug] => do_blog_by_category(store, slug, page),
        ["tag", slug] => do_blog_by_tag(store, slug, page),
        [slug] => do_blog_single(store, slug),
        _ => None,
    }
}

fn dispatch_portfolio(store: &dyn Store, rest: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    if rest.is_empty() {
        return Some(do_portfolio_grid(store, page));
    }
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    match parts.as_slice() {
        ["category", slug] => do_portfolio_by_category(store, slug, page),
        ["tag", slug] => do_portfolio_by_tag(store, slug, page),
        [slug] => do_portfolio_single(store, slug),
        _ => None,
    }
}

/// Portfolio nav categories for the sidebar — called by non-portfolio routes
/// so the sidebar always shows the portfolio category tree.
fn nav_categories(store: &dyn Store) -> Vec<crate::models::category::Category> {
    store.category_list_nav_visible(Some("portfolio"))
}

/// Journal nav categories for the sidebar.
fn nav_journal_categories(store: &dyn Store) -> Vec<crate::models::category::Category> {
    store.category_list_nav_visible(Some("post"))
}

// ── Archives ──────────────────────────────────────────

#[get("/archives")]
pub fn archives(store: &State<Arc<dyn Store>>) -> RawHtml<String> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();

    let archive_entries: Vec<serde_json::Value> = s
        .post_archives()
        .into_iter()
        .map(|(year, month, count)| json!({ "year": year, "month": month, "count": count }))
        .collect();

    let context = json!({
        "settings": settings,
        "nav_categories": nav_categories(s),
        "nav_journal_categories": nav_journal_categories(s),
        "archives": archive_entries,
        "page_type": "archives",
        "seo": seo::build_meta(s, Some("Archives"), None, "/archives"),
    });

    RawHtml(render::render_page(s, "archives", &context))
}

#[get("/archives/<year>/<month>?<page>")]
pub fn archives_month(
    store: &State<Arc<dyn Store>>,
    year: &str,
    month: &str,
    page: Option<i64>,
) -> RawHtml<String> {
    let s: &dyn Store = &**store.inner();
    let per_page = s.setting_get_i64("blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = s.setting_all();

    let posts = s.post_by_year_month(year, month, per_page, offset);
    let total = s.post_count_by_year_month(year, month);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let title = format!("Archives: {}/{}", year, month);
    let context = json!({
        "settings": settings,
        "nav_categories": nav_categories(s),
        "nav_journal_categories": nav_journal_categories(s),
        "posts": posts,
        "current_page": current_page,
        "total_pages": total_pages,
        "archive_year": year,
        "archive_month": month,
        "page_type": "blog_list",
        "seo": seo::build_meta(s, Some(&title), None, &format!("/archives/{}/{}", year, month)),
    });

    RawHtml(render::render_page(s, "blog_list", &context))
}

// ── RSS Feed ───────────────────────────────────────────

#[get("/feed")]
pub fn rss_feed(store: &State<Arc<dyn Store>>) -> RawXml<String> {
    RawXml(crate::rss::generate_feed(&**store.inner()))
}

// ── Sitemap ────────────────────────────────────────────

#[get("/sitemap.xml")]
pub fn sitemap(store: &State<Arc<dyn Store>>) -> Option<RawXml<String>> {
    seo::sitemap::generate_sitemap(&**store.inner()).map(RawXml)
}

// ── Robots.txt ─────────────────────────────────────────

#[get("/robots.txt")]
pub fn robots(store: &State<Arc<dyn Store>>) -> String {
    seo::sitemap::generate_robots(&**store.inner())
}

// ── Privacy Policy ─────────────────────────────────────

#[get("/privacy")]
pub fn privacy_page(store: &State<Arc<dyn Store>>) -> Option<RawHtml<String>> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    if settings.get("privacy_policy_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let html_body = settings
        .get("privacy_policy_content")
        .cloned()
        .unwrap_or_default();
    let page_html = render::render_legal_page(s, &settings, "Privacy Policy", &html_body);
    Some(RawHtml(page_html))
}

// ── Terms of Use ──────────────────────────────────────

#[get("/terms")]
pub fn terms_page(store: &State<Arc<dyn Store>>) -> Option<RawHtml<String>> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    if settings.get("terms_of_use_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let html_body = settings
        .get("terms_of_use_content")
        .cloned()
        .unwrap_or_default();
    let page_html = render::render_legal_page(s, &settings, "Terms of Use", &html_body);
    Some(RawHtml(page_html))
}

// ── Contact Page ──────────────────────────────────────

#[get("/contact")]
pub fn contact_page(store: &State<Arc<dyn Store>>) -> Option<RawHtml<String>> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    if settings.get("contact_page_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    let page_html = render::render_contact_page(s, &settings, None);
    Some(RawHtml(page_html))
}

#[post("/contact", data = "<form>")]
pub fn contact_submit(
    store: &State<Arc<dyn Store>>,
    client_ip: ClientIp,
    form: Form<HashMap<String, String>>,
) -> Option<RawHtml<String>> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    if settings.get("contact_page_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }
    if settings.get("contact_form_enabled").map(|v| v.as_str()) != Some("true") {
        return None;
    }

    // Rate limit: max 5 submissions per IP per 15 minutes
    {
        use std::sync::Mutex;
        use std::time::Instant;
        static CONTACT_RATE: std::sync::LazyLock<Mutex<HashMap<String, (u32, Instant)>>> =
            std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
        let mut map = CONTACT_RATE.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let entry = map.entry(client_ip.0.clone()).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() >= 900 {
            *entry = (1, now);
        } else {
            entry.0 += 1;
            if entry.0 > 5 {
                let html = render::render_contact_page(
                    s,
                    &settings,
                    Some(("error", "Too many submissions. Please try again later.")),
                );
                return Some(RawHtml(html));
            }
        }
    }

    let data = form.into_inner();
    let name = data.get("name").map(|s| s.trim()).unwrap_or("");
    let email = data.get("email").map(|s| s.trim()).unwrap_or("");
    let message = data.get("message").map(|s| s.trim()).unwrap_or("");
    let honey = data.get("_honey").map(|s| s.trim()).unwrap_or("");

    // Honeypot check
    if !honey.is_empty() {
        let html = render::render_contact_page(
            s,
            &settings,
            Some(("success", "Message sent! Thank you.")),
        );
        return Some(RawHtml(html));
    }

    // Validation
    if name.is_empty() || email.is_empty() || message.is_empty() {
        let html = render::render_contact_page(
            s,
            &settings,
            Some(("error", "Please fill in all required fields.")),
        );
        return Some(RawHtml(html));
    }

    // Basic email validation
    if !email.contains('@') || !email.contains('.') {
        let html = render::render_contact_page(
            s,
            &settings,
            Some(("error", "Please enter a valid email address.")),
        );
        return Some(RawHtml(html));
    }

    // Send email to admin
    let admin_email = settings.get("admin_email").cloned().unwrap_or_default();
    if admin_email.is_empty() {
        let html = render::render_contact_page(
            s,
            &settings,
            Some((
                "error",
                "Contact form is not configured. Please try again later.",
            )),
        );
        return Some(RawHtml(html));
    }

    let site_name = settings
        .get("site_name")
        .cloned()
        .unwrap_or_else(|| "Velocty".to_string());
    let subject = format!("[{}] Contact from {}", site_name, name);
    let body = format!(
        "New contact form submission:\n\nName: {}\nEmail: {}\n\nMessage:\n{}",
        name, email, message
    );

    let from = crate::email::get_from_or_admin(&settings);
    match crate::email::send_via_provider(&settings, &from, &admin_email, &subject, &body) {
        Ok(()) => {
            log::info!("[contact] Form submitted by {} <{}>", name, email);
            let html = render::render_contact_page(
                s,
                &settings,
                Some(("success", "Message sent! Thank you for getting in touch.")),
            );
            Some(RawHtml(html))
        }
        Err(e) => {
            log::error!("[contact] Failed to send email: {}", e);
            let html = render::render_contact_page(
                s,
                &settings,
                Some(("error", "Failed to send message. Please try again later.")),
            );
            Some(RawHtml(html))
        }
    }
}

// ── Image proxy: /img/<token> ─────────────────────────
// Decodes the token, verifies HMAC, serves the file with caching headers.

#[get("/img/<token>")]
pub fn image_proxy_route(
    store: &State<Arc<dyn Store>>,
    token: &str,
) -> Result<FileResponse, Status> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    let secret = settings
        .get("image_proxy_secret")
        .cloned()
        .unwrap_or_default();
    if secret.is_empty() {
        return Err(Status::NotFound);
    }

    let old_secret = settings
        .get("image_proxy_secret_old")
        .cloned()
        .unwrap_or_default();
    let old_expires = settings
        .get("image_proxy_secret_old_expires")
        .cloned()
        .unwrap_or_default();
    let path = image_proxy::decode_token_with_fallback(&secret, &old_secret, &old_expires, token)
        .ok_or(Status::NotFound)?;

    serve_file_from_path(&path, "public, max-age=31536000, immutable")
}

/// Serve /uploads/ files only for authenticated admin users.
/// Public visitors get 404 — they should use /img/<token> proxy URLs instead.
#[get("/uploads/<path..>")]
pub fn serve_uploads(
    store: &State<Arc<dyn Store>>,
    cookies: &CookieJar<'_>,
    path: std::path::PathBuf,
) -> Result<FileResponse, Status> {
    let s: &dyn Store = &**store.inner();
    let session_id = cookies
        .get_private("velocty_session")
        .map(|c| c.value().to_string());
    let is_authenticated = session_id
        .as_deref()
        .and_then(|sid| auth::get_session_user(s, sid))
        .map(|u| u.is_active())
        .unwrap_or(false);

    if !is_authenticated {
        return Err(Status::NotFound);
    }

    let upload_path = format!("/uploads/{}", path.display());
    serve_file_from_path(&upload_path, "private, max-age=3600")
}

/// Shared helper: resolve an /uploads/... path to a file and serve it.
fn serve_file_from_path(path: &str, cache_control: &str) -> Result<FileResponse, Status> {
    if !path.starts_with("/uploads/") {
        return Err(Status::NotFound);
    }

    let fs_path = format!("website/site{}", path);
    let fs_path = Path::new(&fs_path);
    if !fs_path.exists() || !fs_path.is_file() {
        return Err(Status::NotFound);
    }

    // Prevent path traversal
    let canonical = fs_path.canonicalize().map_err(|_| Status::NotFound)?;
    let uploads_dir = Path::new("website/site/uploads")
        .canonicalize()
        .map_err(|_| Status::NotFound)?;
    if !canonical.starts_with(&uploads_dir) {
        return Err(Status::Forbidden);
    }

    let bytes = std::fs::read(&canonical).map_err(|_| Status::InternalServerError)?;
    let mime = image_proxy::mime_from_extension(canonical.to_str().unwrap_or(""));

    let csp = if mime == "image/svg+xml" {
        Some("default-src 'none'; style-src 'unsafe-inline'; img-src 'self'".to_string())
    } else {
        None
    };

    Ok(FileResponse {
        bytes,
        content_type: mime.to_string(),
        cache_control: cache_control.to_string(),
        content_security_policy: csp,
    })
}

// ── Search ────────────────────────────────────────────

#[get("/search?<q>")]
pub fn search_page(store: &State<Arc<dyn Store>>, q: Option<String>) -> RawHtml<String> {
    let s: &dyn Store = &**store.inner();
    let settings = s.setting_all();
    if settings.get("design_site_search").map(|v| v.as_str()) != Some("true") {
        return RawHtml(String::new());
    }
    let query = q.as_deref().unwrap_or("").trim().to_string();
    let results = if query.is_empty() {
        vec![]
    } else {
        s.search_query(&query, 50)
    };

    let nav_cats = s.category_list_nav_visible(Some("portfolio"));
    let nav_journal_cats = s.category_list_nav_visible(Some("post"));
    let context = json!({
        "settings": settings,
        "nav_categories": nav_cats,
        "nav_journal_categories": nav_journal_cats,
        "page_type": "search",
        "search_query": query,
        "search_results": results,
    });
    RawHtml(render::render_page(s, "search", &context))
}

pub fn root_routes() -> Vec<rocket::Route> {
    routes![
        search_page,
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
        contact_page,
        contact_submit,
        image_proxy_route,
        serve_uploads,
    ]
}

// ── Internal dispatch functions (called by catch-all) ────

fn do_blog_list(store: &dyn Store, page: Option<i64>) -> RawHtml<String> {
    let per_page = store.setting_get_i64("blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let posts = store.post_list(Some("published"), per_page, offset);
    let total = store.post_count(Some("published"));
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let settings = store.setting_all();

    // Inject author_name into each post from the first admin user
    let author_name = store
        .user_list_all()
        .into_iter()
        .find(|u| u.role == "admin")
        .map(|u| u.display_name)
        .unwrap_or_default();
    let posts_json: Vec<serde_json::Value> = posts
        .iter()
        .map(|p| {
            let mut pj = serde_json::to_value(p).unwrap_or_default();
            if let Some(obj) = pj.as_object_mut() {
                obj.insert("author_name".to_string(), json!(author_name.clone()));
                let cats = store.category_for_content(p.id, "post");
                obj.insert("categories".to_string(), json!(cats));
                let cc = store.comment_for_post(p.id, "post").len();
                obj.insert("comment_count".to_string(), json!(cc));
            }
            pj
        })
        .collect();

    let context = json!({
        "settings": settings,
        "nav_categories": nav_categories(store),
        "nav_journal_categories": nav_journal_categories(store),
        "posts": posts_json,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(store, Some("Blog"), None, &render::slug_url(&store.setting_get_or("blog_slug", "journal"), "")),
    });

    RawHtml(render::render_page(store, "blog_list", &context))
}

fn do_blog_single(store: &dyn Store, slug: &str) -> Option<RawHtml<String>> {
    let post = store.post_find_by_slug(slug)?;
    if post.status != "published" {
        return None;
    }

    let categories = store.category_for_content(post.id, "post");
    let tags = store.tag_for_content(post.id, "post");
    let settings = store.setting_all();
    let comments_enabled = settings.get("comments_on_blog").map(|v| v.as_str()) == Some("true");
    let comments = if comments_enabled {
        store.comment_for_post(post.id, "post")
    } else {
        vec![]
    };

    let prev_post = post
        .published_at
        .as_ref()
        .and_then(|pa| store.post_prev_published(pa));
    let next_post = post
        .published_at
        .as_ref()
        .and_then(|pa| store.post_next_published(pa));

    // Get author name from the first admin user
    let author_name = store
        .user_list_all()
        .into_iter()
        .find(|u| u.role == "admin")
        .map(|u| u.display_name)
        .unwrap_or_default();

    let mut post_json = serde_json::to_value(&post).unwrap_or_default();
    if let Some(obj) = post_json.as_object_mut() {
        obj.insert("author_name".to_string(), json!(author_name));
    }

    let mut context = json!({
        "settings": settings,
        "post": post_json,
        "categories": categories,
        "nav_categories": nav_categories(store),
        "nav_journal_categories": nav_journal_categories(store),
        "tags": tags,
        "comments": comments,
        "comments_enabled": comments_enabled,
        "page_type": "blog_single",
        "seo": seo::build_meta(
            store,
            post.meta_title.as_deref().or(Some(&post.title)),
            post.meta_description.as_deref(),
            &render::slug_url(&store.setting_get_or("blog_slug", "journal"), &post.slug),
        ),
    });

    if let Some(prev) = prev_post {
        context["prev_post"] = json!({"title": prev.title, "slug": prev.slug});
    }
    if let Some(next) = next_post {
        context["next_post"] = json!({"title": next.title, "slug": next.slug});
    }

    Some(RawHtml(render::render_page(store, "blog_single", &context)))
}

fn do_blog_by_category(
    store: &dyn Store,
    slug: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let category = store.category_find_by_slug(slug)?;
    let per_page = store.setting_get_i64("blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = store.setting_all();

    let posts = store.post_by_category(category.id, per_page, offset);
    let total = store.post_count_by_category(category.id);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let author_name = store
        .user_list_all()
        .into_iter()
        .find(|u| u.role == "admin")
        .map(|u| u.display_name)
        .unwrap_or_default();
    let posts_json: Vec<serde_json::Value> = posts
        .iter()
        .map(|p| {
            let mut pj = serde_json::to_value(p).unwrap_or_default();
            if let Some(obj) = pj.as_object_mut() {
                obj.insert("author_name".to_string(), json!(author_name.clone()));
                let cats = store.category_for_content(p.id, "post");
                obj.insert("categories".to_string(), json!(cats));
                let cc = store.comment_for_post(p.id, "post").len();
                obj.insert("comment_count".to_string(), json!(cc));
            }
            pj
        })
        .collect();

    let context = json!({
        "settings": settings,
        "nav_categories": nav_categories(store),
        "nav_journal_categories": nav_journal_categories(store),
        "posts": posts_json,
        "active_category": category,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(store, Some(&category.name), None, &render::slug_url(&store.setting_get_or("blog_slug", "journal"), &format!("category/{}", slug))),
    });

    Some(RawHtml(render::render_page(store, "blog_list", &context)))
}

fn do_blog_by_tag(store: &dyn Store, slug: &str, page: Option<i64>) -> Option<RawHtml<String>> {
    let tag = store.tag_find_by_slug(slug)?;
    let per_page = store.setting_get_i64("blog_posts_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let settings = store.setting_all();

    let posts = store.post_by_tag(tag.id, per_page, offset);
    let total = store.post_count_by_tag(tag.id);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let author_name = store
        .user_list_all()
        .into_iter()
        .find(|u| u.role == "admin")
        .map(|u| u.display_name)
        .unwrap_or_default();
    let posts_json: Vec<serde_json::Value> = posts
        .iter()
        .map(|p| {
            let mut pj = serde_json::to_value(p).unwrap_or_default();
            if let Some(obj) = pj.as_object_mut() {
                obj.insert("author_name".to_string(), json!(author_name.clone()));
                let cats = store.category_for_content(p.id, "post");
                obj.insert("categories".to_string(), json!(cats));
                let cc = store.comment_for_post(p.id, "post").len();
                obj.insert("comment_count".to_string(), json!(cc));
            }
            pj
        })
        .collect();

    let context = json!({
        "settings": settings,
        "nav_categories": nav_categories(store),
        "nav_journal_categories": nav_journal_categories(store),
        "posts": posts_json,
        "active_tag": tag,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "blog_list",
        "seo": seo::build_meta(store, Some(&tag.name), None, &render::slug_url(&store.setting_get_or("blog_slug", "journal"), &format!("tag/{}", slug))),
    });

    Some(RawHtml(render::render_page(store, "blog_list", &context)))
}

fn do_portfolio_grid(store: &dyn Store, page: Option<i64>) -> RawHtml<String> {
    let per_page = store.setting_get_i64("portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let items = store.portfolio_list(Some("published"), per_page, offset);
    let total = store.portfolio_count(Some("published"));
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let categories = store.category_list_nav_visible(Some("portfolio"));
    let settings = store.setting_all();

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = store.tag_for_content(item.id, "portfolio");
            let cats = store.category_for_content(item.id, "portfolio");
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
        "nav_journal_categories": nav_journal_categories(store),
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(store, Some("Portfolio"), None, &render::slug_url(&store.setting_get_or("portfolio_slug", "portfolio"), "")),
    });

    RawHtml(render::render_page(store, "portfolio_grid", &context))
}

fn do_portfolio_single(store: &dyn Store, slug: &str) -> Option<RawHtml<String>> {
    let item = store.portfolio_find_by_slug(slug)?;
    if item.status != "published" {
        return None;
    }

    let categories = store.category_for_content(item.id, "portfolio");
    let tags = store.tag_for_content(item.id, "portfolio");
    let settings = store.setting_all();
    let comments_enabled =
        settings.get("comments_on_portfolio").map(|v| v.as_str()) == Some("true");
    let comments = if comments_enabled {
        store.comment_for_post(item.id, "portfolio")
    } else {
        vec![]
    };

    let any_commerce = [
        "commerce_paypal_enabled",
        "commerce_stripe_enabled",
        "commerce_payoneer_enabled",
        "commerce_2checkout_enabled",
        "commerce_square_enabled",
        "commerce_razorpay_enabled",
        "commerce_mollie_enabled",
    ]
    .iter()
    .any(|k| settings.get(*k).map(|v| v.as_str()) == Some("true"));

    let context = json!({
        "settings": settings,
        "item": item,
        "categories": categories,
        "nav_categories": nav_categories(store),
        "nav_journal_categories": nav_journal_categories(store),
        "tags": tags,
        "comments": comments,
        "comments_enabled": comments_enabled,
        "page_type": "portfolio_single",
        "commerce_enabled": any_commerce && item.sell_enabled && item.price.unwrap_or(0.0) > 0.0,
        "seo": seo::build_meta(
            store,
            item.meta_title.as_deref().or(Some(&item.title)),
            item.meta_description.as_deref(),
            &render::slug_url(&store.setting_get_or("portfolio_slug", "portfolio"), &item.slug),
        ),
    });

    Some(RawHtml(render::render_page(
        store,
        "portfolio_single",
        &context,
    )))
}

fn do_portfolio_by_category(
    store: &dyn Store,
    slug: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let category = store.category_find_by_slug(slug)?;
    let per_page = store.setting_get_i64("portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let items = store.portfolio_by_category(slug, per_page, offset);
    let categories = store.category_list_nav_visible(Some("portfolio"));
    let settings = store.setting_all();

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = store.tag_for_content(item.id, "portfolio");
            let cats = store.category_for_content(item.id, "portfolio");
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
        "nav_journal_categories": nav_journal_categories(store),
        "active_category": category,
        "current_page": current_page,
        "total_pages": ((store.portfolio_count(Some("published")) as f64 / per_page as f64).ceil() as i64),
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(store, Some(&category.name), None, &render::slug_url(&store.setting_get_or("portfolio_slug", "portfolio"), &format!("category/{}", slug))),
    });

    Some(RawHtml(render::render_page(
        store,
        "portfolio_grid",
        &context,
    )))
}

fn do_portfolio_by_tag(
    store: &dyn Store,
    slug: &str,
    page: Option<i64>,
) -> Option<RawHtml<String>> {
    let tag = store.tag_find_by_slug(slug)?;
    let per_page = store.setting_get_i64("portfolio_items_per_page").max(1);
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;
    let categories = store.category_list_nav_visible(Some("portfolio"));
    let settings = store.setting_all();

    let items = store.portfolio_by_tag(tag.id, per_page, offset);

    let items_with_meta: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let tags = store.tag_for_content(item.id, "portfolio");
            let cats = store.category_for_content(item.id, "portfolio");
            json!({
                "item": item,
                "tags": tags,
                "categories": cats,
            })
        })
        .collect();

    let total = store.portfolio_count_by_tag(tag.id);
    let total_pages = (total as f64 / per_page as f64).ceil() as i64;

    let context = json!({
        "settings": settings,
        "items": items_with_meta,
        "categories": categories,
        "nav_journal_categories": nav_journal_categories(store),
        "active_tag": tag,
        "current_page": current_page,
        "total_pages": total_pages,
        "page_type": "portfolio_grid",
        "seo": seo::build_meta(store, Some(&tag.name), None, &render::slug_url(&store.setting_get_or("portfolio_slug", "portfolio"), &format!("tag/{}", slug))),
    });

    Some(RawHtml(render::render_page(
        store,
        "portfolio_grid",
        &context,
    )))
}
