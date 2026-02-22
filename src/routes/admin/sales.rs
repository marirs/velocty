use std::sync::Arc;

use rocket::State;
use rocket_dyn_templates::Template;
use serde_json::json;

use crate::security::auth::AdminUser;
use crate::store::Store;
use crate::AdminSlug;

// ── Sales ──────────────────────────────────────────────

#[get("/sales")]
pub fn sales_dashboard(
    _admin: AdminUser,
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
) -> Template {
    let settings = store.setting_all();
    let total_revenue = store.order_total_revenue();
    let revenue_30d = store.order_revenue_by_period(30);
    let revenue_7d = store.order_revenue_by_period(7);
    let total_orders = store.order_count();
    let completed_orders = store.order_count_by_status("completed");
    let pending_orders = store.order_count_by_status("pending");
    let recent_orders = store.order_list(10, 0);
    let currency = settings
        .get("commerce_currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());

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
    store: &State<Arc<dyn Store>>,
    slug: &State<AdminSlug>,
    page: Option<i64>,
    status: Option<String>,
) -> Template {
    let settings = store.setting_all();
    let per_page: i64 = 25;
    let current_page = page.unwrap_or(1).max(1);
    let offset = (current_page - 1) * per_page;

    let (orders, total) = match status.as_deref() {
        Some(s) if !s.is_empty() => (
            store.order_list_by_status(s, per_page, offset),
            store.order_count_by_status(s),
        ),
        _ => (store.order_list(per_page, offset), store.order_count()),
    };

    let total_pages = (total as f64 / per_page as f64).ceil() as i64;
    let currency = settings
        .get("commerce_currency")
        .cloned()
        .unwrap_or_else(|| "USD".to_string());

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
