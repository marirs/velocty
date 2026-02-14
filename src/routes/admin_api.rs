use rocket::serde::json::Json;
use rocket::State;
use serde_json::Value;

use crate::auth::AdminUser;
use crate::db::DbPool;
use crate::models::analytics::PageView;
use crate::models::settings::Setting;

#[get("/stats/overview?<from>&<to>")]
pub fn stats_overview(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let stats = PageView::overview(pool, &from, &to);
    Json(serde_json::to_value(stats).unwrap_or_default())
}

#[get("/stats/flow?<from>&<to>")]
pub fn stats_flow(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = PageView::flow_data(pool, &from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/geo?<from>&<to>")]
pub fn stats_geo(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = PageView::geo_data(pool, &from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/stream?<from>&<to>")]
pub fn stats_stream(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = PageView::stream_data(pool, &from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/calendar?<from>&<to>")]
pub fn stats_calendar(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let data = PageView::calendar_data(pool, &from, &to);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/top-portfolio?<from>&<to>&<limit>")]
pub fn stats_top_portfolio(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let limit = limit.unwrap_or(10);
    let data = PageView::top_portfolio(pool, &from, &to, limit);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/top-referrers?<from>&<to>&<limit>")]
pub fn stats_top_referrers(
    _admin: AdminUser,
    pool: &State<DbPool>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
) -> Json<Value> {
    let from = from.unwrap_or_else(|| "2000-01-01".to_string());
    let to = to.unwrap_or_else(|| "2099-12-31".to_string());
    let limit = limit.unwrap_or(10);
    let data = PageView::top_referrers(pool, &from, &to, limit);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[get("/stats/tags")]
pub fn stats_tags(_admin: AdminUser, pool: &State<DbPool>) -> Json<Value> {
    let data = PageView::tag_relations(pool);
    Json(serde_json::to_value(data).unwrap_or_default())
}

#[post("/theme", data = "<body>")]
pub fn set_theme(
    _admin: AdminUser,
    pool: &State<DbPool>,
    body: Json<Value>,
) -> Json<Value> {
    let theme = body.get("theme").and_then(|v| v.as_str()).unwrap_or("dark");
    let theme = if theme == "light" { "light" } else { "dark" };
    let _ = Setting::set(pool, "admin_theme", theme);
    Json(serde_json::json!({"ok": true, "theme": theme}))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        stats_overview,
        stats_flow,
        stats_geo,
        stats_stream,
        stats_calendar,
        stats_top_portfolio,
        stats_top_referrers,
        stats_tags,
        set_theme,
    ]
}
