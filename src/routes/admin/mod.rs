use rocket::fs::TempFile;

use crate::AdminSlug;

pub mod api;
pub mod categories;
pub mod comments;
pub mod dashboard;
pub mod designs;
pub mod firewall;
pub mod health;
pub mod import;
pub mod media;
pub mod portfolio;
pub mod posts;
pub mod sales;
pub mod settings;
pub mod users;

/// Helper: get the admin base path from managed state
pub(crate) fn admin_base(slug: &AdminSlug) -> String {
    format!("/{}", slug.0)
}

pub(crate) async fn save_upload(file: &mut TempFile<'_>, prefix: &str) -> Option<String> {
    let ext = file
        .content_type()
        .and_then(|ct| ct.extension())
        .map(|e| e.to_string())
        .unwrap_or_else(|| "jpg".to_string());
    let filename = format!("{}_{}.{}", prefix, uuid::Uuid::new_v4(), ext);
    let dest = std::path::Path::new("website/site/uploads").join(&filename);
    let _ = std::fs::create_dir_all("website/site/uploads");
    match file.persist_to(&dest).await {
        Ok(_) => Some(filename),
        Err(_) => None,
    }
}

pub fn routes() -> Vec<rocket::Route> {
    routes![
        dashboard::dashboard,
        posts::posts_list,
        posts::posts_new,
        posts::posts_edit,
        posts::posts_delete,
        posts::posts_create,
        posts::posts_update,
        portfolio::portfolio_list,
        portfolio::portfolio_new,
        portfolio::portfolio_edit,
        portfolio::portfolio_delete,
        portfolio::portfolio_create,
        portfolio::portfolio_update,
        comments::comments_list,
        comments::comment_approve,
        comments::comment_spam,
        comments::comment_delete,
        categories::categories_list,
        categories::category_create,
        categories::api_category_create,
        categories::category_update,
        categories::category_delete,
        categories::tags_list,
        categories::tag_delete,
        designs::designs_list,
        designs::design_activate,
        import::import_page,
        import::import_wordpress,
        import::import_velocty,
        settings::settings_page,
        settings::settings_save,
        media::media_library,
        media::media_delete,
        media::upload_image,
        media::upload_font,
        health::health_page,
        health::health_vacuum,
        health::health_wal_checkpoint,
        health::health_integrity_check,
        health::health_session_cleanup,
        health::health_orphan_scan,
        health::health_orphan_delete,
        health::health_unused_tags,
        health::health_analytics_prune,
        health::health_export_db,
        health::health_export_content,
        health::health_mongo_ping,
        users::mfa_setup,
        users::mfa_verify,
        users::mfa_disable,
        users::mfa_recovery_codes,
        sales::sales_dashboard,
        sales::sales_orders,
        firewall::firewall_dashboard,
        firewall::firewall_ban,
        firewall::firewall_unban,
        users::users_list,
        users::user_create,
        users::user_update,
        users::user_avatar_upload,
        users::user_lock,
        users::user_unlock,
        users::user_reset_password,
        users::user_delete,
    ]
}

pub fn api_routes() -> Vec<rocket::Route> {
    routes![
        api::stats_overview,
        api::stats_flow,
        api::stats_geo,
        api::stats_stream,
        api::stats_calendar,
        api::stats_top_portfolio,
        api::stats_top_referrers,
        api::stats_tags,
        api::set_theme,
        api::seo_check_post,
        api::seo_check_portfolio,
    ]
}
