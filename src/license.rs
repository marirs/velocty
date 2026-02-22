use crate::store::Store;

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
    store: &dyn Store,
    item_title: &str,
    transaction_id: &str,
    purchase_date: &str,
) -> String {
    let display_name = store.setting_get_or("admin_display_name", "Admin");
    let license_body = store.setting_get_or("downloads_license_template", "");

    format!(
        "License for: {}\nPurchased from: {}\nTransaction: {}\nDate: {}\n--------------------------------------------------\n{}",
        item_title,
        display_name,
        transaction_id,
        purchase_date,
        license_body,
    )
}
