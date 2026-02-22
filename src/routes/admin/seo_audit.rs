use std::sync::Arc;

use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::EditorUser;
use crate::store::Store;

#[get("/seo-audit")]
pub fn seo_audit_dashboard(
    _admin: EditorUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<super::AdminSlug>,
) -> Template {
    let settings = store.setting_all();
    let journal_enabled = settings.get("journal_enabled").map(|v| v.as_str()) == Some("true");
    let portfolio_enabled = settings.get("portfolio_enabled").map(|v| v.as_str()) == Some("true");

    // Gather all posts and portfolio items
    let posts = if journal_enabled {
        store.post_list(None, 10000, 0)
    } else {
        vec![]
    };
    let items = if portfolio_enabled {
        store.portfolio_list(None, 10000, 0)
    } else {
        vec![]
    };

    // Aggregate scores
    let mut good_posts = 0i32;
    let mut warn_posts = 0i32;
    let mut poor_posts = 0i32;
    let mut unscored_posts = 0i32;
    let mut post_total_score = 0i64;
    let mut post_scored_count = 0i32;

    let post_rows: Vec<serde_json::Value> = posts
        .iter()
        .map(|p| {
            let score = p.seo_score;
            if score < 0 {
                unscored_posts += 1;
            } else {
                post_scored_count += 1;
                post_total_score += score as i64;
                if score >= 80 {
                    good_posts += 1;
                } else if score >= 50 {
                    warn_posts += 1;
                } else {
                    poor_posts += 1;
                }
            }
            json!({
                "id": p.id,
                "title": p.title,
                "slug": p.slug,
                "status": p.status,
                "seo_score": score,
                "seo_issues": p.seo_issues,
                "type": "post",
            })
        })
        .collect();

    let mut good_items = 0i32;
    let mut warn_items = 0i32;
    let mut poor_items = 0i32;
    let mut unscored_items = 0i32;
    let mut item_total_score = 0i64;
    let mut item_scored_count = 0i32;

    let portfolio_rows: Vec<serde_json::Value> = items
        .iter()
        .map(|p| {
            let score = p.seo_score;
            if score < 0 {
                unscored_items += 1;
            } else {
                item_scored_count += 1;
                item_total_score += score as i64;
                if score >= 80 {
                    good_items += 1;
                } else if score >= 50 {
                    warn_items += 1;
                } else {
                    poor_items += 1;
                }
            }
            json!({
                "id": p.id,
                "title": p.title,
                "slug": p.slug,
                "status": p.status,
                "seo_score": score,
                "seo_issues": p.seo_issues,
                "type": "portfolio",
            })
        })
        .collect();

    let total_scored = post_scored_count + item_scored_count;
    let total_score_sum = post_total_score + item_total_score;
    let overall_score = if total_scored > 0 {
        (total_score_sum / total_scored as i64) as i32
    } else {
        -1
    };

    let good_total = good_posts + good_items;
    let warn_total = warn_posts + warn_items;
    let poor_total = poor_posts + poor_items;
    let unscored_total = unscored_posts + unscored_items;

    // Aggregate top issues across all items
    let mut issue_counts: std::collections::HashMap<String, (i32, String)> =
        std::collections::HashMap::new();
    for row in post_rows.iter().chain(portfolio_rows.iter()) {
        if let Some(issues_str) = row.get("seo_issues").and_then(|v| v.as_str()) {
            let issues: Vec<crate::seo::audit::SeoIssue> =
                crate::seo::audit::issues_from_json(issues_str);
            for issue in &issues {
                let entry = issue_counts
                    .entry(issue.code.clone())
                    .or_insert((0, issue.message.clone()));
                entry.0 += 1;
            }
        }
    }
    let mut top_issues: Vec<serde_json::Value> = issue_counts
        .into_iter()
        .map(|(code, (count, message))| json!({"code": code, "count": count, "message": message}))
        .collect();
    top_issues.sort_by(|a, b| {
        b.get("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .cmp(&a.get("count").and_then(|v| v.as_i64()).unwrap_or(0))
    });

    // Settings checklist
    let checklist = json!([
        {"label": "Sitemap enabled", "ok": settings.get("seo_sitemap_enabled").map(|v| v.as_str()) == Some("true")},
        {"label": "Structured data (JSON-LD)", "ok": settings.get("seo_structured_data").map(|v| v.as_str()) == Some("true")},
        {"label": "Open Graph tags", "ok": settings.get("seo_open_graph").map(|v| v.as_str()) == Some("true")},
        {"label": "Twitter Card tags", "ok": settings.get("seo_twitter_cards").map(|v| v.as_str()) == Some("true")},
        {"label": "Canonical base URL set", "ok": !settings.get("seo_canonical_base").map(|v| v.as_str()).unwrap_or("").is_empty()},
        {"label": "Default meta description set", "ok": !settings.get("seo_default_description").map(|v| v.as_str()).unwrap_or("").is_empty()},
        {"label": "Google Search Console verified", "ok": !settings.get("seo_google_verification").map(|v| v.as_str()).unwrap_or("").is_empty()},
        {"label": "Bing Webmaster verified", "ok": !settings.get("seo_bing_verification").map(|v| v.as_str()).unwrap_or("").is_empty()},
        {"label": "Analytics configured", "ok":
            settings.get("seo_ga_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_plausible_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_fathom_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_matomo_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_cloudflare_analytics_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_clicky_enabled").map(|v| v.as_str()) == Some("true") ||
            settings.get("seo_umami_enabled").map(|v| v.as_str()) == Some("true")
        },
    ]);

    let site_url = settings
        .get("seo_canonical_base")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| settings.get("site_url").cloned())
        .unwrap_or_default();

    let context = json!({
        "page_title": "SEO Audit",
        "admin_slug": slug.0,
        "user": _admin.user.safe_json(),
        "settings": settings,
        "journal_enabled": journal_enabled,
        "portfolio_enabled": portfolio_enabled,
        "overall_score": overall_score,
        "good_total": good_total,
        "warn_total": warn_total,
        "poor_total": poor_total,
        "unscored_total": unscored_total,
        "total_scored": total_scored,
        "good_posts": good_posts,
        "warn_posts": warn_posts,
        "poor_posts": poor_posts,
        "good_items": good_items,
        "warn_items": warn_items,
        "poor_items": poor_items,
        "post_rows": post_rows,
        "portfolio_rows": portfolio_rows,
        "top_issues": top_issues,
        "checklist": checklist,
        "site_url": site_url,
    });

    Template::render("admin/seo_audit", context)
}
