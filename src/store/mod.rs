use std::collections::HashMap;

use chrono::NaiveDateTime;

use crate::models::analytics::{
    CountEntry, DailyCount, FlowNode, OverviewStats, StreamEntry, TagRelation,
};
use crate::models::audit::AuditEntry;
use crate::models::category::{Category, CategoryForm};
use crate::models::comment::{Comment, CommentForm};
use crate::models::design::{Design, DesignTemplate};
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::import::Import;
use crate::models::order::{DownloadToken, License, Order};
use crate::models::passkey::UserPasskey;
use crate::models::portfolio::{PortfolioForm, PortfolioItem};
use crate::models::post::{Post, PostForm};
use crate::models::search::SearchResult;
use crate::models::tag::{Tag, TagForm};
use crate::models::user::User;

pub mod mongo;
pub mod sqlite;

/// Unified data-access trait. Every database operation goes through here.
/// Implementations: `SqliteStore` (wraps rusqlite/r2d2) and `MongoStore` (wraps mongodb).
pub trait Store: Send + Sync {
    // ── Lifecycle ───────────────────────────────────────────────────
    fn run_migrations(&self) -> Result<(), String>;
    fn seed_defaults(&self) -> Result<(), String>;

    // ── Settings ────────────────────────────────────────────────────
    fn setting_get(&self, key: &str) -> Option<String>;
    fn setting_get_or(&self, key: &str, default: &str) -> String {
        self.setting_get(key).unwrap_or_else(|| default.to_string())
    }
    fn setting_get_bool(&self, key: &str) -> bool {
        self.setting_get(key)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }
    fn setting_get_i64(&self, key: &str) -> i64 {
        self.setting_get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
    fn setting_get_f64(&self, key: &str) -> f64 {
        self.setting_get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }
    fn setting_set(&self, key: &str, value: &str) -> Result<(), String>;
    fn setting_set_many(&self, settings: &HashMap<String, String>) -> Result<(), String>;
    fn setting_get_group(&self, prefix: &str) -> HashMap<String, String>;
    fn setting_all(&self) -> HashMap<String, String>;
    fn setting_delete(&self, key: &str) -> Result<(), String>;

    // ── Users ───────────────────────────────────────────────────────
    fn user_get_by_id(&self, id: i64) -> Option<User>;
    fn user_get_by_email(&self, email: &str) -> Option<User>;
    fn user_list_all(&self) -> Vec<User>;
    fn user_list_paginated(&self, role: Option<&str>, limit: i64, offset: i64) -> Vec<User>;
    fn user_count(&self) -> i64;
    fn user_count_filtered(&self, role: Option<&str>) -> i64;
    fn user_count_by_role(&self, role: &str) -> i64;
    fn user_create(
        &self,
        email: &str,
        password_hash: &str,
        display_name: &str,
        role: &str,
    ) -> Result<i64, String>;
    fn user_update_profile(
        &self,
        id: i64,
        display_name: &str,
        email: &str,
        avatar: &str,
    ) -> Result<(), String>;
    fn user_update_role(&self, id: i64, role: &str) -> Result<(), String>;
    fn user_update_password(&self, id: i64, password_hash: &str) -> Result<(), String>;
    fn user_update_avatar(&self, id: i64, avatar: &str) -> Result<(), String>;
    fn user_touch_last_login(&self, id: i64) -> Result<(), String>;
    fn user_update_mfa(
        &self,
        id: i64,
        enabled: bool,
        secret: &str,
        recovery_codes: &str,
    ) -> Result<(), String>;
    fn user_lock(&self, id: i64) -> Result<(), String>;
    fn user_unlock(&self, id: i64) -> Result<(), String>;
    fn user_delete(&self, id: i64) -> Result<(), String>;
    fn user_update_auth_method(&self, id: i64, method: &str, fallback: &str) -> Result<(), String>;

    // ── Posts ────────────────────────────────────────────────────────
    fn post_find_by_id(&self, id: i64) -> Option<Post>;
    fn post_find_by_slug(&self, slug: &str) -> Option<Post>;
    fn post_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Post>;
    fn post_count(&self, status: Option<&str>) -> i64;
    fn post_create(&self, form: &PostForm) -> Result<i64, String>;
    fn post_update(&self, id: i64, form: &PostForm) -> Result<(), String>;
    fn post_delete(&self, id: i64) -> Result<(), String>;
    fn post_prev_published(&self, published_at: &NaiveDateTime) -> Option<Post>;
    fn post_next_published(&self, published_at: &NaiveDateTime) -> Option<Post>;
    fn post_update_status(&self, id: i64, status: &str) -> Result<(), String>;
    fn post_update_seo_score(&self, id: i64, score: i32, issues_json: &str) -> Result<(), String>;
    fn post_archives(&self) -> Vec<(String, String, i64)>;
    fn post_by_year_month(&self, year: &str, month: &str, limit: i64, offset: i64) -> Vec<Post>;
    fn post_count_by_year_month(&self, year: &str, month: &str) -> i64;
    fn post_by_category(&self, category_id: i64, limit: i64, offset: i64) -> Vec<Post>;
    fn post_count_by_category(&self, category_id: i64) -> i64;
    fn post_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<Post>;
    fn post_count_by_tag(&self, tag_id: i64) -> i64;

    // ── Portfolio ───────────────────────────────────────────────────
    fn portfolio_find_by_id(&self, id: i64) -> Option<PortfolioItem>;
    fn portfolio_find_by_slug(&self, slug: &str) -> Option<PortfolioItem>;
    fn portfolio_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<PortfolioItem>;
    fn portfolio_count(&self, status: Option<&str>) -> i64;
    fn portfolio_by_category(
        &self,
        category_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Vec<PortfolioItem>;
    fn portfolio_create(&self, form: &PortfolioForm) -> Result<i64, String>;
    fn portfolio_update(&self, id: i64, form: &PortfolioForm) -> Result<(), String>;
    fn portfolio_update_status(&self, id: i64, status: &str) -> Result<(), String>;
    fn portfolio_delete(&self, id: i64) -> Result<(), String>;
    fn portfolio_increment_likes(&self, id: i64) -> Result<i64, String>;
    fn portfolio_decrement_likes(&self, id: i64) -> Result<i64, String>;
    fn portfolio_update_seo_score(
        &self,
        id: i64,
        score: i32,
        issues_json: &str,
    ) -> Result<(), String>;
    fn portfolio_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<PortfolioItem>;
    fn portfolio_count_by_tag(&self, tag_id: i64) -> i64;

    // ── Comments ────────────────────────────────────────────────────
    fn comment_find_by_id(&self, id: i64) -> Option<Comment>;
    fn comment_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Comment>;
    fn comment_for_post(&self, post_id: i64, content_type: &str) -> Vec<Comment>;
    fn comment_count(&self, status: Option<&str>) -> i64;
    fn comment_create(&self, form: &CommentForm) -> Result<i64, String>;
    fn comment_update_status(&self, id: i64, status: &str) -> Result<(), String>;
    fn comment_delete(&self, id: i64) -> Result<(), String>;

    // ── Categories ──────────────────────────────────────────────────
    fn category_find_by_id(&self, id: i64) -> Option<Category>;
    fn category_find_by_slug(&self, slug: &str) -> Option<Category>;
    fn category_list(&self, type_filter: Option<&str>) -> Vec<Category>;
    fn category_list_paginated(
        &self,
        type_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<Category>;
    fn category_count(&self, type_filter: Option<&str>) -> i64;
    fn category_for_content(&self, content_id: i64, content_type: &str) -> Vec<Category>;
    fn category_count_items(&self, category_id: i64) -> i64;
    fn category_create(&self, form: &CategoryForm) -> Result<i64, String>;
    fn category_update(&self, id: i64, form: &CategoryForm) -> Result<(), String>;
    fn category_delete(&self, id: i64) -> Result<(), String>;
    fn category_set_show_in_nav(&self, id: i64, show: bool) -> Result<(), String>;
    fn category_list_nav_visible(&self, type_filter: Option<&str>) -> Vec<Category>;
    fn category_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        category_ids: &[i64],
    ) -> Result<(), String>;

    // ── Tags ────────────────────────────────────────────────────────
    fn tag_find_by_id(&self, id: i64) -> Option<Tag>;
    fn tag_find_by_slug(&self, slug: &str) -> Option<Tag>;
    fn tag_list(&self) -> Vec<Tag>;
    fn tag_list_paginated(&self, limit: i64, offset: i64) -> Vec<Tag>;
    fn tag_count(&self) -> i64;
    fn tag_for_content(&self, content_id: i64, content_type: &str) -> Vec<Tag>;
    fn tag_count_items(&self, tag_id: i64) -> i64;
    fn tag_create(&self, form: &TagForm) -> Result<i64, String>;
    fn tag_update(&self, id: i64, form: &TagForm) -> Result<(), String>;
    fn tag_delete(&self, id: i64) -> Result<(), String>;
    fn tag_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        tag_ids: &[i64],
    ) -> Result<(), String>;
    fn tag_find_or_create(&self, name: &str) -> Result<i64, String>;

    // ── Designs ─────────────────────────────────────────────────────
    fn design_find_by_id(&self, id: i64) -> Option<Design>;
    fn design_find_by_slug(&self, slug: &str) -> Option<Design>;
    fn design_active(&self) -> Option<Design>;
    fn design_list(&self) -> Vec<Design>;
    fn design_activate(&self, id: i64) -> Result<(), String>;
    fn design_create(&self, name: &str) -> Result<i64, String>;
    fn design_duplicate(&self, id: i64, new_name: &str) -> Result<i64, String>;
    fn design_delete(&self, id: i64) -> Result<(), String>;

    // ── Design Templates ────────────────────────────────────────────
    fn design_template_for_design(&self, design_id: i64) -> Vec<DesignTemplate>;
    fn design_template_get(&self, design_id: i64, template_type: &str) -> Option<DesignTemplate>;
    fn design_template_upsert(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
    ) -> Result<(), String>;
    fn design_template_upsert_full(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
        grapesjs_data: &str,
    ) -> Result<(), String>;

    // ── Audit Log ───────────────────────────────────────────────────
    fn audit_log(
        &self,
        user_id: Option<i64>,
        user_name: Option<&str>,
        action: &str,
        entity_type: Option<&str>,
        entity_id: Option<i64>,
        entity_title: Option<&str>,
        details: Option<&str>,
        ip_address: Option<&str>,
    );
    fn audit_list(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Vec<AuditEntry>;
    fn audit_count(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
    ) -> i64;
    fn audit_distinct_actions(&self) -> Vec<String>;
    fn audit_distinct_entity_types(&self) -> Vec<String>;
    fn audit_cleanup(&self, max_age_days: i64) -> Result<usize, String>;

    // ── Firewall: Bans ──────────────────────────────────────────────
    fn fw_is_banned(&self, ip: &str) -> bool;
    fn fw_ban_create(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        expires_at: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String>;
    fn fw_ban_create_with_duration(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        duration: &str,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String>;
    fn fw_unban(&self, ip: &str) -> Result<usize, String>;
    fn fw_unban_by_id(&self, id: i64) -> Result<usize, String>;
    fn fw_active_bans(&self, limit: i64, offset: i64) -> Vec<FwBan>;
    fn fw_active_count(&self) -> i64;
    fn fw_all_bans(&self, limit: i64, offset: i64) -> Vec<FwBan>;
    fn fw_expire_stale(&self);

    // ── Firewall: Events ────────────────────────────────────────────
    fn fw_event_log(
        &self,
        ip: &str,
        event_type: &str,
        detail: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
        request_path: Option<&str>,
    );
    fn fw_event_recent(&self, event_type: Option<&str>, limit: i64, offset: i64) -> Vec<FwEvent>;
    fn fw_event_count_all(&self, event_type: Option<&str>) -> i64;
    fn fw_event_count_since_hours(&self, hours: i64) -> i64;
    fn fw_event_count_for_ip_since(&self, ip: &str, event_type: &str, minutes: i64) -> i64;
    fn fw_event_top_ips(&self, limit: i64) -> Vec<(String, i64)>;
    fn fw_event_counts_by_type(&self) -> Vec<(String, i64)>;

    // ── Analytics ───────────────────────────────────────────────────
    fn analytics_record(
        &self,
        path: &str,
        ip_hash: &str,
        country: Option<&str>,
        city: Option<&str>,
        referrer: Option<&str>,
        user_agent: Option<&str>,
        device_type: Option<&str>,
        browser: Option<&str>,
    ) -> Result<(), String>;
    fn analytics_overview(&self, from: &str, to: &str) -> OverviewStats;
    fn analytics_flow_data(&self, from: &str, to: &str) -> Vec<FlowNode>;
    fn analytics_geo_data(&self, from: &str, to: &str) -> Vec<CountEntry>;
    fn analytics_stream_data(&self, from: &str, to: &str) -> Vec<StreamEntry>;
    fn analytics_calendar_data(&self, from: &str, to: &str) -> Vec<DailyCount>;
    fn analytics_top_portfolio(&self, from: &str, to: &str, limit: i64) -> Vec<CountEntry>;
    fn analytics_top_referrers(&self, from: &str, to: &str, limit: i64) -> Vec<CountEntry>;
    fn analytics_tag_relations(&self) -> Vec<TagRelation>;

    // ── Orders ──────────────────────────────────────────────────────
    fn order_find_by_id(&self, id: i64) -> Option<Order>;
    fn order_find_by_provider_order_id(&self, provider_order_id: &str) -> Option<Order>;
    fn order_list(&self, limit: i64, offset: i64) -> Vec<Order>;
    fn order_list_by_status(&self, status: &str, limit: i64, offset: i64) -> Vec<Order>;
    fn order_list_by_email(&self, email: &str, limit: i64, offset: i64) -> Vec<Order>;
    fn order_list_by_portfolio(&self, portfolio_id: i64) -> Vec<Order>;
    fn order_count(&self) -> i64;
    fn order_count_by_status(&self, status: &str) -> i64;
    fn order_total_revenue(&self) -> f64;
    fn order_revenue_by_period(&self, days: i64) -> f64;
    fn order_create(
        &self,
        portfolio_id: i64,
        buyer_email: &str,
        buyer_name: &str,
        amount: f64,
        currency: &str,
        provider: &str,
        provider_order_id: &str,
        status: &str,
    ) -> Result<i64, String>;
    fn order_update_status(&self, id: i64, status: &str) -> Result<(), String>;
    fn order_update_provider_order_id(
        &self,
        id: i64,
        provider_order_id: &str,
    ) -> Result<(), String>;
    fn order_update_buyer_info(
        &self,
        id: i64,
        buyer_email: &str,
        buyer_name: &str,
    ) -> Result<(), String>;
    fn order_find_completed_by_email_and_portfolio(
        &self,
        email: &str,
        portfolio_id: i64,
    ) -> Option<Order>;

    // ── Download Tokens ─────────────────────────────────────────────
    fn download_token_find_by_token(&self, token: &str) -> Option<DownloadToken>;
    fn download_token_find_by_order(&self, order_id: i64) -> Option<DownloadToken>;
    fn download_token_increment(&self, id: i64) -> Result<(), String>;
    fn download_token_create(
        &self,
        order_id: i64,
        token: &str,
        max_downloads: i64,
        expires_at: NaiveDateTime,
    ) -> Result<i64, String>;

    // ── Licenses ────────────────────────────────────────────────────
    fn license_find_by_order(&self, order_id: i64) -> Option<License>;
    fn license_find_by_key(&self, key: &str) -> Option<License>;
    fn license_create(&self, order_id: i64, license_key: &str) -> Result<i64, String>;

    // ── Passkeys ────────────────────────────────────────────────────
    fn passkey_list_for_user(&self, user_id: i64) -> Vec<UserPasskey>;
    fn passkey_get_by_credential_id(&self, credential_id: &str) -> Option<UserPasskey>;
    fn passkey_count_for_user(&self, user_id: i64) -> i64;
    fn passkey_create(
        &self,
        user_id: i64,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
        transports: &str,
        name: &str,
    ) -> Result<i64, String>;
    fn passkey_update_sign_count(&self, credential_id: &str, sign_count: i64)
        -> Result<(), String>;
    fn passkey_update_public_key(
        &self,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
    ) -> Result<(), String>;
    fn passkey_delete(&self, id: i64, user_id: i64) -> Result<(), String>;
    fn passkey_delete_all_for_user(&self, user_id: i64) -> Result<(), String>;

    // ── Imports ─────────────────────────────────────────────────────
    fn import_list(&self) -> Vec<Import>;
    fn import_create(
        &self,
        source: &str,
        filename: Option<&str>,
        posts_count: i64,
        portfolio_count: i64,
        comments_count: i64,
        skipped_count: i64,
        log: Option<&str>,
    ) -> Result<i64, String>;

    // ── Search (FTS) ────────────────────────────────────────────────
    fn search_create_fts_table(&self) -> Result<(), String>;
    fn search_rebuild_index(&self) -> Result<usize, String>;
    fn search_upsert_item(
        &self,
        item_type: &str,
        item_id: i64,
        title: &str,
        html_body: &str,
        slug: &str,
        image: Option<&str>,
        date: Option<&str>,
        is_published: bool,
    );
    fn search_remove_item(&self, item_type: &str, item_id: i64);
    fn search_query(&self, query: &str, limit: i64) -> Vec<SearchResult>;

    // ── Sessions (used by auth, firewall) ───────────────────────────
    fn session_create(&self, user_id: i64, token: &str, expires_at: &str) -> Result<(), String>;
    fn session_create_full(
        &self,
        user_id: i64,
        token: &str,
        expires_at: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String>;
    fn session_get_user_id(&self, token: &str) -> Option<i64>;
    fn session_get_user(&self, token: &str) -> Option<User> {
        let uid = self.session_get_user_id(token)?;
        self.user_get_by_id(uid)
    }
    fn session_validate(&self, token: &str) -> bool {
        self.session_get_user_id(token).is_some()
    }
    fn session_delete(&self, token: &str) -> Result<(), String>;
    fn session_delete_for_user(&self, user_id: i64) -> Result<(), String>;
    fn session_cleanup_expired(&self);
    fn session_count_recent_by_ip(&self, ip_hash: &str, minutes: i64) -> i64;

    // ── Magic links / password reset tokens ──────────────────────────
    fn magic_link_create(
        &self,
        token: &str,
        email: &str,
        expires_minutes: i64,
    ) -> Result<(), String>;
    fn magic_link_verify(&self, token: &str) -> Result<String, String>;

    // ── Likes (portfolio) ───────────────────────────────────────────
    fn like_exists(&self, portfolio_id: i64, ip_hash: &str) -> bool;
    fn like_add(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String>;
    fn like_remove(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String>;

    // ── Analytics pruning ───────────────────────────────────────────
    fn analytics_prune(&self, before_date: &str) -> Result<usize, String>;
    fn analytics_count(&self) -> i64;

    // ── Health / maintenance ──────────────────────────────────────────
    /// Return the database backend name: "sqlite" or "mongodb"
    fn db_backend(&self) -> &str;

    /// Gather content statistics for the health report.
    /// Returns (posts_total, posts_published, posts_draft, portfolio_total,
    ///          comments_total, comments_pending, categories_count, tags_count,
    ///          sessions_total, sessions_expired)
    fn health_content_stats(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64);

    /// Return the set of filenames referenced in posts, portfolio, and settings
    /// (for orphan file detection).
    fn health_referenced_files(&self) -> std::collections::HashSet<String>;

    /// Delete expired sessions, return count deleted.
    fn health_session_cleanup(&self) -> Result<u64, String>;

    /// Delete tags not referenced by any post or portfolio item, return count deleted.
    fn health_unused_tags_cleanup(&self) -> Result<u64, String>;

    /// Delete analytics events older than `days`, return count deleted.
    fn health_analytics_prune(&self, days: u64) -> Result<u64, String>;

    /// Export all content as JSON Value.
    fn health_export_content(&self) -> Result<serde_json::Value, String>;

    // ── Background tasks ────────────────────────────────────────────
    /// Delete expired and used magic link tokens, return count deleted.
    fn magic_link_cleanup(&self) -> Result<usize, String>;

    /// Publish posts/portfolio items whose `published_at` is in the past
    /// and status is 'scheduled'. Returns total count published.
    fn task_publish_scheduled(&self) -> Result<usize, String>;

    /// Delete sessions older than `max_age_days` or already expired.
    fn task_cleanup_sessions(&self, max_age_days: i64) -> Result<usize, String>;

    /// Delete analytics page views older than `max_age_days`.
    fn task_cleanup_analytics(&self, max_age_days: i64) -> Result<usize, String>;

    // ── Email queue (built-in MTA) ─────────────────────────────────
    /// Push an email onto the retry queue.
    fn mta_queue_push(
        &self,
        to: &str,
        from: &str,
        subject: &str,
        body: &str,
    ) -> Result<i64, String>;

    /// Fetch pending messages ready for delivery (status='pending', next_retry_at <= now).
    fn mta_queue_pending(&self, limit: i64) -> Vec<crate::mta::queue::QueuedEmail>;

    /// Update the status of a queued message (and optionally error + next_retry_at).
    fn mta_queue_update_status(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
        next_retry: Option<&str>,
    ) -> Result<(), String>;

    /// Count emails sent (status='sent') in the last hour.
    fn mta_queue_sent_last_hour(&self) -> Result<u64, String>;

    /// Delete old queue entries older than `days`.
    fn mta_queue_cleanup(&self, days: u64) -> Result<u64, String>;

    // ── Raw execute (escape hatch for migrations/health tools) ──────
    fn raw_execute(&self, sql: &str) -> Result<usize, String>;
    fn raw_query_i64(&self, sql: &str) -> Result<i64, String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::sqlite::SqliteStore;

    /// Create a fresh in-memory SqliteStore with migrations + seed applied.
    fn test_store() -> SqliteStore {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .expect("Failed to create in-memory pool");
        let store = SqliteStore::new(pool);
        store.run_migrations().expect("migrations failed");
        store.seed_defaults().expect("seed failed");
        store
    }

    // ── Settings ────────────────────────────────────────────────────

    #[test]
    fn test_setting_get_set() {
        let s = test_store();
        assert!(s.setting_get("nonexistent_key_xyz").is_none());
        s.setting_set("test_key", "hello").unwrap();
        assert_eq!(s.setting_get("test_key"), Some("hello".to_string()));
    }

    #[test]
    fn test_setting_get_or() {
        let s = test_store();
        assert_eq!(s.setting_get_or("missing", "fallback"), "fallback");
        s.setting_set("present", "val").unwrap();
        assert_eq!(s.setting_get_or("present", "fallback"), "val");
    }

    #[test]
    fn test_setting_get_bool() {
        let s = test_store();
        assert!(!s.setting_get_bool("missing_bool"));
        s.setting_set("flag_true", "true").unwrap();
        s.setting_set("flag_one", "1").unwrap();
        s.setting_set("flag_false", "false").unwrap();
        assert!(s.setting_get_bool("flag_true"));
        assert!(s.setting_get_bool("flag_one"));
        assert!(!s.setting_get_bool("flag_false"));
    }

    #[test]
    fn test_setting_get_i64() {
        let s = test_store();
        assert_eq!(s.setting_get_i64("missing_num"), 0);
        s.setting_set("num", "42").unwrap();
        assert_eq!(s.setting_get_i64("num"), 42);
    }

    #[test]
    fn test_setting_set_many() {
        let s = test_store();
        let mut batch = HashMap::new();
        batch.insert("batch_a".to_string(), "1".to_string());
        batch.insert("batch_b".to_string(), "2".to_string());
        s.setting_set_many(&batch).unwrap();
        assert_eq!(s.setting_get("batch_a"), Some("1".to_string()));
        assert_eq!(s.setting_get("batch_b"), Some("2".to_string()));
    }

    #[test]
    fn test_setting_get_group() {
        let s = test_store();
        s.setting_set("grp_alpha", "a").unwrap();
        s.setting_set("grp_beta", "b").unwrap();
        s.setting_set("other_key", "c").unwrap();
        let group = s.setting_get_group("grp_");
        assert_eq!(group.len(), 2);
        assert_eq!(group.get("grp_alpha").unwrap(), "a");
    }

    #[test]
    fn test_setting_all() {
        let s = test_store();
        let all = s.setting_all();
        // seed_defaults populates many settings
        assert!(all.len() > 5);
    }

    #[test]
    fn test_setting_delete() {
        let s = test_store();
        s.setting_set("del_me", "val").unwrap();
        assert!(s.setting_get("del_me").is_some());
        s.setting_delete("del_me").unwrap();
        assert!(s.setting_get("del_me").is_none());
    }

    // ── Users ───────────────────────────────────────────────────────

    #[test]
    fn test_user_create_and_find() {
        let s = test_store();
        let id = s
            .user_create("test@example.com", "hash123", "Test User", "admin")
            .unwrap();
        assert!(id > 0);

        let user = s.user_get_by_id(id).expect("user not found by id");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.display_name, "Test User");
        assert_eq!(user.role, "admin");

        let user2 = s
            .user_get_by_email("test@example.com")
            .expect("user not found by email");
        assert_eq!(user2.id, id);
    }

    #[test]
    fn test_user_count_and_roles() {
        let s = test_store();
        assert_eq!(s.user_count(), 0);
        s.user_create("a@a.com", "h", "A", "admin").unwrap();
        s.user_create("b@b.com", "h", "B", "editor").unwrap();
        s.user_create("c@c.com", "h", "C", "editor").unwrap();
        assert_eq!(s.user_count(), 3);
        assert_eq!(s.user_count_by_role("admin"), 1);
        assert_eq!(s.user_count_by_role("editor"), 2);
        assert_eq!(s.user_count_filtered(Some("editor")), 2);
        assert_eq!(s.user_count_filtered(None), 3);
    }

    #[test]
    fn test_user_update_profile() {
        let s = test_store();
        let id = s.user_create("u@u.com", "h", "Old", "admin").unwrap();
        s.user_update_profile(id, "New Name", "new@u.com", "/avatar.png")
            .unwrap();
        let u = s.user_get_by_id(id).unwrap();
        assert_eq!(u.display_name, "New Name");
        assert_eq!(u.email, "new@u.com");
        assert_eq!(u.avatar, "/avatar.png");
    }

    #[test]
    fn test_user_lock_unlock_delete() {
        let s = test_store();
        let id = s.user_create("lock@t.com", "h", "Lock", "editor").unwrap();
        s.user_lock(id).unwrap();
        let u = s.user_get_by_id(id).unwrap();
        assert_eq!(u.status, "locked");

        s.user_unlock(id).unwrap();
        let u = s.user_get_by_id(id).unwrap();
        assert_eq!(u.status, "active");

        s.user_delete(id).unwrap();
        assert!(s.user_get_by_id(id).is_none());
    }

    // ── Posts ───────────────────────────────────────────────────────

    #[test]
    fn test_post_crud() {
        let s = test_store();
        let form = PostForm {
            title: "Test Post".to_string(),
            slug: "test-post".to_string(),
            content_json: "{}".to_string(),
            content_html: "<p>Hello</p>".to_string(),
            excerpt: Some("excerpt".to_string()),
            featured_image: None,
            meta_title: None,
            meta_description: None,
            status: "draft".to_string(),
            published_at: None,
            category_ids: None,
            tag_ids: None,
        };
        let id = s.post_create(&form).unwrap();
        assert!(id > 0);

        let post = s.post_find_by_id(id).unwrap();
        assert_eq!(post.title, "Test Post");
        assert_eq!(post.slug, "test-post");

        let post2 = s.post_find_by_slug("test-post").unwrap();
        assert_eq!(post2.id, id);

        assert_eq!(s.post_count(None), 1);
        assert_eq!(s.post_count(Some("draft")), 1);
        assert_eq!(s.post_count(Some("published")), 0);

        let list = s.post_list(None, 10, 0);
        assert_eq!(list.len(), 1);

        s.post_delete(id).unwrap();
        assert!(s.post_find_by_id(id).is_none());
        assert_eq!(s.post_count(None), 0);
    }

    // ── Portfolio ───────────────────────────────────────────────────

    #[test]
    fn test_portfolio_crud() {
        let s = test_store();
        let form = PortfolioForm {
            title: "Project".to_string(),
            slug: "project".to_string(),
            description_json: None,
            description_html: Some("<p>Desc</p>".to_string()),
            image_path: "img.jpg".to_string(),
            thumbnail_path: None,
            meta_title: None,
            meta_description: None,
            sell_enabled: None,
            price: None,
            purchase_note: None,
            payment_provider: None,
            download_file_path: None,
            status: "published".to_string(),
            published_at: None,
            category_ids: None,
            tag_ids: None,
        };
        let id = s.portfolio_create(&form).unwrap();
        assert!(id > 0);

        let item = s.portfolio_find_by_id(id).unwrap();
        assert_eq!(item.title, "Project");

        let item2 = s.portfolio_find_by_slug("project").unwrap();
        assert_eq!(item2.id, id);

        assert_eq!(s.portfolio_count(None), 1);

        s.portfolio_delete(id).unwrap();
        assert!(s.portfolio_find_by_id(id).is_none());
    }

    // ── Categories ──────────────────────────────────────────────────

    #[test]
    fn test_category_crud() {
        let s = test_store();
        let form = CategoryForm {
            name: "Tech".to_string(),
            slug: "tech".to_string(),
            r#type: "post".to_string(),
        };
        let id = s.category_create(&form).unwrap();
        assert!(id > 0);

        let cat = s.category_find_by_id(id).unwrap();
        assert_eq!(cat.name, "Tech");
        assert_eq!(cat.slug, "tech");

        let cat2 = s.category_find_by_slug("tech").unwrap();
        assert_eq!(cat2.id, id);

        let list = s.category_list(Some("post"));
        assert!(list.iter().any(|c| c.id == id));

        s.category_delete(id).unwrap();
        assert!(s.category_find_by_id(id).is_none());
    }

    // ── Tags ────────────────────────────────────────────────────────

    #[test]
    fn test_tag_crud() {
        let s = test_store();
        let form = TagForm {
            name: "rust".to_string(),
            slug: "rust".to_string(),
        };
        let id = s.tag_create(&form).unwrap();
        assert!(id > 0);

        let tag = s.tag_find_by_id(id).unwrap();
        assert_eq!(tag.name, "rust");

        let tag2 = s.tag_find_by_slug("rust").unwrap();
        assert_eq!(tag2.id, id);

        assert!(s.tag_count() >= 1);

        s.tag_delete(id).unwrap();
        assert!(s.tag_find_by_id(id).is_none());
    }

    #[test]
    fn test_tag_find_or_create() {
        let s = test_store();
        let id1 = s.tag_find_or_create("newtag").unwrap();
        let id2 = s.tag_find_or_create("newtag").unwrap();
        assert_eq!(id1, id2); // idempotent
    }

    // ── Comments ────────────────────────────────────────────────────

    #[test]
    fn test_comment_crud() {
        let s = test_store();
        // Create a post first
        let post_id = s
            .post_create(&PostForm {
                title: "P".to_string(),
                slug: "p".to_string(),
                content_json: "{}".to_string(),
                content_html: "".to_string(),
                excerpt: None,
                featured_image: None,
                meta_title: None,
                meta_description: None,
                status: "published".to_string(),
                published_at: None,
                category_ids: None,
                tag_ids: None,
            })
            .unwrap();

        let form = CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: "Alice".to_string(),
            author_email: Some("alice@test.com".to_string()),
            body: "Great post!".to_string(),
            honeypot: None,
            parent_id: None,
        };
        let cid = s.comment_create(&form).unwrap();
        assert!(cid > 0);

        let c = s.comment_find_by_id(cid).unwrap();
        assert_eq!(c.author_name, "Alice");

        assert_eq!(s.comment_count(None), 1);

        s.comment_update_status(cid, "approved").unwrap();
        let c2 = s.comment_find_by_id(cid).unwrap();
        assert_eq!(c2.status, "approved");

        s.comment_delete(cid).unwrap();
        assert!(s.comment_find_by_id(cid).is_none());
    }

    // ── Audit ───────────────────────────────────────────────────────

    #[test]
    fn test_audit_log() {
        let s = test_store();
        s.audit_log(
            Some(1),
            Some("admin"),
            "test_action",
            Some("post"),
            Some(42),
            Some("My Post"),
            None,
            None,
        );
        let entries = s.audit_list(Some("test_action"), None, None, 10, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "test_action");
        assert_eq!(s.audit_count(Some("test_action"), None, None), 1);
    }

    // ── Sessions ────────────────────────────────────────────────────

    #[test]
    fn test_session_lifecycle() {
        let s = test_store();
        let uid = s.user_create("sess@t.com", "h", "S", "admin").unwrap();
        s.session_create(uid, "tok123", "2099-12-31 23:59:59")
            .unwrap();
        assert_eq!(s.session_get_user_id("tok123"), Some(uid));

        s.session_delete("tok123").unwrap();
        assert_eq!(s.session_get_user_id("tok123"), None);
    }

    // ── Likes ───────────────────────────────────────────────────────

    #[test]
    fn test_likes() {
        let s = test_store();
        let pid = s
            .portfolio_create(&PortfolioForm {
                title: "L".to_string(),
                slug: "l".to_string(),
                description_json: None,
                description_html: None,
                image_path: "x.jpg".to_string(),
                thumbnail_path: None,
                meta_title: None,
                meta_description: None,
                sell_enabled: None,
                price: None,
                purchase_note: None,
                payment_provider: None,
                download_file_path: None,
                status: "published".to_string(),
                published_at: None,
                category_ids: None,
                tag_ids: None,
            })
            .unwrap();

        assert!(!s.like_exists(pid, "iphash1"));
        s.like_add(pid, "iphash1").unwrap();
        assert!(s.like_exists(pid, "iphash1"));
        s.like_remove(pid, "iphash1").unwrap();
        assert!(!s.like_exists(pid, "iphash1"));
    }

    // ── Designs ─────────────────────────────────────────────────────

    #[test]
    fn test_design_active() {
        let s = test_store();
        // seed_defaults creates the default design
        let active = s.design_active();
        assert!(active.is_some());
    }

    // ── Raw execute ─────────────────────────────────────────────────

    #[test]
    fn test_raw_query() {
        let s = test_store();
        let count = s.raw_query_i64("SELECT COUNT(*) FROM settings").unwrap();
        assert!(count > 0);
    }

    // ── DbPool bridge ───────────────────────────────────────────────

    #[test]
    fn test_dbpool_bridge() {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .expect("pool");
        // Use pool directly as &dyn Store via the bridge impl
        let store: &dyn Store = &pool;
        store.run_migrations().unwrap();
        store.seed_defaults().unwrap();
        store.setting_set("bridge_test", "works").unwrap();
        assert_eq!(store.setting_get("bridge_test"), Some("works".to_string()));
    }
}
