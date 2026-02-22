use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::State;
use serde_json::Value;

use crate::security::auth::{AdminUser, EditorUser};
use crate::store::Store;

#[get("/stats/overview?<from>&<to>")]
pub fn stats_overview(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let stats = store.analytics_overview(&from, &to);
    Json(serde_json::to_value(stats).unwrap_or_default())
}

#[get("/stats/flow?<from>&<to>")]
pub fn stats_flow(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = store.analytics_flow_data(&from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/geo?<from>&<to>")]
pub fn stats_geo(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = store.analytics_geo_data(&from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/stream?<from>&<to>")]
pub fn stats_stream(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = store.analytics_stream_data(&from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/calendar?<from>&<to>")]
pub fn stats_calendar(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = store.analytics_calendar_data(&from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/top-portfolio?<from>&<to>&<limit>")]
pub fn stats_top_portfolio(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let limit = limit.unwrap_or(10);
    let data = store.analytics_top_portfolio(&from, &to, limit);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/top-referrers?<from>&<to>&<limit>")]
pub fn stats_top_referrers(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let limit = limit.unwrap_or(10);
    let data = store.analytics_top_referrers(&from, &to, limit);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/tags")]
pub fn stats_tags(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let data = store.analytics_tag_relations();
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[post("/theme", data = "<body>")]
pub fn set_theme(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    body: Json<Value>,
) -> Json<Value> {
    let theme = body.get("theme").and_then(|v| v.as_str()).unwrap_or("dark");
    let theme = if theme == "light" { "light" } else { "dark" };
    let _ = store.setting_set("admin_theme", theme);
    Json(serde_json::json!({"ok": true, "theme": theme}))
}

// ── SEO Check ─────────────────────────────────────────

fn seo_check_item(
    store: &dyn Store,
    content_type: &str,
    title: &str,
    slug: &str,
    meta_title: Option<&str>,
    meta_description: Option<&str>,
    excerpt: Option<&str>,
    content_html: Option<&str>,
    featured_image: Option<&str>,
    content_id: i64,
) -> Value {
    let mut checks: Vec<Value> = Vec::new();
    let mut score: u32 = 0;
    let total: u32 = 10;

    // 1. Meta title
    let mt = meta_title.unwrap_or("");
    if mt.is_empty() {
        checks.push(serde_json::json!({"check": "Meta Title", "status": "fail", "message": "Missing. Search engines will use the post title instead."}));
    } else if mt.len() > 60 {
        checks.push(serde_json::json!({"check": "Meta Title", "status": "warn", "message": format!("Too long ({} chars). Recommended: ≤60 characters.", mt.len())}));
    } else {
        checks.push(serde_json::json!({"check": "Meta Title", "status": "pass", "message": format!("{} chars — good length.", mt.len())}));
        score += 1;
    }

    // 2. Meta description
    let md = meta_description.unwrap_or("");
    if md.is_empty() {
        checks.push(serde_json::json!({"check": "Meta Description", "status": "fail", "message": "Missing. Add a description for search engine snippets."}));
    } else if md.len() > 160 {
        checks.push(serde_json::json!({"check": "Meta Description", "status": "warn", "message": format!("Too long ({} chars). Recommended: ≤160 characters.", md.len())}));
    } else if md.len() < 50 {
        checks.push(serde_json::json!({"check": "Meta Description", "status": "warn", "message": format!("Too short ({} chars). Recommended: 50–160 characters.", md.len())}));
    } else {
        checks.push(serde_json::json!({"check": "Meta Description", "status": "pass", "message": format!("{} chars — good length.", md.len())}));
        score += 1;
    }

    // 3. Slug quality
    if slug.contains("--") || slug.contains('_') {
        checks.push(serde_json::json!({"check": "URL Slug", "status": "warn", "message": "Contains double hyphens or underscores. Use clean single hyphens."}));
    } else if slug.len() > 75 {
        checks.push(serde_json::json!({"check": "URL Slug", "status": "warn", "message": format!("Long slug ({} chars). Shorter URLs rank better.", slug.len())}));
    } else {
        checks.push(serde_json::json!({"check": "URL Slug", "status": "pass", "message": "Clean and readable."}));
        score += 1;
    }

    // 4. Title length
    if title.len() > 70 {
        checks.push(serde_json::json!({"check": "Title Length", "status": "warn", "message": format!("{} chars. May be truncated in search results (≤70 recommended).", title.len())}));
    } else if title.is_empty() {
        checks.push(serde_json::json!({"check": "Title Length", "status": "fail", "message": "Title is empty."}));
    } else {
        checks.push(serde_json::json!({"check": "Title Length", "status": "pass", "message": format!("{} chars — good.", title.len())}));
        score += 1;
    }

    // 5. Excerpt / description
    let exc = excerpt.unwrap_or("");
    if exc.is_empty() {
        checks.push(serde_json::json!({"check": "Excerpt", "status": "warn", "message": "No excerpt set. Auto-generated excerpts may not be ideal."}));
    } else {
        checks.push(
            serde_json::json!({"check": "Excerpt", "status": "pass", "message": "Excerpt is set."}),
        );
        score += 1;
    }

    // 6. Featured image
    let fi = featured_image.unwrap_or("");
    if fi.is_empty() && content_type == "post" {
        checks.push(serde_json::json!({"check": "Featured Image", "status": "warn", "message": "No featured image. Posts with images get more engagement."}));
    } else {
        checks.push(serde_json::json!({"check": "Featured Image", "status": "pass", "message": "Featured image is set."}));
        score += 1;
    }

    // 7. Content length
    let html = content_html.unwrap_or("");
    let text_len = html.len();
    if content_type == "post" {
        if text_len < 300 {
            checks.push(serde_json::json!({"check": "Content Length", "status": "warn", "message": "Very short content. Longer posts tend to rank better (300+ words recommended)."}));
        } else {
            checks.push(serde_json::json!({"check": "Content Length", "status": "pass", "message": "Content has good length."}));
            score += 1;
        }
    } else {
        // Portfolio — description is optional but helpful
        if text_len > 0 {
            checks.push(serde_json::json!({"check": "Description", "status": "pass", "message": "Description is set."}));
            score += 1;
        } else {
            checks.push(serde_json::json!({"check": "Description", "status": "warn", "message": "No description. Adding one helps SEO."}));
        }
    }

    // 8. Image alt text in content (check for <img without alt)
    if html.contains("<img") {
        let missing_alt =
            html.contains("alt=\"\"") || (html.contains("<img") && !html.contains("alt="));
        if missing_alt {
            checks.push(serde_json::json!({"check": "Image Alt Text", "status": "warn", "message": "Some images may be missing alt text. Alt text improves accessibility and SEO."}));
        } else {
            checks.push(serde_json::json!({"check": "Image Alt Text", "status": "pass", "message": "Images have alt text."}));
            score += 1;
        }
    } else {
        checks.push(serde_json::json!({"check": "Image Alt Text", "status": "pass", "message": "No inline images to check."}));
        score += 1;
    }

    // 9. Tags
    let tags = store.tag_for_content(content_id, content_type);
    if tags.is_empty() {
        checks.push(serde_json::json!({"check": "Tags", "status": "warn", "message": "No tags assigned. Tags help with internal linking and discovery."}));
    } else {
        checks.push(serde_json::json!({"check": "Tags", "status": "pass", "message": format!("{} tag(s) assigned.", tags.len())}));
        score += 1;
    }

    // 10. Heading structure (H1 in content — should not have H1 since title is H1)
    if html.contains("<h1") {
        checks.push(serde_json::json!({"check": "Heading Structure", "status": "warn", "message": "Content contains an H1 tag. The page title is already H1 — use H2+ in content."}));
    } else {
        checks.push(serde_json::json!({"check": "Heading Structure", "status": "pass", "message": "No duplicate H1 in content."}));
        score += 1;
    }

    let grade = match score * 100 / total {
        90..=100 => "A",
        70..=89 => "B",
        50..=69 => "C",
        30..=49 => "D",
        _ => "F",
    };

    serde_json::json!({
        "score": score,
        "total": total,
        "grade": grade,
        "checks": checks,
    })
}

#[get("/seo-check/post/<id>")]
pub fn seo_check_post(_admin: EditorUser, store: &State<Arc<dyn Store>>, id: i64) -> Json<Value> {
    let post = match store.post_find_by_id(id) {
        Some(p) => p,
        None => return Json(serde_json::json!({"error": "Post not found"})),
    };

    Json(seo_check_item(
        &**store.inner(),
        "post",
        &post.title,
        &post.slug,
        post.meta_title.as_deref(),
        post.meta_description.as_deref(),
        post.excerpt.as_deref(),
        Some(&post.content_html),
        post.featured_image.as_deref(),
        post.id,
    ))
}

#[get("/seo-check/portfolio/<id>")]
pub fn seo_check_portfolio(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    id: i64,
) -> Json<Value> {
    let item = match store.portfolio_find_by_id(id) {
        Some(i) => i,
        None => return Json(serde_json::json!({"error": "Portfolio item not found"})),
    };

    Json(seo_check_item(
        &**store.inner(),
        "portfolio",
        &item.title,
        &item.slug,
        item.meta_title.as_deref(),
        item.meta_description.as_deref(),
        None,
        item.description_html.as_deref(),
        Some(&item.image_path),
        item.id,
    ))
}

/// Rotate the image proxy HMAC secret key.
/// Copies current → old (with expiry), generates a new current key.
#[post("/rotate-image-proxy-key")]
pub fn rotate_image_proxy_key(admin: AdminUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    use rand::Rng;

    let current_secret = store.setting_get_or("image_proxy_secret", "");
    if current_secret.is_empty() {
        return Json(
            serde_json::json!({ "ok": false, "error": "No image proxy secret configured" }),
        );
    }

    let rotation_days: i64 = store
        .setting_get_or("image_proxy_rotation_days", "7")
        .parse()
        .unwrap_or(7)
        .clamp(1, 30);

    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::days(rotation_days);
    let expires_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

    // Copy current key → old key with expiry
    let _ = store.setting_set("image_proxy_secret_old", &current_secret);
    let _ = store.setting_set("image_proxy_secret_old_expires", &expires_str);

    // Generate new key
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    let new_secret = hex::encode(bytes);
    let _ = store.setting_set("image_proxy_secret", &new_secret);

    store.audit_log(
        Some(admin.user.id),
        Some(&admin.user.display_name),
        "rotate_image_proxy_key",
        None,
        None,
        None,
        Some(&format!(
            "Image proxy key rotated. Old key expires: {}",
            expires_str
        )),
        None,
    );

    Json(serde_json::json!({
        "ok": true,
        "expires": expires_str,
        "rotation_days": rotation_days
    }))
}

/// Lightweight endpoint for sidebar widget — returns average SEO score
#[get("/seo-score")]
pub fn seo_score_summary(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let posts = store.post_list(None, 10000, 0);
    let items = store.portfolio_list(None, 10000, 0);
    let mut sum: i64 = 0;
    let mut count: i64 = 0;
    for p in &posts {
        if p.seo_score >= 0 {
            sum += p.seo_score as i64;
            count += 1;
        }
    }
    for i in &items {
        if i.seo_score >= 0 {
            sum += i.seo_score as i64;
            count += 1;
        }
    }
    if count > 0 {
        Json(serde_json::json!({"score": sum / count, "count": count}))
    } else {
        Json(serde_json::json!({"score": -1, "count": 0}))
    }
}

/// Rescan SEO scores for all posts and portfolio items
#[post("/seo-rescan")]
pub fn seo_rescan_all(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let mut scanned = 0i32;

    // Score all posts
    let posts = store.post_list(None, 10000, 0);
    for post in &posts {
        let input = crate::seo::audit::SeoInput {
            title: &post.title,
            slug: &post.slug,
            meta_title: post.meta_title.as_deref().unwrap_or(""),
            meta_description: post.meta_description.as_deref().unwrap_or(""),
            body_html: &post.content_html,
            featured_image: post.featured_image.as_deref().unwrap_or(""),
            content_type: "post",
        };
        let audit = crate::seo::audit::compute_score(&input);
        let _ = store.post_update_seo_score(
            post.id,
            audit.score,
            &crate::seo::audit::issues_to_json(&audit.issues),
        );
        scanned += 1;
    }

    // Score all portfolio items
    let items = store.portfolio_list(None, 10000, 0);
    for item in &items {
        let input = crate::seo::audit::SeoInput {
            title: &item.title,
            slug: &item.slug,
            meta_title: item.meta_title.as_deref().unwrap_or(""),
            meta_description: item.meta_description.as_deref().unwrap_or(""),
            body_html: item.description_html.as_deref().unwrap_or(""),
            featured_image: &item.image_path,
            content_type: "portfolio",
        };
        let audit = crate::seo::audit::compute_score(&input);
        let _ = store.portfolio_update_seo_score(
            item.id,
            audit.score,
            &crate::seo::audit::issues_to_json(&audit.issues),
        );
        scanned += 1;
    }

    store.audit_log(
        Some(_admin.user.id),
        Some(&_admin.user.display_name),
        "seo_rescan",
        None,
        None,
        Some(&format!("Rescanned {} items", scanned)),
        None,
        None,
    );

    Json(serde_json::json!({ "ok": true, "scanned": scanned }))
}

/// Fetch PageSpeed Insights for a URL (proxied to avoid CORS)
#[get("/pagespeed?<url>")]
pub fn pagespeed_fetch(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    url: &str,
) -> Json<Value> {
    let api_key = store.setting_get_or("seo_pagespeed_api_key", "");

    // URL must be publicly accessible — localhost won't work
    if url.contains("localhost") || url.contains("127.0.0.1") {
        return Json(
            serde_json::json!({"error": "PageSpeed Insights requires a publicly accessible URL. localhost cannot be tested."}),
        );
    }

    let encoded_url: String = url
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b'/' => {
                format!("{}", b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect();
    let mut endpoint = format!(
        "https://www.googleapis.com/pagespeedonline/v5/runPagespeed?url={}&strategy=mobile&category=performance&category=accessibility&category=seo",
        encoded_url
    );
    if !api_key.is_empty() {
        endpoint.push_str(&format!("&key={}", api_key));
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": format!("HTTP client error: {}", e)})),
    };

    match client.get(&endpoint).send() {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<Value>() {
                Ok(data) => {
                    if !status.is_success() {
                        let msg = data
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown API error");
                        Json(
                            serde_json::json!({"error": format!("PageSpeed API error ({}): {}", status.as_u16(), msg)}),
                        )
                    } else {
                        Json(data)
                    }
                }
                Err(_) => Json(
                    serde_json::json!({"error": format!("Failed to parse PageSpeed response (HTTP {})", status.as_u16())}),
                ),
            }
        }
        Err(e) => Json(serde_json::json!({"error": format!("Request failed: {}", e)})),
    }
}

/// Fetch Moz domain metrics (DA, PA, backlinks, spam score) — cached in settings
#[get("/moz-domain")]
pub fn moz_domain_fetch(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let access_id = store.setting_get_or("seo_moz_access_id", "");
    let secret_key = store.setting_get_or("seo_moz_secret_key", "");

    if access_id.is_empty() || secret_key.is_empty() {
        return Json(
            serde_json::json!({"error": "Moz API credentials not configured. Go to Settings > SEO > Ranking."}),
        );
    }

    let site_url = store.setting_get_or("seo_canonical_base", "");
    let site_url = if site_url.is_empty() {
        store.setting_get_or("site_url", "")
    } else {
        site_url
    };
    if site_url.is_empty() {
        return Json(
            serde_json::json!({"error": "No site URL configured. Set a canonical base URL in Settings > SEO > General."}),
        );
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": format!("HTTP client error: {}", e)})),
    };

    let body = serde_json::json!({
        "targets": [site_url],
        "columns": ["domain_authority", "page_authority", "spam_score", "external_links_to_root_domain", "linking_root_domains_to_root_domain"]
    });

    match client
        .post("https://lsapi.seomoz.com/v2/url_metrics")
        .basic_auth(&access_id, Some(&secret_key))
        .json(&body)
        .send()
    {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<Value>() {
                Ok(data) => {
                    if !status.is_success() {
                        let msg = data
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown Moz API error");
                        Json(
                            serde_json::json!({"error": format!("Moz API error ({}): {}", status.as_u16(), msg)}),
                        )
                    } else {
                        // Cache the result
                        let cache = serde_json::json!({
                            "data": data,
                            "fetched_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
                        });
                        let _ = store.setting_set("seo_moz_cache", &cache.to_string());
                        Json(cache)
                    }
                }
                Err(_) => Json(
                    serde_json::json!({"error": format!("Failed to parse Moz response (HTTP {})", status.as_u16())}),
                ),
            }
        }
        Err(e) => Json(serde_json::json!({"error": format!("Request failed: {}", e)})),
    }
}

/// Fetch cached Moz data without hitting the API
#[get("/moz-domain/cached")]
pub fn moz_domain_cached(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let cached = store.setting_get_or("seo_moz_cache", "");
    if cached.is_empty() {
        return Json(serde_json::json!({"cached": false}));
    }
    match serde_json::from_str::<Value>(&cached) {
        Ok(v) => Json(v),
        Err(_) => Json(serde_json::json!({"cached": false})),
    }
}

/// Fetch Open PageRank score — cached in settings
#[get("/pagerank")]
pub fn pagerank_fetch(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let api_key = store.setting_get_or("seo_openpagerank_api_key", "");

    if api_key.is_empty() {
        return Json(
            serde_json::json!({"error": "Open PageRank API key not configured. Go to Settings > SEO > Ranking."}),
        );
    }

    let site_url = store.setting_get_or("seo_canonical_base", "");
    let site_url = if site_url.is_empty() {
        store.setting_get_or("site_url", "")
    } else {
        site_url
    };
    if site_url.is_empty() {
        return Json(
            serde_json::json!({"error": "No site URL configured. Set a canonical base URL in Settings > SEO > General."}),
        );
    }

    // Extract domain from URL
    let domain = site_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": format!("HTTP client error: {}", e)})),
    };

    let endpoint = format!(
        "https://openpagerank.com/api/v1.0/getPageRank?domains%5B0%5D={}",
        domain
    );

    match client.get(&endpoint).header("API-OPR", &api_key).send() {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<Value>() {
                Ok(data) => {
                    if !status.is_success() {
                        let msg = data
                            .get("error")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown PageRank API error");
                        Json(
                            serde_json::json!({"error": format!("PageRank API error ({}): {}", status.as_u16(), msg)}),
                        )
                    } else {
                        let cache = serde_json::json!({
                            "data": data,
                            "fetched_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
                        });
                        let _ = store.setting_set("seo_pagerank_cache", &cache.to_string());
                        Json(cache)
                    }
                }
                Err(_) => Json(
                    serde_json::json!({"error": format!("Failed to parse PageRank response (HTTP {})", status.as_u16())}),
                ),
            }
        }
        Err(e) => Json(serde_json::json!({"error": format!("Request failed: {}", e)})),
    }
}

/// Fetch cached PageRank data without hitting the API
#[get("/pagerank/cached")]
pub fn pagerank_cached(_admin: EditorUser, store: &State<Arc<dyn Store>>) -> Json<Value> {
    let cached = store.setting_get_or("seo_pagerank_cache", "");
    if cached.is_empty() {
        return Json(serde_json::json!({"cached": false}));
    }
    match serde_json::from_str::<Value>(&cached) {
        Ok(v) => Json(v),
        Err(_) => Json(serde_json::json!({"cached": false})),
    }
}
