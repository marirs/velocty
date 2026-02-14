use crate::db::DbPool;
use crate::models::settings::Setting;

/// Generate a license.txt file content for a purchased digital download.
///
/// The output format:
/// ```
/// License for: <item_title>
/// Purchased from: <display_name>
/// Transaction: <transaction_id>
/// Date: <date>
/// --------------------------------------------------
/// <license body from settings>
/// ```
pub fn generate_license_txt(
    pool: &DbPool,
    item_title: &str,
    transaction_id: &str,
    purchase_date: &str,
) -> String {
    let display_name = Setting::get_or(pool, "admin_display_name", "Admin");
    let license_body = Setting::get_or(pool, "downloads_license_template", "");

    format!(
        "License for: {}\nPurchased from: {}\nTransaction: {}\nDate: {}\n--------------------------------------------------\n{}",
        item_title,
        display_name,
        transaction_id,
        purchase_date,
        license_body,
    )
}
