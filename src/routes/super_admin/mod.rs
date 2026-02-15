#![cfg(feature = "multi-site")]

pub mod auth;
pub mod dashboard;
pub mod health;
pub mod sites;

pub fn routes() -> Vec<rocket::Route> {
    routes![
        auth::setup_page,
        auth::setup_submit,
        auth::login_page,
        auth::login_submit,
        auth::logout,
        dashboard::dashboard,
        dashboard::settings_page,
        health::health_page,
        health::tool_vacuum,
        health::tool_wal_checkpoint,
        health::tool_integrity_check,
        health::tool_session_cleanup,
        health::tool_orphan_scan,
        health::tool_orphan_delete,
        health::tool_unused_tags,
        health::tool_export_content,
        sites::new_site_page,
        sites::new_site_submit,
        sites::edit_site_page,
        sites::edit_site_submit,
        sites::delete_site,
    ]
}
