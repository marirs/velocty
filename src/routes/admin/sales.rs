use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AdminUser;
use crate::db::DbPool;
use crate::models::settings::Setting;
use crate::AdminSlug;

// ── Sales ──────────────────────────────────────────────

#[get("/sales")]
pub fn sales_dashboard(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
) -> Template {
    use crate::models::order::Order;
    let settings = Setting::all(pool);
    let total_revenue = Order::total_revenue(pool);
    let revenue_30d = Order::revenue_by_period(pool, 30);
    let revenue_7d = Order::revenue_by_period(pool, 7);
    let total_orders = Order::count(pool);
    let completed_orders = Order::count_by_status(pool, "completed");
    let pending_orders = Order::count_by_status(pool, "pending");
    let recent_orders = Order::list(pool, 10, 0);
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());

    let context = json!({
        "page_title": "Sales Dashboard",
        "admin_slug": &slug.0,
        "settings": &settings,
        "total_revenue": total_revenue,
        "revenue_30d": revenue_30d,
        "revenue_7d": revenue_7d,
        "total_orders": total_orders,
        "completed_orders": completed_orders,
        "pending_orders": pending_orders,
        "recent_orders": recent_orders,
        "currency": currency,
    });
    Template::render("admin/sales/dashboard", &context)
}

#[get("/sales/orders?<page>&<status>")]
pub fn sales_orders(
    _admin: AdminUser,
    pool: &State<DbPool>,
    slug: &State<AdminSlug>,
    page: Option<i64>,
    status: Option<String>,
) -> Template {
    use crate::models::order::Order;
    let settings = Setting::all(pool);
    let per_page: i64 = 25;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let (orders, total) = match status.as_deref() {
        Some(s) if !s.is_empty() => (
            Order::list_by_status(pool, s, per_page, offset),
            Order::count_by_status(pool, s),
        ),
        _ => (Order::list(pool, per_page, offset), Order::count(pool)),
    };

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let currency = settings.get("commerce_currency").cloned().unwrap_or_else(|| "USD".to_string());

    let context = json!({
        "page_title": "Orders",
        "admin_slug": &slug.0,
        "settings": &settings,
        "orders": orders,
        "current_page": current_page,
        "total_pages": total_pages,
        "total": total,
        "per_page": per_page,
        "filter_status": status.unwrap_or_default(),
        "currency": currency,
    });
    Template::render("admin/sales/orders", &context)
}
