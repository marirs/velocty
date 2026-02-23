use std::collections::HashMap;

use chrono::NaiveDateTime;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

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

use super::Store;

pub type DbPool = Pool<SqliteConnectionManager>;

/// SQLite-backed implementation of the Store trait.
/// Wraps the existing r2d2 connection pool and delegates to model methods.
pub struct SqliteStore {
    pub pool: DbPool,
}

impl SqliteStore {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub fn new_at(path: &str) -> Result<Self, String> {
        let pool = crate::db::init_pool_at(path)?;
        Ok(Self { pool })
    }
}

impl Store for SqliteStore {
    // ── Lifecycle ───────────────────────────────────────────────────

    fn run_migrations(&self) -> Result<(), String> {
        crate::db::run_migrations(&self.pool).map_err(|e| e.to_string())
    }

    fn seed_defaults(&self) -> Result<(), String> {
        crate::db::seed_defaults(&self.pool).map_err(|e| e.to_string())
    }

    // ── Settings ────────────────────────────────────────────────────

    fn setting_get(&self, key: &str) -> Option<String> {
        crate::models::settings::Setting::get(&self.pool, key)
    }

    fn setting_set(&self, key: &str, value: &str) -> Result<(), String> {
        crate::models::settings::Setting::set(&self.pool, key, value)
    }

    fn setting_set_many(&self, settings: &HashMap<String, String>) -> Result<(), String> {
        crate::models::settings::Setting::set_many(&self.pool, settings)
    }

    fn setting_get_group(&self, prefix: &str) -> HashMap<String, String> {
        crate::models::settings::Setting::get_group(&self.pool, prefix)
    }

    fn setting_all(&self) -> HashMap<String, String> {
        crate::models::settings::Setting::all(&self.pool)
    }

    fn setting_delete(&self, key: &str) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Users ───────────────────────────────────────────────────────

    fn user_get_by_id(&self, id: i64) -> Option<User> {
        User::get_by_id(&self.pool, id)
    }

    fn user_get_by_email(&self, email: &str) -> Option<User> {
        User::get_by_email(&self.pool, email)
    }

    fn user_list_all(&self) -> Vec<User> {
        User::list_all(&self.pool)
    }

    fn user_list_paginated(&self, role: Option<&str>, limit: i64, offset: i64) -> Vec<User> {
        User::list_paginated(&self.pool, role, limit, offset)
    }

    fn user_count(&self) -> i64 {
        User::count(&self.pool)
    }

    fn user_count_filtered(&self, role: Option<&str>) -> i64 {
        User::count_filtered(&self.pool, role)
    }

    fn user_count_by_role(&self, role: &str) -> i64 {
        User::count_by_role(&self.pool, role)
    }

    fn user_create(
        &self,
        email: &str,
        password_hash: &str,
        display_name: &str,
        role: &str,
    ) -> Result<i64, String> {
        User::create(&self.pool, email, password_hash, display_name, role)
    }

    fn user_update_profile(
        &self,
        id: i64,
        display_name: &str,
        email: &str,
        avatar: &str,
    ) -> Result<(), String> {
        User::update_profile(&self.pool, id, display_name, email, avatar)
    }

    fn user_update_role(&self, id: i64, role: &str) -> Result<(), String> {
        User::update_role(&self.pool, id, role)
    }

    fn user_update_password(&self, id: i64, password_hash: &str) -> Result<(), String> {
        User::update_password(&self.pool, id, password_hash)
    }

    fn user_update_avatar(&self, id: i64, avatar: &str) -> Result<(), String> {
        User::update_avatar(&self.pool, id, avatar)
    }

    fn user_touch_last_login(&self, id: i64) -> Result<(), String> {
        User::touch_last_login(&self.pool, id)
    }

    fn user_update_mfa(
        &self,
        id: i64,
        enabled: bool,
        secret: &str,
        recovery_codes: &str,
    ) -> Result<(), String> {
        User::update_mfa(&self.pool, id, enabled, secret, recovery_codes)
    }

    fn user_lock(&self, id: i64) -> Result<(), String> {
        User::lock(&self.pool, id)
    }

    fn user_unlock(&self, id: i64) -> Result<(), String> {
        User::unlock(&self.pool, id)
    }

    fn user_delete(&self, id: i64) -> Result<(), String> {
        User::delete(&self.pool, id)
    }

    fn user_update_auth_method(&self, id: i64, method: &str, fallback: &str) -> Result<(), String> {
        User::update_auth_method(&self.pool, id, method, fallback)
    }
    fn user_set_force_password_change(&self, id: i64, force: bool) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let val: i32 = if force { 1 } else { 0 };
        conn.execute(
            "UPDATE users SET force_password_change = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            rusqlite::params![val, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Posts ────────────────────────────────────────────────────────

    fn post_find_by_id(&self, id: i64) -> Option<Post> {
        Post::find_by_id(&self.pool, id)
    }

    fn post_find_by_slug(&self, slug: &str) -> Option<Post> {
        Post::find_by_slug(&self.pool, slug)
    }

    fn post_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Post> {
        Post::list(&self.pool, status, limit, offset)
    }

    fn post_count(&self, status: Option<&str>) -> i64 {
        Post::count(&self.pool, status)
    }

    fn post_create(&self, form: &PostForm) -> Result<i64, String> {
        Post::create(&self.pool, form)
    }

    fn post_update(&self, id: i64, form: &PostForm) -> Result<(), String> {
        Post::update(&self.pool, id, form)
    }

    fn post_delete(&self, id: i64) -> Result<(), String> {
        Post::delete(&self.pool, id)
    }

    fn post_prev_published(&self, published_at: &NaiveDateTime) -> Option<Post> {
        Post::prev_published(&self.pool, published_at)
    }

    fn post_next_published(&self, published_at: &NaiveDateTime) -> Option<Post> {
        Post::next_published(&self.pool, published_at)
    }

    fn post_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        Post::update_status(&self.pool, id, status)
    }

    fn post_update_seo_score(&self, id: i64, score: i32, issues_json: &str) -> Result<(), String> {
        Post::update_seo_score(&self.pool, id, score, issues_json)
    }

    fn post_archives(&self) -> Vec<(String, String, i64)> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT strftime('%Y', published_at) as year, strftime('%m', published_at) as month,
                    COUNT(*) as count
             FROM posts WHERE status = 'published' AND published_at IS NOT NULL
             GROUP BY year, month ORDER BY year DESC, month DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn post_by_year_month(&self, year: &str, month: &str, limit: i64, offset: i64) -> Vec<Post> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT * FROM posts
             WHERE status = 'published'
               AND strftime('%Y', published_at) = ?1
               AND strftime('%m', published_at) = ?2
             ORDER BY published_at DESC LIMIT ?3 OFFSET ?4",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![year, month, limit, offset], |row| {
            Ok(Post {
                id: row.get("id")?,
                title: row.get("title")?,
                slug: row.get("slug")?,
                content_json: row.get("content_json")?,
                content_html: row.get("content_html")?,
                excerpt: row.get("excerpt")?,
                featured_image: row.get("featured_image")?,
                meta_title: row.get("meta_title")?,
                meta_description: row.get("meta_description")?,
                status: row.get("status")?,
                published_at: row.get("published_at")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                seo_score: row.get("seo_score").unwrap_or(-1),
                seo_issues: row.get("seo_issues").unwrap_or_else(|_| "[]".to_string()),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn post_count_by_year_month(&self, year: &str, month: &str) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM posts
             WHERE status = 'published'
               AND strftime('%Y', published_at) = ?1
               AND strftime('%m', published_at) = ?2",
            params![year, month],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    fn post_by_category(&self, category_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT p.* FROM posts p
             JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'post'
             WHERE cc.category_id = ?1 AND p.status = 'published'
             ORDER BY p.published_at DESC LIMIT ?2 OFFSET ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![category_id, limit, offset], |row| {
            Ok(Post {
                id: row.get("id")?,
                title: row.get("title")?,
                slug: row.get("slug")?,
                content_json: row.get("content_json")?,
                content_html: row.get("content_html")?,
                excerpt: row.get("excerpt")?,
                featured_image: row.get("featured_image")?,
                meta_title: row.get("meta_title")?,
                meta_description: row.get("meta_description")?,
                status: row.get("status")?,
                published_at: row.get("published_at")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                seo_score: row.get("seo_score").unwrap_or(-1),
                seo_issues: row.get("seo_issues").unwrap_or_else(|_| "[]".to_string()),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn post_count_by_category(&self, category_id: i64) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM posts p
             JOIN content_categories cc ON cc.content_id = p.id AND cc.content_type = 'post'
             WHERE cc.category_id = ?1 AND p.status = 'published'",
            params![category_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    fn post_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT p.* FROM posts p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'post'
             WHERE ct.tag_id = ?1 AND p.status = 'published'
             ORDER BY p.published_at DESC LIMIT ?2 OFFSET ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![tag_id, limit, offset], |row| {
            Ok(Post {
                id: row.get("id")?,
                title: row.get("title")?,
                slug: row.get("slug")?,
                content_json: row.get("content_json")?,
                content_html: row.get("content_html")?,
                excerpt: row.get("excerpt")?,
                featured_image: row.get("featured_image")?,
                meta_title: row.get("meta_title")?,
                meta_description: row.get("meta_description")?,
                status: row.get("status")?,
                published_at: row.get("published_at")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                seo_score: row.get("seo_score").unwrap_or(-1),
                seo_issues: row.get("seo_issues").unwrap_or_else(|_| "[]".to_string()),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn post_count_by_tag(&self, tag_id: i64) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM posts p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'post'
             WHERE ct.tag_id = ?1 AND p.status = 'published'",
            params![tag_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    // ── Portfolio ───────────────────────────────────────────────────

    fn portfolio_find_by_id(&self, id: i64) -> Option<PortfolioItem> {
        PortfolioItem::find_by_id(&self.pool, id)
    }

    fn portfolio_find_by_slug(&self, slug: &str) -> Option<PortfolioItem> {
        PortfolioItem::find_by_slug(&self.pool, slug)
    }

    fn portfolio_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<PortfolioItem> {
        PortfolioItem::list(&self.pool, status, limit, offset)
    }

    fn portfolio_count(&self, status: Option<&str>) -> i64 {
        PortfolioItem::count(&self.pool, status)
    }

    fn portfolio_by_category(
        &self,
        category_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Vec<PortfolioItem> {
        PortfolioItem::by_category(&self.pool, category_slug, limit, offset)
    }

    fn portfolio_create(&self, form: &PortfolioForm) -> Result<i64, String> {
        PortfolioItem::create(&self.pool, form)
    }

    fn portfolio_update(&self, id: i64, form: &PortfolioForm) -> Result<(), String> {
        PortfolioItem::update(&self.pool, id, form)
    }

    fn portfolio_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        PortfolioItem::update_status(&self.pool, id, status)
    }

    fn portfolio_delete(&self, id: i64) -> Result<(), String> {
        PortfolioItem::delete(&self.pool, id)
    }

    fn portfolio_increment_likes(&self, id: i64) -> Result<i64, String> {
        PortfolioItem::increment_likes(&self.pool, id)
    }

    fn portfolio_decrement_likes(&self, id: i64) -> Result<i64, String> {
        PortfolioItem::decrement_likes(&self.pool, id)
    }

    fn portfolio_update_seo_score(
        &self,
        id: i64,
        score: i32,
        issues_json: &str,
    ) -> Result<(), String> {
        PortfolioItem::update_seo_score(&self.pool, id, score, issues_json)
    }

    fn portfolio_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<PortfolioItem> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT p.* FROM portfolio p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'portfolio'
             WHERE ct.tag_id = ?1 AND p.status = 'published'
             ORDER BY p.created_at DESC LIMIT ?2 OFFSET ?3",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![tag_id, limit, offset], |row| {
            let sell_raw: i64 = row.get("sell_enabled")?;
            Ok(PortfolioItem {
                id: row.get("id")?,
                title: row.get("title")?,
                slug: row.get("slug")?,
                description_json: row.get("description_json")?,
                description_html: row.get("description_html")?,
                image_path: row.get("image_path")?,
                thumbnail_path: row.get("thumbnail_path")?,
                meta_title: row.get("meta_title")?,
                meta_description: row.get("meta_description")?,
                sell_enabled: sell_raw != 0,
                price: row.get("price")?,
                purchase_note: row
                    .get::<_, Option<String>>("purchase_note")?
                    .unwrap_or_default(),
                payment_provider: row
                    .get::<_, Option<String>>("payment_provider")?
                    .unwrap_or_default(),
                download_file_path: row
                    .get::<_, Option<String>>("download_file_path")?
                    .unwrap_or_default(),
                likes: row.get("likes")?,
                status: row.get("status")?,
                published_at: row.get("published_at")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                seo_score: row.get("seo_score").unwrap_or(-1),
                seo_issues: row.get("seo_issues").unwrap_or_else(|_| "[]".to_string()),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn portfolio_count_by_tag(&self, tag_id: i64) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM portfolio p
             JOIN content_tags ct ON ct.content_id = p.id AND ct.content_type = 'portfolio'
             WHERE ct.tag_id = ?1 AND p.status = 'published'",
            params![tag_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    // ── Comments ────────────────────────────────────────────────────

    fn comment_find_by_id(&self, id: i64) -> Option<Comment> {
        Comment::find_by_id(&self.pool, id)
    }

    fn comment_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Comment> {
        Comment::list(&self.pool, status, limit, offset)
    }

    fn comment_for_post(&self, post_id: i64, content_type: &str) -> Vec<Comment> {
        Comment::for_post(&self.pool, post_id, content_type)
    }

    fn comment_count(&self, status: Option<&str>) -> i64 {
        Comment::count(&self.pool, status)
    }

    fn comment_create(&self, form: &CommentForm) -> Result<i64, String> {
        Comment::create(&self.pool, form)
    }

    fn comment_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        Comment::update_status(&self.pool, id, status)
    }

    fn comment_delete(&self, id: i64) -> Result<(), String> {
        Comment::delete(&self.pool, id)
    }

    // ── Categories ──────────────────────────────────────────────────

    fn category_find_by_id(&self, id: i64) -> Option<Category> {
        Category::find_by_id(&self.pool, id)
    }

    fn category_find_by_slug(&self, slug: &str) -> Option<Category> {
        Category::find_by_slug(&self.pool, slug)
    }

    fn category_list(&self, type_filter: Option<&str>) -> Vec<Category> {
        Category::list(&self.pool, type_filter)
    }

    fn category_list_paginated(
        &self,
        type_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<Category> {
        Category::list_paginated(&self.pool, type_filter, limit, offset)
    }

    fn category_count(&self, type_filter: Option<&str>) -> i64 {
        Category::count(&self.pool, type_filter)
    }

    fn category_for_content(&self, content_id: i64, content_type: &str) -> Vec<Category> {
        Category::for_content(&self.pool, content_id, content_type)
    }

    fn category_count_items(&self, category_id: i64) -> i64 {
        Category::count_items(&self.pool, category_id)
    }

    fn category_create(&self, form: &CategoryForm) -> Result<i64, String> {
        Category::create(&self.pool, form)
    }

    fn category_update(&self, id: i64, form: &CategoryForm) -> Result<(), String> {
        Category::update(&self.pool, id, form)
    }

    fn category_delete(&self, id: i64) -> Result<(), String> {
        Category::delete(&self.pool, id)
    }

    fn category_set_show_in_nav(&self, id: i64, show: bool) -> Result<(), String> {
        Category::set_show_in_nav(&self.pool, id, show)
    }

    fn category_list_nav_visible(&self, type_filter: Option<&str>) -> Vec<Category> {
        Category::list_nav_visible(&self.pool, type_filter)
    }

    fn category_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        category_ids: &[i64],
    ) -> Result<(), String> {
        Category::set_for_content(&self.pool, content_id, content_type, category_ids)
    }

    // ── Tags ────────────────────────────────────────────────────────

    fn tag_find_by_id(&self, id: i64) -> Option<Tag> {
        Tag::find_by_id(&self.pool, id)
    }

    fn tag_find_by_slug(&self, slug: &str) -> Option<Tag> {
        Tag::find_by_slug(&self.pool, slug)
    }

    fn tag_list(&self) -> Vec<Tag> {
        Tag::list(&self.pool)
    }

    fn tag_list_paginated(&self, limit: i64, offset: i64) -> Vec<Tag> {
        Tag::list_paginated(&self.pool, limit, offset)
    }

    fn tag_count(&self) -> i64 {
        Tag::count(&self.pool)
    }

    fn tag_for_content(&self, content_id: i64, content_type: &str) -> Vec<Tag> {
        Tag::for_content(&self.pool, content_id, content_type)
    }

    fn tag_count_items(&self, tag_id: i64) -> i64 {
        Tag::count_items(&self.pool, tag_id)
    }

    fn tag_create(&self, form: &TagForm) -> Result<i64, String> {
        Tag::create(&self.pool, form)
    }

    fn tag_update(&self, id: i64, form: &TagForm) -> Result<(), String> {
        Tag::update(&self.pool, id, form)
    }

    fn tag_delete(&self, id: i64) -> Result<(), String> {
        Tag::delete(&self.pool, id)
    }

    fn tag_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        tag_ids: &[i64],
    ) -> Result<(), String> {
        Tag::set_for_content(&self.pool, content_id, content_type, tag_ids)
    }

    fn tag_find_or_create(&self, name: &str) -> Result<i64, String> {
        Tag::find_or_create(&self.pool, name)
    }

    // ── Designs ─────────────────────────────────────────────────────

    fn design_find_by_id(&self, id: i64) -> Option<Design> {
        Design::find_by_id(&self.pool, id)
    }

    fn design_find_by_slug(&self, slug: &str) -> Option<Design> {
        Design::find_by_slug(&self.pool, slug)
    }

    fn design_active(&self) -> Option<Design> {
        Design::active(&self.pool)
    }

    fn design_list(&self) -> Vec<Design> {
        Design::list(&self.pool)
    }

    fn design_activate(&self, id: i64) -> Result<(), String> {
        Design::activate(&self.pool, id)
    }

    fn design_create(&self, name: &str) -> Result<i64, String> {
        Design::create(&self.pool, name)
    }

    fn design_duplicate(&self, id: i64, new_name: &str) -> Result<i64, String> {
        Design::duplicate(&self.pool, id, new_name)
    }

    fn design_delete(&self, id: i64) -> Result<(), String> {
        Design::delete(&self.pool, id)
    }

    // ── Design Templates ────────────────────────────────────────────

    fn design_template_for_design(&self, design_id: i64) -> Vec<DesignTemplate> {
        DesignTemplate::for_design(&self.pool, design_id)
    }

    fn design_template_get(&self, design_id: i64, template_type: &str) -> Option<DesignTemplate> {
        DesignTemplate::get(&self.pool, design_id, template_type)
    }

    fn design_template_upsert(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
    ) -> Result<(), String> {
        DesignTemplate::upsert(&self.pool, design_id, template_type, layout_html, style_css)
    }

    fn design_template_upsert_full(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
        grapesjs_data: &str,
    ) -> Result<(), String> {
        DesignTemplate::upsert_full(
            &self.pool,
            design_id,
            template_type,
            layout_html,
            style_css,
            grapesjs_data,
        )
    }

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
    ) {
        AuditEntry::log(
            &self.pool,
            user_id,
            user_name,
            action,
            entity_type,
            entity_id,
            entity_title,
            details,
            ip_address,
        );
    }

    fn audit_list(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Vec<AuditEntry> {
        AuditEntry::list(
            &self.pool,
            action_filter,
            entity_filter,
            user_filter,
            limit,
            offset,
        )
    }

    fn audit_count(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
    ) -> i64 {
        AuditEntry::count(&self.pool, action_filter, entity_filter, user_filter)
    }

    fn audit_distinct_actions(&self) -> Vec<String> {
        AuditEntry::distinct_actions(&self.pool)
    }

    fn audit_distinct_entity_types(&self) -> Vec<String> {
        AuditEntry::distinct_entity_types(&self.pool)
    }

    fn audit_cleanup(&self, max_age_days: i64) -> Result<usize, String> {
        AuditEntry::cleanup(&self.pool, max_age_days)
    }

    // ── Firewall: Bans ──────────────────────────────────────────────

    fn fw_is_banned(&self, ip: &str) -> bool {
        FwBan::is_banned(&self.pool, ip)
    }

    fn fw_ban_create(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        expires_at: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        FwBan::create(
            &self.pool, ip, reason, detail, expires_at, country, user_agent,
        )
    }

    fn fw_ban_create_with_duration(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        duration: &str,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        FwBan::create_with_duration(
            &self.pool, ip, reason, detail, duration, country, user_agent,
        )
    }

    fn fw_unban(&self, ip: &str) -> Result<usize, String> {
        FwBan::unban(&self.pool, ip)
    }

    fn fw_unban_by_id(&self, id: i64) -> Result<usize, String> {
        FwBan::unban_by_id(&self.pool, id)
    }

    fn fw_active_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        FwBan::active_bans(&self.pool, limit, offset)
    }

    fn fw_active_count(&self) -> i64 {
        FwBan::active_count(&self.pool)
    }

    fn fw_all_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        FwBan::all_bans(&self.pool, limit, offset)
    }

    fn fw_expire_stale(&self) {
        FwBan::expire_stale(&self.pool);
    }

    // ── Firewall: Events ────────────────────────────────────────────

    fn fw_event_log(
        &self,
        ip: &str,
        event_type: &str,
        detail: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
        request_path: Option<&str>,
    ) {
        FwEvent::log(
            &self.pool,
            ip,
            event_type,
            detail,
            country,
            user_agent,
            request_path,
        );
    }

    fn fw_event_recent(&self, event_type: Option<&str>, limit: i64, offset: i64) -> Vec<FwEvent> {
        FwEvent::recent(&self.pool, event_type, limit, offset)
    }

    fn fw_event_count_all(&self, event_type: Option<&str>) -> i64 {
        FwEvent::count_all(&self.pool, event_type)
    }

    fn fw_event_count_since_hours(&self, hours: i64) -> i64 {
        FwEvent::count_since_hours(&self.pool, hours)
    }

    fn fw_event_count_for_ip_since(&self, ip: &str, event_type: &str, minutes: i64) -> i64 {
        FwEvent::count_for_ip_since(&self.pool, ip, event_type, minutes)
    }

    fn fw_event_top_ips(&self, limit: i64) -> Vec<(String, i64)> {
        FwEvent::top_ips(&self.pool, limit)
    }

    fn fw_event_counts_by_type(&self) -> Vec<(String, i64)> {
        FwEvent::counts_by_type(&self.pool)
    }

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
    ) -> Result<(), String> {
        crate::models::analytics::PageView::record(
            &self.pool,
            path,
            ip_hash,
            country,
            city,
            referrer,
            user_agent,
            device_type,
            browser,
        )
    }

    fn analytics_overview(&self, from: &str, to: &str) -> OverviewStats {
        crate::models::analytics::PageView::overview(&self.pool, from, to)
    }

    fn analytics_flow_data(&self, from: &str, to: &str) -> Vec<FlowNode> {
        crate::models::analytics::PageView::flow_data(&self.pool, from, to)
    }

    fn analytics_geo_data(&self, from: &str, to: &str) -> Vec<CountEntry> {
        crate::models::analytics::PageView::geo_data(&self.pool, from, to)
    }

    fn analytics_stream_data(&self, from: &str, to: &str) -> Vec<StreamEntry> {
        crate::models::analytics::PageView::stream_data(&self.pool, from, to)
    }

    fn analytics_calendar_data(&self, from: &str, to: &str) -> Vec<DailyCount> {
        crate::models::analytics::PageView::calendar_data(&self.pool, from, to)
    }

    fn analytics_top_portfolio(&self, from: &str, to: &str, limit: i64) -> Vec<CountEntry> {
        crate::models::analytics::PageView::top_portfolio(&self.pool, from, to, limit)
    }

    fn analytics_top_referrers(&self, from: &str, to: &str, limit: i64) -> Vec<CountEntry> {
        crate::models::analytics::PageView::top_referrers(&self.pool, from, to, limit)
    }

    fn analytics_tag_relations(&self) -> Vec<TagRelation> {
        crate::models::analytics::PageView::tag_relations(&self.pool)
    }

    // ── Orders ──────────────────────────────────────────────────────

    fn order_find_by_id(&self, id: i64) -> Option<Order> {
        Order::find_by_id(&self.pool, id)
    }

    fn order_find_by_provider_order_id(&self, provider_order_id: &str) -> Option<Order> {
        Order::find_by_provider_order_id(&self.pool, provider_order_id)
    }

    fn order_list(&self, limit: i64, offset: i64) -> Vec<Order> {
        Order::list(&self.pool, limit, offset)
    }

    fn order_list_by_status(&self, status: &str, limit: i64, offset: i64) -> Vec<Order> {
        Order::list_by_status(&self.pool, status, limit, offset)
    }

    fn order_list_by_email(&self, email: &str, limit: i64, offset: i64) -> Vec<Order> {
        Order::list_by_email(&self.pool, email, limit, offset)
    }

    fn order_list_by_portfolio(&self, portfolio_id: i64) -> Vec<Order> {
        Order::list_by_portfolio(&self.pool, portfolio_id)
    }

    fn order_count(&self) -> i64 {
        Order::count(&self.pool)
    }

    fn order_count_by_status(&self, status: &str) -> i64 {
        Order::count_by_status(&self.pool, status)
    }

    fn order_total_revenue(&self) -> f64 {
        Order::total_revenue(&self.pool)
    }

    fn order_revenue_by_period(&self, days: i64) -> f64 {
        Order::revenue_by_period(&self.pool, days)
    }

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
    ) -> Result<i64, String> {
        Order::create(
            &self.pool,
            portfolio_id,
            buyer_email,
            buyer_name,
            amount,
            currency,
            provider,
            provider_order_id,
            status,
        )
    }

    fn order_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        Order::update_status(&self.pool, id, status)
    }

    fn order_update_provider_order_id(
        &self,
        id: i64,
        provider_order_id: &str,
    ) -> Result<(), String> {
        Order::update_provider_order_id(&self.pool, id, provider_order_id)
    }

    fn order_update_buyer_info(
        &self,
        id: i64,
        buyer_email: &str,
        buyer_name: &str,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE orders SET buyer_email = ?1, buyer_name = ?2 WHERE id = ?3",
            rusqlite::params![buyer_email, buyer_name, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn order_find_completed_by_email_and_portfolio(
        &self,
        email: &str,
        portfolio_id: i64,
    ) -> Option<Order> {
        let conn = self.pool.get().ok()?;
        let mut stmt = conn
            .prepare("SELECT id FROM orders WHERE portfolio_id = ?1 AND buyer_email = ?2 AND status = 'completed' ORDER BY created_at DESC LIMIT 1")
            .ok()?;
        let id: i64 = stmt
            .query_row(rusqlite::params![portfolio_id, email], |row| row.get(0))
            .ok()?;
        Order::find_by_id(&self.pool, id)
    }

    // ── Download Tokens ─────────────────────────────────────────────

    fn download_token_find_by_token(&self, token: &str) -> Option<DownloadToken> {
        DownloadToken::find_by_token(&self.pool, token)
    }

    fn download_token_find_by_order(&self, order_id: i64) -> Option<DownloadToken> {
        DownloadToken::find_by_order(&self.pool, order_id)
    }

    fn download_token_increment(&self, id: i64) -> Result<(), String> {
        DownloadToken::increment_download(&self.pool, id)
    }

    fn download_token_create(
        &self,
        order_id: i64,
        token: &str,
        max_downloads: i64,
        expires_at: NaiveDateTime,
    ) -> Result<i64, String> {
        DownloadToken::create(&self.pool, order_id, token, max_downloads, expires_at)
    }

    // ── Licenses ────────────────────────────────────────────────────

    fn license_find_by_order(&self, order_id: i64) -> Option<License> {
        License::find_by_order(&self.pool, order_id)
    }

    fn license_find_by_key(&self, key: &str) -> Option<License> {
        License::find_by_key(&self.pool, key)
    }

    fn license_create(&self, order_id: i64, license_key: &str) -> Result<i64, String> {
        License::create(&self.pool, order_id, license_key)
    }

    // ── Passkeys ────────────────────────────────────────────────────

    fn passkey_list_for_user(&self, user_id: i64) -> Vec<UserPasskey> {
        UserPasskey::list_for_user(&self.pool, user_id)
    }

    fn passkey_get_by_credential_id(&self, credential_id: &str) -> Option<UserPasskey> {
        UserPasskey::get_by_credential_id(&self.pool, credential_id)
    }

    fn passkey_count_for_user(&self, user_id: i64) -> i64 {
        UserPasskey::count_for_user(&self.pool, user_id)
    }

    fn passkey_create(
        &self,
        user_id: i64,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
        transports: &str,
        name: &str,
    ) -> Result<i64, String> {
        UserPasskey::create(
            &self.pool,
            user_id,
            credential_id,
            public_key,
            sign_count,
            transports,
            name,
        )
    }

    fn passkey_update_sign_count(
        &self,
        credential_id: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        UserPasskey::update_sign_count(&self.pool, credential_id, sign_count)
    }

    fn passkey_update_public_key(
        &self,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE user_passkeys SET public_key = ?1, sign_count = ?2 WHERE credential_id = ?3",
            rusqlite::params![public_key, sign_count, credential_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn passkey_delete(&self, id: i64, user_id: i64) -> Result<(), String> {
        UserPasskey::delete(&self.pool, id, user_id)
    }

    fn passkey_delete_all_for_user(&self, user_id: i64) -> Result<(), String> {
        UserPasskey::delete_all_for_user(&self.pool, user_id)
    }

    // ── Imports ─────────────────────────────────────────────────────

    fn import_list(&self) -> Vec<Import> {
        Import::list(&self.pool)
    }

    fn import_create(
        &self,
        source: &str,
        filename: Option<&str>,
        posts_count: i64,
        portfolio_count: i64,
        comments_count: i64,
        skipped_count: i64,
        log: Option<&str>,
    ) -> Result<i64, String> {
        Import::create(
            &self.pool,
            source,
            filename,
            posts_count,
            portfolio_count,
            comments_count,
            skipped_count,
            log,
        )
    }

    // ── Search (FTS) ────────────────────────────────────────────────

    fn search_create_fts_table(&self) -> Result<(), String> {
        crate::models::search::create_fts_table(&self.pool)
    }

    fn search_rebuild_index(&self) -> Result<usize, String> {
        crate::models::search::rebuild_index(&self.pool)
    }

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
    ) {
        crate::models::search::upsert_item(
            &self.pool,
            item_type,
            item_id,
            title,
            html_body,
            slug,
            image,
            date,
            is_published,
        );
    }

    fn search_remove_item(&self, item_type: &str, item_id: i64) {
        crate::models::search::remove_item(&self.pool, item_type, item_id);
    }

    fn search_query(&self, query: &str, limit: i64) -> Vec<SearchResult> {
        crate::models::search::search(&self.pool, query, limit)
    }

    // ── Sessions ────────────────────────────────────────────────────

    fn session_create(&self, user_id: i64, token: &str, expires_at: &str) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES (?1, ?2, datetime('now'), ?3)",
            params![token, user_id, expires_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_get_user_id(&self, token: &str) -> Option<i64> {
        let conn = self.pool.get().ok()?;
        conn.query_row(
            "SELECT user_id FROM sessions WHERE id = ?1 AND expires_at > datetime('now')",
            params![token],
            |row| row.get(0),
        )
        .ok()
    }

    fn session_delete(&self, token: &str) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![token])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_delete_for_user(&self, user_id: i64) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_create_full(
        &self,
        user_id: i64,
        token: &str,
        expires_at: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO sessions (id, user_id, created_at, expires_at, ip_address, user_agent)
             VALUES (?1, ?2, datetime('now'), ?3, ?4, ?5)",
            params![token, user_id, expires_at, ip, user_agent],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_cleanup_expired(&self) {
        if let Ok(conn) = self.pool.get() {
            let _ = conn.execute(
                "DELETE FROM sessions WHERE expires_at <= datetime('now')",
                [],
            );
        }
    }

    fn session_count_recent_by_ip(&self, ip_hash: &str, minutes: i64) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM sessions WHERE ip_address = ?1 AND created_at > datetime('now', '-{} minutes')",
                minutes
            ),
            params![ip_hash],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    // ── Magic links / password reset tokens ─────────────────────────

    fn magic_link_create(
        &self,
        token: &str,
        email: &str,
        expires_minutes: i64,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().naive_utc();
        let expires = now + chrono::Duration::minutes(expires_minutes);
        conn.execute(
            "INSERT INTO magic_links (token, email, created_at, expires_at, used) VALUES (?1, ?2, ?3, ?4, 0)",
            params![token, email, now, expires],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn magic_link_verify(&self, token: &str) -> Result<String, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().naive_utc();
        let result: Result<(String, bool), _> = conn.query_row(
            "SELECT email, used FROM magic_links WHERE token = ?1 AND expires_at > ?2",
            params![token, now],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok((email, used)) => {
                if used {
                    return Err("This link has already been used".into());
                }
                conn.execute(
                    "UPDATE magic_links SET used = 1 WHERE token = ?1",
                    params![token],
                )
                .map_err(|e| e.to_string())?;
                Ok(email)
            }
            Err(_) => Err("Invalid or expired link".into()),
        }
    }

    // ── Likes ───────────────────────────────────────────────────────

    fn like_exists(&self, portfolio_id: i64, ip_hash: &str) -> bool {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM likes WHERE portfolio_id = ?1 AND ip_hash = ?2",
            params![portfolio_id, ip_hash],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    fn like_add(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO likes (portfolio_id, ip_hash) VALUES (?1, ?2)",
            params![portfolio_id, ip_hash],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn like_remove(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM likes WHERE portfolio_id = ?1 AND ip_hash = ?2",
            params![portfolio_id, ip_hash],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Analytics pruning ───────────────────────────────────────────

    fn analytics_prune(&self, before_date: &str) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM page_views WHERE created_at < ?1",
            params![before_date],
        )
        .map_err(|e| e.to_string())
    }

    fn analytics_count(&self) -> i64 {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM page_views", [], |row| row.get(0))
            .unwrap_or(0)
    }

    // ── Health / maintenance ───────────────────────────────────────────

    fn db_backend(&self) -> &str {
        "sqlite"
    }

    fn health_content_stats(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        };
        let count = |sql: &str| -> u64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
        (
            count("SELECT COUNT(*) FROM posts"),
            count("SELECT COUNT(*) FROM posts WHERE status = 'published'"),
            count("SELECT COUNT(*) FROM posts WHERE status = 'draft'"),
            count("SELECT COUNT(*) FROM portfolio"),
            count("SELECT COUNT(*) FROM comments"),
            count("SELECT COUNT(*) FROM comments WHERE status = 'pending'"),
            count("SELECT COUNT(*) FROM categories"),
            count("SELECT COUNT(*) FROM tags"),
            count("SELECT COUNT(*) FROM sessions"),
            count("SELECT COUNT(*) FROM sessions WHERE expires_at < datetime('now')"),
        )
    }

    fn health_referenced_files(&self) -> std::collections::HashSet<String> {
        let mut referenced = std::collections::HashSet::new();
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return referenced,
        };

        // Post featured images
        if let Ok(mut stmt) = conn.prepare("SELECT featured_image FROM posts WHERE featured_image IS NOT NULL AND featured_image != ''") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for row in rows.flatten() {
                    if let Some(name) = row.split('/').next_back() {
                        referenced.insert(name.to_string());
                    }
                }
            }
        }
        // Portfolio images
        if let Ok(mut stmt) = conn.prepare(
            "SELECT image_path FROM portfolio WHERE image_path IS NOT NULL AND image_path != ''",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for row in rows.flatten() {
                    if let Some(name) = row.split('/').next_back() {
                        referenced.insert(name.to_string());
                    }
                }
            }
        }
        // Images in post body
        if let Ok(mut stmt) =
            conn.prepare("SELECT content_html FROM posts WHERE content_html IS NOT NULL")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for body in rows.flatten() {
                    crate::health::extract_upload_refs(&body, &mut referenced);
                }
            }
        }
        // Images in portfolio description
        if let Ok(mut stmt) = conn
            .prepare("SELECT description_html FROM portfolio WHERE description_html IS NOT NULL")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for body in rows.flatten() {
                    crate::health::extract_upload_refs(&body, &mut referenced);
                }
            }
        }
        // Settings referencing uploads
        if let Ok(mut stmt) =
            conn.prepare("SELECT value FROM settings WHERE value LIKE '%/uploads/%'")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for val in rows.flatten() {
                    if let Some(name) = val.split('/').next_back() {
                        referenced.insert(name.to_string());
                    }
                }
            }
        }
        referenced
    }

    fn health_session_cleanup(&self) -> Result<u64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let expired: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE expires_at < datetime('now')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "DELETE FROM sessions WHERE expires_at < datetime('now')",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(expired)
    }

    fn health_unused_tags_cleanup(&self) -> Result<u64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE id NOT IN (SELECT tag_id FROM post_tags) AND id NOT IN (SELECT tag_id FROM portfolio_tags)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count == 0 {
            return Ok(0);
        }
        conn.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM post_tags) AND id NOT IN (SELECT tag_id FROM portfolio_tags)",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn health_analytics_prune(&self, days: u64) -> Result<u64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let count: u64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM analytics_events WHERE created_at < datetime('now', '-{} days')",
                    days
                ),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            &format!(
                "DELETE FROM analytics_events WHERE created_at < datetime('now', '-{} days')",
                days
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn health_export_content(&self) -> Result<serde_json::Value, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut export = serde_json::Map::new();

        // Posts
        if let Ok(mut stmt) = conn.prepare("SELECT id, title, slug, content_html, excerpt, featured_image, status, created_at, updated_at FROM posts ORDER BY id") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "title": r.get::<_, String>(1).unwrap_or_default(),
                        "slug": r.get::<_, String>(2).unwrap_or_default(),
                        "body": r.get::<_, String>(3).unwrap_or_default(),
                        "excerpt": r.get::<_, String>(4).unwrap_or_default(),
                        "featured_image": r.get::<_, String>(5).unwrap_or_default(),
                        "status": r.get::<_, String>(6).unwrap_or_default(),
                        "created_at": r.get::<_, String>(7).unwrap_or_default(),
                        "updated_at": r.get::<_, String>(8).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("posts".to_string(), serde_json::Value::Array(rows));
        }

        // Portfolio items
        if let Ok(mut stmt) = conn.prepare("SELECT id, title, slug, description_html, image_path, status, created_at, updated_at FROM portfolio ORDER BY id") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "title": r.get::<_, String>(1).unwrap_or_default(),
                        "slug": r.get::<_, String>(2).unwrap_or_default(),
                        "description": r.get::<_, String>(3).unwrap_or_default(),
                        "image_path": r.get::<_, String>(4).unwrap_or_default(),
                        "status": r.get::<_, String>(5).unwrap_or_default(),
                        "created_at": r.get::<_, String>(6).unwrap_or_default(),
                        "updated_at": r.get::<_, String>(7).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("portfolio".to_string(), serde_json::Value::Array(rows));
        }

        // Categories
        if let Ok(mut stmt) = conn.prepare("SELECT id, name, slug FROM categories ORDER BY id") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "name": r.get::<_, String>(1).unwrap_or_default(),
                        "slug": r.get::<_, String>(2).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("categories".to_string(), serde_json::Value::Array(rows));
        }

        // Tags
        if let Ok(mut stmt) = conn.prepare("SELECT id, name, slug FROM tags ORDER BY id") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "name": r.get::<_, String>(1).unwrap_or_default(),
                        "slug": r.get::<_, String>(2).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("tags".to_string(), serde_json::Value::Array(rows));
        }

        // Comments
        if let Ok(mut stmt) = conn.prepare("SELECT id, post_id, author_name, author_email, body, status, created_at FROM comments ORDER BY id") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "post_id": r.get::<_, i64>(1)?,
                        "author_name": r.get::<_, String>(2).unwrap_or_default(),
                        "author_email": r.get::<_, String>(3).unwrap_or_default(),
                        "body": r.get::<_, String>(4).unwrap_or_default(),
                        "status": r.get::<_, String>(5).unwrap_or_default(),
                        "created_at": r.get::<_, String>(6).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("comments".to_string(), serde_json::Value::Array(rows));
        }

        // Relationships (content_tags and content_categories with content_type filter)
        for (export_key, table, content_type) in &[
            ("post_tags", "content_tags", "post"),
            ("post_categories", "content_categories", "post"),
            ("portfolio_tags", "content_tags", "portfolio"),
            ("portfolio_categories", "content_categories", "portfolio"),
        ] {
            let sql = format!(
                "SELECT content_id, {} FROM {} WHERE content_type = ?1",
                if *table == "content_tags" {
                    "tag_id"
                } else {
                    "category_id"
                },
                table
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let id2_col = if *table == "content_tags" {
                    "tag_id"
                } else {
                    "category_id"
                };
                let rows: Vec<serde_json::Value> = stmt
                    .query_map(rusqlite::params![content_type], |r| {
                        Ok(serde_json::json!({
                            "content_id": r.get::<_, i64>(0)?,
                            id2_col: r.get::<_, i64>(1)?,
                        }))
                    })
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default();
                export.insert(export_key.to_string(), serde_json::Value::Array(rows));
            }
        }

        // Settings
        if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM settings ORDER BY key") {
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({
                        "key": r.get::<_, String>(0)?,
                        "value": r.get::<_, String>(1).unwrap_or_default(),
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
            export.insert("settings".to_string(), serde_json::Value::Array(rows));
        }

        Ok(serde_json::Value::Object(export))
    }

    // ── Background tasks ──────────────────────────────────────────────

    fn magic_link_cleanup(&self) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM magic_links WHERE expires_at < datetime('now') OR used = 1",
            [],
        )
        .map_err(|e| e.to_string())
    }

    fn task_publish_scheduled(&self) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let posts = conn
            .execute(
                "UPDATE posts SET status = 'published', updated_at = CURRENT_TIMESTAMP WHERE status = 'scheduled' AND published_at <= datetime('now')",
                [],
            )
            .map_err(|e| e.to_string())?;
        let portfolio = conn
            .execute(
                "UPDATE portfolio SET status = 'published', updated_at = CURRENT_TIMESTAMP WHERE status = 'scheduled' AND published_at <= datetime('now')",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(posts + portfolio)
    }

    fn task_cleanup_sessions(&self, max_age_days: i64) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM sessions WHERE expires_at < datetime('now') OR created_at < datetime('now', ?1)",
            rusqlite::params![format!("-{} days", max_age_days)],
        )
        .map_err(|e| e.to_string())
    }

    fn task_cleanup_analytics(&self, max_age_days: i64) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM page_views WHERE created_at < datetime('now', ?1)",
            rusqlite::params![format!("-{} days", max_age_days)],
        )
        .map_err(|e| e.to_string())
    }

    // ── Email queue (built-in MTA) ──────────────────────────────────

    fn mta_queue_push(
        &self,
        to: &str,
        from: &str,
        subject: &str,
        body: &str,
    ) -> Result<i64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO email_queue (to_addr, from_addr, subject, body_text) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![to, from, subject, body],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    fn mta_queue_pending(&self, limit: i64) -> Vec<crate::mta::queue::QueuedEmail> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT id, to_addr, from_addr, subject, body_text, attempts, max_attempts, next_retry_at, status, error, created_at \
             FROM email_queue WHERE status = 'pending' AND next_retry_at <= datetime('now') \
             ORDER BY next_retry_at ASC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(rusqlite::params![limit], |r| {
            Ok(crate::mta::queue::QueuedEmail {
                id: r.get(0)?,
                to_addr: r.get(1)?,
                from_addr: r.get(2)?,
                subject: r.get(3)?,
                body_text: r.get(4)?,
                attempts: r.get(5)?,
                max_attempts: r.get(6)?,
                next_retry_at: r.get(7)?,
                status: r.get(8)?,
                error: r.get::<_, String>(9).unwrap_or_default(),
                created_at: r.get(10)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    fn mta_queue_update_status(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
        next_retry: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE email_queue SET status = ?1, error = COALESCE(?2, error), \
             next_retry_at = COALESCE(?3, next_retry_at), \
             attempts = CASE WHEN ?1 = 'sending' THEN attempts + 1 ELSE attempts END \
             WHERE id = ?4",
            rusqlite::params![status, error, next_retry, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn mta_queue_sent_last_hour(&self) -> Result<u64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT COUNT(*) FROM email_queue WHERE status = 'sent' AND created_at >= datetime('now', '-1 hour')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    }

    fn mta_queue_stats(&self) -> (u64, u64, u64, u64) {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return (0, 0, 0, 0),
        };
        conn.query_row(
            "SELECT \
                COALESCE(SUM(CASE WHEN status='sent' THEN 1 ELSE 0 END),0), \
                COALESCE(SUM(CASE WHEN status='pending' OR status='sending' THEN 1 ELSE 0 END),0), \
                COALESCE(SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END),0), \
                COUNT(*) \
             FROM email_queue",
            [],
            |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, u64>(1)?,
                    r.get::<_, u64>(2)?,
                    r.get::<_, u64>(3)?,
                ))
            },
        )
        .unwrap_or((0, 0, 0, 0))
    }

    fn mta_queue_cleanup(&self, days: u64) -> Result<u64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let count = conn
            .execute(
                "DELETE FROM email_queue WHERE created_at < datetime('now', ?1)",
                rusqlite::params![format!("-{} days", days)],
            )
            .map_err(|e| e.to_string())?;
        Ok(count as u64)
    }

    // ── Raw execute ─────────────────────────────────────────────────

    fn raw_execute(&self, sql: &str) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(sql, []).map_err(|e| e.to_string())
    }

    fn raw_query_i64(&self, sql: &str) -> Result<i64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(sql, [], |row| row.get(0))
            .map_err(|e| e.to_string())
    }
}

// ── Bridge: implement Store for DbPool directly ─────────────────────
// This allows existing routes that still use `pool: &State<DbPool>` to pass
// `pool.inner()` as `&dyn Store` to rewired helpers during the gradual migration.

impl Store for DbPool {
    fn run_migrations(&self) -> Result<(), String> {
        SqliteStore::new(self.clone()).run_migrations()
    }
    fn seed_defaults(&self) -> Result<(), String> {
        SqliteStore::new(self.clone()).seed_defaults()
    }
    fn setting_get(&self, key: &str) -> Option<String> {
        SqliteStore::new(self.clone()).setting_get(key)
    }
    fn setting_set(&self, key: &str, value: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).setting_set(key, value)
    }
    fn setting_set_many(
        &self,
        settings: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).setting_set_many(settings)
    }
    fn setting_get_group(&self, prefix: &str) -> std::collections::HashMap<String, String> {
        SqliteStore::new(self.clone()).setting_get_group(prefix)
    }
    fn setting_all(&self) -> std::collections::HashMap<String, String> {
        SqliteStore::new(self.clone()).setting_all()
    }
    fn setting_delete(&self, key: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).setting_delete(key)
    }
    fn user_get_by_id(&self, id: i64) -> Option<crate::models::user::User> {
        SqliteStore::new(self.clone()).user_get_by_id(id)
    }
    fn user_get_by_email(&self, email: &str) -> Option<crate::models::user::User> {
        SqliteStore::new(self.clone()).user_get_by_email(email)
    }
    fn user_list_all(&self) -> Vec<crate::models::user::User> {
        SqliteStore::new(self.clone()).user_list_all()
    }
    fn user_list_paginated(
        &self,
        role: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<crate::models::user::User> {
        SqliteStore::new(self.clone()).user_list_paginated(role, limit, offset)
    }
    fn user_count(&self) -> i64 {
        SqliteStore::new(self.clone()).user_count()
    }
    fn user_count_filtered(&self, role: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).user_count_filtered(role)
    }
    fn user_count_by_role(&self, role: &str) -> i64 {
        SqliteStore::new(self.clone()).user_count_by_role(role)
    }
    fn user_create(
        &self,
        email: &str,
        password_hash: &str,
        display_name: &str,
        role: &str,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).user_create(email, password_hash, display_name, role)
    }
    fn user_update_profile(
        &self,
        id: i64,
        display_name: &str,
        email: &str,
        avatar: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_profile(id, display_name, email, avatar)
    }
    fn user_update_role(&self, id: i64, role: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_role(id, role)
    }
    fn user_update_password(&self, id: i64, password_hash: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_password(id, password_hash)
    }
    fn user_update_avatar(&self, id: i64, avatar: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_avatar(id, avatar)
    }
    fn user_touch_last_login(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_touch_last_login(id)
    }
    fn user_update_mfa(
        &self,
        id: i64,
        enabled: bool,
        secret: &str,
        recovery_codes: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_mfa(id, enabled, secret, recovery_codes)
    }
    fn user_lock(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_lock(id)
    }
    fn user_unlock(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_unlock(id)
    }
    fn user_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_delete(id)
    }
    fn user_update_auth_method(&self, id: i64, method: &str, fallback: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_update_auth_method(id, method, fallback)
    }
    fn user_set_force_password_change(&self, id: i64, force: bool) -> Result<(), String> {
        SqliteStore::new(self.clone()).user_set_force_password_change(id, force)
    }
    fn post_find_by_id(&self, id: i64) -> Option<crate::models::post::Post> {
        SqliteStore::new(self.clone()).post_find_by_id(id)
    }
    fn post_find_by_slug(&self, slug: &str) -> Option<crate::models::post::Post> {
        SqliteStore::new(self.clone()).post_find_by_slug(slug)
    }
    fn post_list(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<crate::models::post::Post> {
        SqliteStore::new(self.clone()).post_list(status, limit, offset)
    }
    fn post_count(&self, status: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).post_count(status)
    }
    fn post_create(&self, form: &crate::models::post::PostForm) -> Result<i64, String> {
        SqliteStore::new(self.clone()).post_create(form)
    }
    fn post_update(&self, id: i64, form: &crate::models::post::PostForm) -> Result<(), String> {
        SqliteStore::new(self.clone()).post_update(id, form)
    }
    fn post_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).post_delete(id)
    }
    fn post_prev_published(
        &self,
        published_at: &NaiveDateTime,
    ) -> Option<crate::models::post::Post> {
        SqliteStore::new(self.clone()).post_prev_published(published_at)
    }
    fn post_next_published(
        &self,
        published_at: &NaiveDateTime,
    ) -> Option<crate::models::post::Post> {
        SqliteStore::new(self.clone()).post_next_published(published_at)
    }
    fn post_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).post_update_status(id, status)
    }
    fn post_update_seo_score(&self, id: i64, score: i32, issues_json: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).post_update_seo_score(id, score, issues_json)
    }
    fn post_archives(&self) -> Vec<(String, String, i64)> {
        SqliteStore::new(self.clone()).post_archives()
    }
    fn post_by_year_month(&self, year: &str, month: &str, limit: i64, offset: i64) -> Vec<Post> {
        SqliteStore::new(self.clone()).post_by_year_month(year, month, limit, offset)
    }
    fn post_count_by_year_month(&self, year: &str, month: &str) -> i64 {
        SqliteStore::new(self.clone()).post_count_by_year_month(year, month)
    }
    fn post_by_category(&self, category_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        SqliteStore::new(self.clone()).post_by_category(category_id, limit, offset)
    }
    fn post_count_by_category(&self, category_id: i64) -> i64 {
        SqliteStore::new(self.clone()).post_count_by_category(category_id)
    }
    fn post_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        SqliteStore::new(self.clone()).post_by_tag(tag_id, limit, offset)
    }
    fn post_count_by_tag(&self, tag_id: i64) -> i64 {
        SqliteStore::new(self.clone()).post_count_by_tag(tag_id)
    }
    fn portfolio_find_by_id(&self, id: i64) -> Option<crate::models::portfolio::PortfolioItem> {
        SqliteStore::new(self.clone()).portfolio_find_by_id(id)
    }
    fn portfolio_find_by_slug(
        &self,
        slug: &str,
    ) -> Option<crate::models::portfolio::PortfolioItem> {
        SqliteStore::new(self.clone()).portfolio_find_by_slug(slug)
    }
    fn portfolio_list(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<crate::models::portfolio::PortfolioItem> {
        SqliteStore::new(self.clone()).portfolio_list(status, limit, offset)
    }
    fn portfolio_count(&self, status: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).portfolio_count(status)
    }
    fn portfolio_by_category(
        &self,
        category_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Vec<crate::models::portfolio::PortfolioItem> {
        SqliteStore::new(self.clone()).portfolio_by_category(category_slug, limit, offset)
    }
    fn portfolio_create(
        &self,
        form: &crate::models::portfolio::PortfolioForm,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).portfolio_create(form)
    }
    fn portfolio_update(
        &self,
        id: i64,
        form: &crate::models::portfolio::PortfolioForm,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).portfolio_update(id, form)
    }
    fn portfolio_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).portfolio_update_status(id, status)
    }
    fn portfolio_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).portfolio_delete(id)
    }
    fn portfolio_increment_likes(&self, id: i64) -> Result<i64, String> {
        SqliteStore::new(self.clone()).portfolio_increment_likes(id)
    }
    fn portfolio_decrement_likes(&self, id: i64) -> Result<i64, String> {
        SqliteStore::new(self.clone()).portfolio_decrement_likes(id)
    }
    fn portfolio_update_seo_score(
        &self,
        id: i64,
        score: i32,
        issues_json: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).portfolio_update_seo_score(id, score, issues_json)
    }
    fn portfolio_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<PortfolioItem> {
        SqliteStore::new(self.clone()).portfolio_by_tag(tag_id, limit, offset)
    }
    fn portfolio_count_by_tag(&self, tag_id: i64) -> i64 {
        SqliteStore::new(self.clone()).portfolio_count_by_tag(tag_id)
    }
    fn comment_find_by_id(&self, id: i64) -> Option<crate::models::comment::Comment> {
        SqliteStore::new(self.clone()).comment_find_by_id(id)
    }
    fn comment_list(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<crate::models::comment::Comment> {
        SqliteStore::new(self.clone()).comment_list(status, limit, offset)
    }
    fn comment_for_post(
        &self,
        post_id: i64,
        content_type: &str,
    ) -> Vec<crate::models::comment::Comment> {
        SqliteStore::new(self.clone()).comment_for_post(post_id, content_type)
    }
    fn comment_count(&self, status: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).comment_count(status)
    }
    fn comment_create(&self, form: &crate::models::comment::CommentForm) -> Result<i64, String> {
        SqliteStore::new(self.clone()).comment_create(form)
    }
    fn comment_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).comment_update_status(id, status)
    }
    fn comment_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).comment_delete(id)
    }
    fn category_find_by_id(&self, id: i64) -> Option<Category> {
        SqliteStore::new(self.clone()).category_find_by_id(id)
    }
    fn category_find_by_slug(&self, slug: &str) -> Option<Category> {
        SqliteStore::new(self.clone()).category_find_by_slug(slug)
    }
    fn category_list(&self, type_filter: Option<&str>) -> Vec<Category> {
        SqliteStore::new(self.clone()).category_list(type_filter)
    }
    fn category_list_paginated(
        &self,
        type_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<Category> {
        SqliteStore::new(self.clone()).category_list_paginated(type_filter, limit, offset)
    }
    fn category_count(&self, type_filter: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).category_count(type_filter)
    }
    fn category_for_content(&self, content_id: i64, content_type: &str) -> Vec<Category> {
        SqliteStore::new(self.clone()).category_for_content(content_id, content_type)
    }
    fn category_count_items(&self, category_id: i64) -> i64 {
        SqliteStore::new(self.clone()).category_count_items(category_id)
    }
    fn category_create(&self, form: &crate::models::category::CategoryForm) -> Result<i64, String> {
        SqliteStore::new(self.clone()).category_create(form)
    }
    fn category_update(
        &self,
        id: i64,
        form: &crate::models::category::CategoryForm,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).category_update(id, form)
    }
    fn category_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).category_delete(id)
    }
    fn category_set_show_in_nav(&self, id: i64, show: bool) -> Result<(), String> {
        SqliteStore::new(self.clone()).category_set_show_in_nav(id, show)
    }
    fn category_list_nav_visible(&self, type_filter: Option<&str>) -> Vec<Category> {
        SqliteStore::new(self.clone()).category_list_nav_visible(type_filter)
    }
    fn category_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        category_ids: &[i64],
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).category_set_for_content(
            content_id,
            content_type,
            category_ids,
        )
    }
    fn tag_find_by_id(&self, id: i64) -> Option<Tag> {
        SqliteStore::new(self.clone()).tag_find_by_id(id)
    }
    fn tag_find_by_slug(&self, slug: &str) -> Option<Tag> {
        SqliteStore::new(self.clone()).tag_find_by_slug(slug)
    }
    fn tag_list(&self) -> Vec<Tag> {
        SqliteStore::new(self.clone()).tag_list()
    }
    fn tag_list_paginated(&self, limit: i64, offset: i64) -> Vec<Tag> {
        SqliteStore::new(self.clone()).tag_list_paginated(limit, offset)
    }
    fn tag_count(&self) -> i64 {
        SqliteStore::new(self.clone()).tag_count()
    }
    fn tag_for_content(&self, content_id: i64, content_type: &str) -> Vec<Tag> {
        SqliteStore::new(self.clone()).tag_for_content(content_id, content_type)
    }
    fn tag_count_items(&self, tag_id: i64) -> i64 {
        SqliteStore::new(self.clone()).tag_count_items(tag_id)
    }
    fn tag_create(&self, form: &crate::models::tag::TagForm) -> Result<i64, String> {
        SqliteStore::new(self.clone()).tag_create(form)
    }
    fn tag_update(&self, id: i64, form: &crate::models::tag::TagForm) -> Result<(), String> {
        SqliteStore::new(self.clone()).tag_update(id, form)
    }
    fn tag_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).tag_delete(id)
    }
    fn tag_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        tag_ids: &[i64],
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).tag_set_for_content(content_id, content_type, tag_ids)
    }
    fn tag_find_or_create(&self, name: &str) -> Result<i64, String> {
        SqliteStore::new(self.clone()).tag_find_or_create(name)
    }
    fn design_find_by_id(&self, id: i64) -> Option<Design> {
        SqliteStore::new(self.clone()).design_find_by_id(id)
    }
    fn design_find_by_slug(&self, slug: &str) -> Option<Design> {
        SqliteStore::new(self.clone()).design_find_by_slug(slug)
    }
    fn design_active(&self) -> Option<Design> {
        SqliteStore::new(self.clone()).design_active()
    }
    fn design_list(&self) -> Vec<Design> {
        SqliteStore::new(self.clone()).design_list()
    }
    fn design_activate(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).design_activate(id)
    }
    fn design_create(&self, name: &str) -> Result<i64, String> {
        SqliteStore::new(self.clone()).design_create(name)
    }
    fn design_duplicate(&self, id: i64, new_name: &str) -> Result<i64, String> {
        SqliteStore::new(self.clone()).design_duplicate(id, new_name)
    }
    fn design_delete(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).design_delete(id)
    }
    fn design_template_for_design(&self, design_id: i64) -> Vec<DesignTemplate> {
        SqliteStore::new(self.clone()).design_template_for_design(design_id)
    }
    fn design_template_get(&self, design_id: i64, template_type: &str) -> Option<DesignTemplate> {
        SqliteStore::new(self.clone()).design_template_get(design_id, template_type)
    }
    fn design_template_upsert(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).design_template_upsert(
            design_id,
            template_type,
            layout_html,
            style_css,
        )
    }
    fn design_template_upsert_full(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
        grapesjs_data: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).design_template_upsert_full(
            design_id,
            template_type,
            layout_html,
            style_css,
            grapesjs_data,
        )
    }
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
    ) {
        SqliteStore::new(self.clone()).audit_log(
            user_id,
            user_name,
            action,
            entity_type,
            entity_id,
            entity_title,
            details,
            ip_address,
        )
    }
    fn audit_list(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Vec<AuditEntry> {
        SqliteStore::new(self.clone()).audit_list(
            action_filter,
            entity_filter,
            user_filter,
            limit,
            offset,
        )
    }
    fn audit_count(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
    ) -> i64 {
        SqliteStore::new(self.clone()).audit_count(action_filter, entity_filter, user_filter)
    }
    fn audit_distinct_actions(&self) -> Vec<String> {
        SqliteStore::new(self.clone()).audit_distinct_actions()
    }
    fn audit_distinct_entity_types(&self) -> Vec<String> {
        SqliteStore::new(self.clone()).audit_distinct_entity_types()
    }
    fn audit_cleanup(&self, max_age_days: i64) -> Result<usize, String> {
        SqliteStore::new(self.clone()).audit_cleanup(max_age_days)
    }
    fn fw_is_banned(&self, ip: &str) -> bool {
        SqliteStore::new(self.clone()).fw_is_banned(ip)
    }
    fn fw_ban_create(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        expires_at: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone())
            .fw_ban_create(ip, reason, detail, expires_at, country, user_agent)
    }
    fn fw_ban_create_with_duration(
        &self,
        ip: &str,
        reason: &str,
        detail: Option<&str>,
        duration: &str,
        country: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone())
            .fw_ban_create_with_duration(ip, reason, detail, duration, country, user_agent)
    }
    fn fw_unban(&self, ip: &str) -> Result<usize, String> {
        SqliteStore::new(self.clone()).fw_unban(ip)
    }
    fn fw_unban_by_id(&self, id: i64) -> Result<usize, String> {
        SqliteStore::new(self.clone()).fw_unban_by_id(id)
    }
    fn fw_active_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        SqliteStore::new(self.clone()).fw_active_bans(limit, offset)
    }
    fn fw_active_count(&self) -> i64 {
        SqliteStore::new(self.clone()).fw_active_count()
    }
    fn fw_all_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        SqliteStore::new(self.clone()).fw_all_bans(limit, offset)
    }
    fn fw_expire_stale(&self) {
        SqliteStore::new(self.clone()).fw_expire_stale()
    }
    fn fw_event_log(
        &self,
        ip: &str,
        event_type: &str,
        detail: Option<&str>,
        country: Option<&str>,
        user_agent: Option<&str>,
        request_path: Option<&str>,
    ) {
        SqliteStore::new(self.clone()).fw_event_log(
            ip,
            event_type,
            detail,
            country,
            user_agent,
            request_path,
        )
    }
    fn fw_event_recent(&self, event_type: Option<&str>, limit: i64, offset: i64) -> Vec<FwEvent> {
        SqliteStore::new(self.clone()).fw_event_recent(event_type, limit, offset)
    }
    fn fw_event_count_all(&self, event_type: Option<&str>) -> i64 {
        SqliteStore::new(self.clone()).fw_event_count_all(event_type)
    }
    fn fw_event_count_since_hours(&self, hours: i64) -> i64 {
        SqliteStore::new(self.clone()).fw_event_count_since_hours(hours)
    }
    fn fw_event_count_for_ip_since(&self, ip: &str, event_type: &str, minutes: i64) -> i64 {
        SqliteStore::new(self.clone()).fw_event_count_for_ip_since(ip, event_type, minutes)
    }
    fn fw_event_top_ips(&self, limit: i64) -> Vec<(String, i64)> {
        SqliteStore::new(self.clone()).fw_event_top_ips(limit)
    }
    fn fw_event_counts_by_type(&self) -> Vec<(String, i64)> {
        SqliteStore::new(self.clone()).fw_event_counts_by_type()
    }
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
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).analytics_record(
            path,
            ip_hash,
            country,
            city,
            referrer,
            user_agent,
            device_type,
            browser,
        )
    }
    fn analytics_overview(&self, from: &str, to: &str) -> crate::models::analytics::OverviewStats {
        SqliteStore::new(self.clone()).analytics_overview(from, to)
    }
    fn analytics_flow_data(&self, from: &str, to: &str) -> Vec<crate::models::analytics::FlowNode> {
        SqliteStore::new(self.clone()).analytics_flow_data(from, to)
    }
    fn analytics_geo_data(
        &self,
        from: &str,
        to: &str,
    ) -> Vec<crate::models::analytics::CountEntry> {
        SqliteStore::new(self.clone()).analytics_geo_data(from, to)
    }
    fn analytics_stream_data(
        &self,
        from: &str,
        to: &str,
    ) -> Vec<crate::models::analytics::StreamEntry> {
        SqliteStore::new(self.clone()).analytics_stream_data(from, to)
    }
    fn analytics_calendar_data(
        &self,
        from: &str,
        to: &str,
    ) -> Vec<crate::models::analytics::DailyCount> {
        SqliteStore::new(self.clone()).analytics_calendar_data(from, to)
    }
    fn analytics_top_portfolio(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Vec<crate::models::analytics::CountEntry> {
        SqliteStore::new(self.clone()).analytics_top_portfolio(from, to, limit)
    }
    fn analytics_top_referrers(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Vec<crate::models::analytics::CountEntry> {
        SqliteStore::new(self.clone()).analytics_top_referrers(from, to, limit)
    }
    fn analytics_tag_relations(&self) -> Vec<crate::models::analytics::TagRelation> {
        SqliteStore::new(self.clone()).analytics_tag_relations()
    }
    fn order_find_by_id(&self, id: i64) -> Option<Order> {
        SqliteStore::new(self.clone()).order_find_by_id(id)
    }
    fn order_find_by_provider_order_id(&self, provider_order_id: &str) -> Option<Order> {
        SqliteStore::new(self.clone()).order_find_by_provider_order_id(provider_order_id)
    }
    fn order_list(&self, limit: i64, offset: i64) -> Vec<Order> {
        SqliteStore::new(self.clone()).order_list(limit, offset)
    }
    fn order_list_by_status(&self, status: &str, limit: i64, offset: i64) -> Vec<Order> {
        SqliteStore::new(self.clone()).order_list_by_status(status, limit, offset)
    }
    fn order_list_by_email(&self, email: &str, limit: i64, offset: i64) -> Vec<Order> {
        SqliteStore::new(self.clone()).order_list_by_email(email, limit, offset)
    }
    fn order_list_by_portfolio(&self, portfolio_id: i64) -> Vec<Order> {
        SqliteStore::new(self.clone()).order_list_by_portfolio(portfolio_id)
    }
    fn order_count(&self) -> i64 {
        SqliteStore::new(self.clone()).order_count()
    }
    fn order_count_by_status(&self, status: &str) -> i64 {
        SqliteStore::new(self.clone()).order_count_by_status(status)
    }
    fn order_total_revenue(&self) -> f64 {
        SqliteStore::new(self.clone()).order_total_revenue()
    }
    fn order_revenue_by_period(&self, days: i64) -> f64 {
        SqliteStore::new(self.clone()).order_revenue_by_period(days)
    }
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
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).order_create(
            portfolio_id,
            buyer_email,
            buyer_name,
            amount,
            currency,
            provider,
            provider_order_id,
            status,
        )
    }
    fn order_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).order_update_status(id, status)
    }
    fn order_update_provider_order_id(
        &self,
        id: i64,
        provider_order_id: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).order_update_provider_order_id(id, provider_order_id)
    }
    fn order_update_buyer_info(
        &self,
        id: i64,
        buyer_email: &str,
        buyer_name: &str,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).order_update_buyer_info(id, buyer_email, buyer_name)
    }
    fn order_find_completed_by_email_and_portfolio(
        &self,
        email: &str,
        portfolio_id: i64,
    ) -> Option<Order> {
        SqliteStore::new(self.clone())
            .order_find_completed_by_email_and_portfolio(email, portfolio_id)
    }
    fn download_token_find_by_token(&self, token: &str) -> Option<DownloadToken> {
        SqliteStore::new(self.clone()).download_token_find_by_token(token)
    }
    fn download_token_find_by_order(&self, order_id: i64) -> Option<DownloadToken> {
        SqliteStore::new(self.clone()).download_token_find_by_order(order_id)
    }
    fn download_token_increment(&self, id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).download_token_increment(id)
    }
    fn download_token_create(
        &self,
        order_id: i64,
        token: &str,
        max_downloads: i64,
        expires_at: NaiveDateTime,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).download_token_create(
            order_id,
            token,
            max_downloads,
            expires_at,
        )
    }
    fn license_find_by_order(&self, order_id: i64) -> Option<License> {
        SqliteStore::new(self.clone()).license_find_by_order(order_id)
    }
    fn license_find_by_key(&self, key: &str) -> Option<License> {
        SqliteStore::new(self.clone()).license_find_by_key(key)
    }
    fn license_create(&self, order_id: i64, license_key: &str) -> Result<i64, String> {
        SqliteStore::new(self.clone()).license_create(order_id, license_key)
    }
    fn passkey_list_for_user(&self, user_id: i64) -> Vec<crate::models::passkey::UserPasskey> {
        SqliteStore::new(self.clone()).passkey_list_for_user(user_id)
    }
    fn passkey_get_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Option<crate::models::passkey::UserPasskey> {
        SqliteStore::new(self.clone()).passkey_get_by_credential_id(credential_id)
    }
    fn passkey_count_for_user(&self, user_id: i64) -> i64 {
        SqliteStore::new(self.clone()).passkey_count_for_user(user_id)
    }
    fn passkey_create(
        &self,
        user_id: i64,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
        transports: &str,
        name: &str,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).passkey_create(
            user_id,
            credential_id,
            public_key,
            sign_count,
            transports,
            name,
        )
    }
    fn passkey_update_sign_count(
        &self,
        credential_id: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).passkey_update_sign_count(credential_id, sign_count)
    }
    fn passkey_update_public_key(
        &self,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).passkey_update_public_key(
            credential_id,
            public_key,
            sign_count,
        )
    }
    fn passkey_delete(&self, id: i64, user_id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).passkey_delete(id, user_id)
    }
    fn passkey_delete_all_for_user(&self, user_id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).passkey_delete_all_for_user(user_id)
    }
    fn import_list(&self) -> Vec<Import> {
        SqliteStore::new(self.clone()).import_list()
    }
    fn import_create(
        &self,
        source: &str,
        filename: Option<&str>,
        posts_count: i64,
        portfolio_count: i64,
        comments_count: i64,
        skipped_count: i64,
        log: Option<&str>,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).import_create(
            source,
            filename,
            posts_count,
            portfolio_count,
            comments_count,
            skipped_count,
            log,
        )
    }
    fn search_create_fts_table(&self) -> Result<(), String> {
        SqliteStore::new(self.clone()).search_create_fts_table()
    }
    fn search_rebuild_index(&self) -> Result<usize, String> {
        SqliteStore::new(self.clone()).search_rebuild_index()
    }
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
    ) {
        SqliteStore::new(self.clone()).search_upsert_item(
            item_type,
            item_id,
            title,
            html_body,
            slug,
            image,
            date,
            is_published,
        )
    }
    fn search_remove_item(&self, item_type: &str, item_id: i64) {
        SqliteStore::new(self.clone()).search_remove_item(item_type, item_id)
    }
    fn search_query(&self, query: &str, limit: i64) -> Vec<SearchResult> {
        SqliteStore::new(self.clone()).search_query(query, limit)
    }
    fn session_create(&self, user_id: i64, token: &str, expires_at: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).session_create(user_id, token, expires_at)
    }
    fn session_get_user_id(&self, token: &str) -> Option<i64> {
        SqliteStore::new(self.clone()).session_get_user_id(token)
    }
    fn session_delete(&self, token: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).session_delete(token)
    }
    fn session_delete_for_user(&self, user_id: i64) -> Result<(), String> {
        SqliteStore::new(self.clone()).session_delete_for_user(user_id)
    }
    fn session_create_full(
        &self,
        user_id: i64,
        token: &str,
        expires_at: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone())
            .session_create_full(user_id, token, expires_at, ip, user_agent)
    }
    fn session_cleanup_expired(&self) {
        SqliteStore::new(self.clone()).session_cleanup_expired()
    }
    fn session_count_recent_by_ip(&self, ip_hash: &str, minutes: i64) -> i64 {
        SqliteStore::new(self.clone()).session_count_recent_by_ip(ip_hash, minutes)
    }
    fn magic_link_create(
        &self,
        token: &str,
        email: &str,
        expires_minutes: i64,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).magic_link_create(token, email, expires_minutes)
    }
    fn magic_link_verify(&self, token: &str) -> Result<String, String> {
        SqliteStore::new(self.clone()).magic_link_verify(token)
    }
    fn like_exists(&self, portfolio_id: i64, ip_hash: &str) -> bool {
        SqliteStore::new(self.clone()).like_exists(portfolio_id, ip_hash)
    }
    fn like_add(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).like_add(portfolio_id, ip_hash)
    }
    fn like_remove(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        SqliteStore::new(self.clone()).like_remove(portfolio_id, ip_hash)
    }
    fn analytics_prune(&self, before_date: &str) -> Result<usize, String> {
        SqliteStore::new(self.clone()).analytics_prune(before_date)
    }
    fn analytics_count(&self) -> i64 {
        SqliteStore::new(self.clone()).analytics_count()
    }
    fn db_backend(&self) -> &str {
        "sqlite"
    }
    fn health_content_stats(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) {
        SqliteStore::new(self.clone()).health_content_stats()
    }
    fn health_referenced_files(&self) -> std::collections::HashSet<String> {
        SqliteStore::new(self.clone()).health_referenced_files()
    }
    fn health_session_cleanup(&self) -> Result<u64, String> {
        SqliteStore::new(self.clone()).health_session_cleanup()
    }
    fn health_unused_tags_cleanup(&self) -> Result<u64, String> {
        SqliteStore::new(self.clone()).health_unused_tags_cleanup()
    }
    fn health_analytics_prune(&self, days: u64) -> Result<u64, String> {
        SqliteStore::new(self.clone()).health_analytics_prune(days)
    }
    fn health_export_content(&self) -> Result<serde_json::Value, String> {
        SqliteStore::new(self.clone()).health_export_content()
    }
    fn magic_link_cleanup(&self) -> Result<usize, String> {
        SqliteStore::new(self.clone()).magic_link_cleanup()
    }
    fn task_publish_scheduled(&self) -> Result<usize, String> {
        SqliteStore::new(self.clone()).task_publish_scheduled()
    }
    fn task_cleanup_sessions(&self, max_age_days: i64) -> Result<usize, String> {
        SqliteStore::new(self.clone()).task_cleanup_sessions(max_age_days)
    }
    fn task_cleanup_analytics(&self, max_age_days: i64) -> Result<usize, String> {
        SqliteStore::new(self.clone()).task_cleanup_analytics(max_age_days)
    }
    fn mta_queue_push(
        &self,
        to: &str,
        from: &str,
        subject: &str,
        body: &str,
    ) -> Result<i64, String> {
        SqliteStore::new(self.clone()).mta_queue_push(to, from, subject, body)
    }
    fn mta_queue_pending(&self, limit: i64) -> Vec<crate::mta::queue::QueuedEmail> {
        SqliteStore::new(self.clone()).mta_queue_pending(limit)
    }
    fn mta_queue_update_status(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
        next_retry: Option<&str>,
    ) -> Result<(), String> {
        SqliteStore::new(self.clone()).mta_queue_update_status(id, status, error, next_retry)
    }
    fn mta_queue_sent_last_hour(&self) -> Result<u64, String> {
        SqliteStore::new(self.clone()).mta_queue_sent_last_hour()
    }
    fn mta_queue_stats(&self) -> (u64, u64, u64, u64) {
        SqliteStore::new(self.clone()).mta_queue_stats()
    }
    fn mta_queue_cleanup(&self, days: u64) -> Result<u64, String> {
        SqliteStore::new(self.clone()).mta_queue_cleanup(days)
    }
    fn raw_execute(&self, sql: &str) -> Result<usize, String> {
        SqliteStore::new(self.clone()).raw_execute(sql)
    }
    fn raw_query_i64(&self, sql: &str) -> Result<i64, String> {
        SqliteStore::new(self.clone()).raw_query_i64(sql)
    }
}
