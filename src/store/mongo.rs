use std::collections::HashMap;

use chrono::NaiveDateTime;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::ClientOptions;
use mongodb::sync::Client;
use mongodb::sync::Database;

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

/// MongoDB-backed implementation of the Store trait.
pub struct MongoStore {
    db: Database,
}

impl MongoStore {
    /// Create a new MongoStore by connecting to the given URI and database name.
    pub fn new(uri: &str, db_name: &str) -> Result<Self, String> {
        let client_options = ClientOptions::parse(uri).map_err(|e| e.to_string())?;
        let client = Client::with_options(client_options).map_err(|e| e.to_string())?;
        let db = client.database(db_name);
        Ok(Self { db })
    }

    /// Test connectivity by pinging the server.
    pub fn test_connection(&self) -> Result<(), String> {
        self.db
            .run_command(doc! { "ping": 1 }, None)
            .map_err(|e| format!("MongoDB connection test failed: {}", e))?;
        Ok(())
    }

    // ── Helper: get next auto-increment ID for a collection ──
    fn next_id(&self, collection_name: &str) -> Result<i64, String> {
        let counters = self.db.collection::<Document>("_counters");
        let filter = doc! { "_id": collection_name };
        let update = doc! { "$inc": { "seq": 1_i64 } };
        let opts = mongodb::options::FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .build();
        let result = counters
            .find_one_and_update(filter, update, opts)
            .map_err(|e| e.to_string())?;
        match result {
            Some(d) => d
                .get_i64("seq")
                .map_err(|e| format!("Failed to get seq: {}", e)),
            None => Err("Failed to generate ID".to_string()),
        }
    }

    // ── Helper: get a setting value ──
    fn get_setting_doc(&self, key: &str) -> Option<String> {
        let coll = self.db.collection::<Document>("settings");
        let d = coll.find_one(doc! { "key": key }, None).ok()??;
        d.get_str("value").ok().map(|s| s.to_string())
    }
}

impl Store for MongoStore {
    // ── Lifecycle ───────────────────────────────────────────────────

    fn run_migrations(&self) -> Result<(), String> {
        // Create indexes for all collections
        use mongodb::IndexModel;

        let settings = self.db.collection::<Document>("settings");
        settings
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "key": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let users = self.db.collection::<Document>("users");
        users
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "email": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let posts = self.db.collection::<Document>("posts");
        posts
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "slug": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;
        posts
            .create_index(
                IndexModel::builder().keys(doc! { "status": 1 }).build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let portfolio = self.db.collection::<Document>("portfolio");
        portfolio
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "slug": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let page_views = self.db.collection::<Document>("page_views");
        page_views
            .create_index(IndexModel::builder().keys(doc! { "path": 1 }).build(), None)
            .map_err(|e| e.to_string())?;
        page_views
            .create_index(
                IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let sessions = self.db.collection::<Document>("sessions");
        sessions
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "token": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .unique(true)
                            .build(),
                    )
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let fw_bans = self.db.collection::<Document>("fw_bans");
        fw_bans
            .create_index(IndexModel::builder().keys(doc! { "ip": 1 }).build(), None)
            .map_err(|e| e.to_string())?;

        let fw_events = self.db.collection::<Document>("fw_events");
        fw_events
            .create_index(IndexModel::builder().keys(doc! { "ip": 1 }).build(), None)
            .map_err(|e| e.to_string())?;
        fw_events
            .create_index(
                IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        let audit = self.db.collection::<Document>("audit_log");
        audit
            .create_index(
                IndexModel::builder().keys(doc! { "action": 1 }).build(),
                None,
            )
            .map_err(|e| e.to_string())?;
        audit
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "created_at": -1 })
                    .build(),
                None,
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), String> {
        // Seed default settings (INSERT OR IGNORE equivalent)
        let coll = self.db.collection::<Document>("settings");
        let defaults = crate::db::default_settings();
        for (key, value) in &defaults {
            let filter = doc! { "key": *key };
            let update = doc! { "$setOnInsert": { "key": *key, "value": *value } };
            let opts = mongodb::options::UpdateOptions::builder()
                .upsert(true)
                .build();
            coll.update_one(filter, update, opts)
                .map_err(|e| e.to_string())?;
        }

        // Seed image proxy secret if not present
        if self.get_setting_doc("image_proxy_secret").is_none() {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let bytes: [u8; 32] = rng.gen();
            let secret = hex::encode(bytes);
            self.setting_set("image_proxy_secret", &secret)?;
        }

        // Backfill legal page content if empty
        for (key, value) in &defaults {
            if (*key == "privacy_policy_content" || *key == "terms_of_use_content")
                && !value.is_empty()
            {
                let filter = doc! { "key": *key, "value": "" };
                let update = doc! { "$set": { "value": *value } };
                let _ = coll.update_one(filter, update, None);
            }
        }

        // Seed default designs if none exist
        let designs = self.db.collection::<Document>("designs");
        let count = designs
            .count_documents(doc! {}, None)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            let id1 = self.next_id("designs")?;
            designs
                .insert_one(doc! {
                    "id": id1,
                    "name": "Inkwell",
                    "slug": "inkwell",
                    "description": "A modern, wide-format journal theme with clean typography and generous whitespace. Full-width images, two-column single posts, and a refined reading experience.",
                    "layout_html": crate::render::INKWELL_SHELL_HTML,
                    "style_css": crate::render::INKWELL_DESIGN_CSS,
                    "thumbnail_path": Bson::Null,
                    "is_active": true,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                }, None)
                .map_err(|e| e.to_string())?;

            let id2 = self.next_id("designs")?;
            designs
                .insert_one(doc! {
                    "id": id2,
                    "name": "Oneguy",
                    "slug": "oneguy",
                    "description": "A clean, sidebar-driven portfolio theme for photographers and illustrators. Fixed navigation, masonry and grid layouts, minimal journal — designed to let your work speak for itself.",
                    "layout_html": crate::render::ONEGUY_SHELL_HTML,
                    "style_css": crate::render::ONEGUY_DESIGN_CSS,
                    "thumbnail_path": Bson::Null,
                    "is_active": false,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                }, None)
                .map_err(|e| e.to_string())?;
        }

        // Ensure Inkwell design exists (migration for existing MongoDB databases)
        let inkwell_exists = designs
            .count_documents(doc! { "slug": "inkwell" }, None)
            .unwrap_or(0)
            > 0;
        if !inkwell_exists {
            let id = self.next_id("designs")?;
            designs
                .insert_one(doc! {
                    "id": id,
                    "name": "Inkwell",
                    "slug": "inkwell",
                    "description": "A modern, wide-format journal theme with clean typography and generous whitespace. Full-width images, two-column single posts, and a refined reading experience.",
                    "layout_html": crate::render::INKWELL_SHELL_HTML,
                    "style_css": crate::render::INKWELL_DESIGN_CSS,
                    "thumbnail_path": Bson::Null,
                    "is_active": false,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                }, None)
                .map_err(|e| e.to_string())?;
        }

        // Keep design CSS/HTML in sync with binary constants
        let _ = designs.update_one(
            doc! { "slug": "inkwell" },
            doc! { "$set": {
                "layout_html": crate::render::INKWELL_SHELL_HTML,
                "style_css": crate::render::INKWELL_DESIGN_CSS,
            }},
            None,
        );
        let _ = designs.update_one(
            doc! { "slug": "oneguy" },
            doc! { "$set": {
                "layout_html": crate::render::ONEGUY_SHELL_HTML,
                "style_css": crate::render::ONEGUY_DESIGN_CSS,
            }},
            None,
        );

        // Backfill Oneguy description if empty
        let _ = designs.update_one(
            doc! { "slug": "oneguy", "description": "" },
            doc! { "$set": { "description": "A clean, sidebar-driven portfolio theme for photographers and illustrators. Fixed navigation, masonry and grid layouts, minimal journal — designed to let your work speak for itself." } },
            None,
        );

        Ok(())
    }

    // ── Settings ────────────────────────────────────────────────────

    fn setting_get(&self, key: &str) -> Option<String> {
        self.get_setting_doc(key)
    }

    fn setting_set(&self, key: &str, value: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("settings");
        let opts = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        coll.update_one(
            doc! { "key": key },
            doc! { "$set": { "key": key, "value": value } },
            opts,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn setting_set_many(&self, settings: &HashMap<String, String>) -> Result<(), String> {
        for (key, value) in settings {
            self.setting_set(key, value)?;
        }
        Ok(())
    }

    fn setting_get_group(&self, prefix: &str) -> HashMap<String, String> {
        let coll = self.db.collection::<Document>("settings");
        let regex = format!("^{}", regex::escape(prefix));
        let filter = doc! { "key": { "$regex": &regex } };
        let mut map = HashMap::new();
        if let Ok(cursor) = coll.find(filter, None) {
            for doc in cursor.flatten() {
                if let (Ok(k), Ok(v)) = (doc.get_str("key"), doc.get_str("value")) {
                    map.insert(k.to_string(), v.to_string());
                }
            }
        }
        map
    }

    fn setting_all(&self) -> HashMap<String, String> {
        let coll = self.db.collection::<Document>("settings");
        let mut map = HashMap::new();
        if let Ok(cursor) = coll.find(doc! {}, None) {
            for doc in cursor.flatten() {
                if let (Ok(k), Ok(v)) = (doc.get_str("key"), doc.get_str("value")) {
                    map.insert(k.to_string(), v.to_string());
                }
            }
        }
        map
    }

    fn setting_delete(&self, key: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("settings");
        coll.delete_one(doc! { "key": key }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Users ───────────────────────────────────────────────────────

    fn user_get_by_id(&self, id: i64) -> Option<User> {
        let coll = self.db.collection::<Document>("users");
        let doc = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_user(&doc)
    }

    fn user_get_by_email(&self, email: &str) -> Option<User> {
        let coll = self.db.collection::<Document>("users");
        let doc = coll.find_one(doc! { "email": email }, None).ok()??;
        doc_to_user(&doc)
    }

    fn user_list_all(&self) -> Vec<User> {
        let coll = self.db.collection::<Document>("users");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": 1 })
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_user(&d))
            .collect()
    }

    fn user_list_paginated(&self, role: Option<&str>, limit: i64, offset: i64) -> Vec<User> {
        let coll = self.db.collection::<Document>("users");
        let filter = match role {
            Some(r) => doc! { "role": r },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": 1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_user(&d))
            .collect()
    }

    fn user_count(&self) -> i64 {
        let coll = self.db.collection::<Document>("users");
        coll.count_documents(doc! {}, None).unwrap_or(0) as i64
    }

    fn user_count_filtered(&self, role: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("users");
        let filter = match role {
            Some(r) => doc! { "role": r },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }

    fn user_count_by_role(&self, role: &str) -> i64 {
        let coll = self.db.collection::<Document>("users");
        coll.count_documents(doc! { "role": role }, None)
            .unwrap_or(0) as i64
    }

    fn user_create(
        &self,
        email: &str,
        password_hash: &str,
        display_name: &str,
        role: &str,
    ) -> Result<i64, String> {
        let id = self.next_id("users")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("users");
        coll.insert_one(
            doc! {
                "id": id,
                "email": email,
                "password_hash": password_hash,
                "display_name": display_name,
                "role": role,
                "status": "active",
                "avatar": "",
                "mfa_enabled": false,
                "mfa_secret": "",
                "mfa_recovery_codes": "[]",
                "last_login_at": Bson::Null,
                "created_at": &now,
                "updated_at": &now,
                "auth_method": "password",
                "auth_method_fallback": "password",
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn user_update_profile(
        &self,
        id: i64,
        display_name: &str,
        email: &str,
        avatar: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": {
                "display_name": display_name,
                "email": email,
                "avatar": avatar,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_update_role(&self, id: i64, role: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "role": role, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_update_password(&self, id: i64, password_hash: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "password_hash": password_hash, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_update_avatar(&self, id: i64, avatar: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "avatar": avatar, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_touch_last_login(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "last_login_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_update_mfa(
        &self,
        id: i64,
        enabled: bool,
        secret: &str,
        recovery_codes: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": {
                "mfa_enabled": enabled,
                "mfa_secret": secret,
                "mfa_recovery_codes": recovery_codes,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_lock(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": "locked", "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        // Delete sessions
        self.session_delete_for_user(id)?;
        Ok(())
    }

    fn user_unlock(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": "active", "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_delete(&self, id: i64) -> Result<(), String> {
        self.session_delete_for_user(id)?;
        let users = self.db.collection::<Document>("users");
        users
            .delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn user_update_auth_method(&self, id: i64, method: &str, fallback: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("users");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": {
                "auth_method": method,
                "auth_method_fallback": fallback,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Posts ────────────────────────────────────────────────────────
    // TODO: Full MongoDB implementations for posts, portfolio, comments,
    // categories, tags, designs, audit, firewall, analytics, orders,
    // passkeys, imports, search, sessions, likes.
    // These follow the same pattern as users above — BSON documents
    // with next_id() for auto-increment, find/insert/update/delete.

    fn post_find_by_id(&self, id: i64) -> Option<Post> {
        let coll = self.db.collection::<Document>("posts");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_post(&d)
    }
    fn post_find_by_slug(&self, slug: &str) -> Option<Post> {
        let coll = self.db.collection::<Document>("posts");
        let d = coll.find_one(doc! { "slug": slug }, None).ok()??;
        doc_to_post(&d)
    }
    fn post_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Post> {
        let coll = self.db.collection::<Document>("posts");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_post(&d))
            .collect()
    }
    fn post_count(&self, status: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("posts");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn post_create(&self, form: &PostForm) -> Result<i64, String> {
        let id = self.next_id("posts")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("posts");
        coll.insert_one(
            doc! {
                "id": id,
                "title": &form.title,
                "slug": &form.slug,
                "content_json": &form.content_json,
                "content_html": &form.content_html,
                "excerpt": form.excerpt.as_deref(),
                "featured_image": form.featured_image.as_deref(),
                "meta_title": form.meta_title.as_deref(),
                "meta_description": form.meta_description.as_deref(),
                "status": &form.status,
                "published_at": form.published_at.as_deref(),
                "created_at": &now,
                "updated_at": &now,
                "seo_score": -1_i32,
                "seo_issues": "[]",
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn post_update(&self, id: i64, form: &PostForm) -> Result<(), String> {
        let coll = self.db.collection::<Document>("posts");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": {
                "title": &form.title,
                "slug": &form.slug,
                "content_json": &form.content_json,
                "content_html": &form.content_html,
                "excerpt": form.excerpt.as_deref(),
                "featured_image": form.featured_image.as_deref(),
                "meta_title": form.meta_title.as_deref(),
                "meta_description": form.meta_description.as_deref(),
                "status": &form.status,
                "published_at": form.published_at.as_deref(),
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn post_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("posts");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn post_prev_published(&self, published_at: &NaiveDateTime) -> Option<Post> {
        let coll = self.db.collection::<Document>("posts");
        let ts = published_at.format("%Y-%m-%dT%H:%M:%S").to_string();
        let opts = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "published_at": -1 })
            .build();
        let d = coll
            .find_one(
                doc! { "status": "published", "published_at": { "$lt": &ts } },
                opts,
            )
            .ok()??;
        doc_to_post(&d)
    }
    fn post_next_published(&self, published_at: &NaiveDateTime) -> Option<Post> {
        let coll = self.db.collection::<Document>("posts");
        let ts = published_at.format("%Y-%m-%dT%H:%M:%S").to_string();
        let opts = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "published_at": 1 })
            .build();
        let d = coll
            .find_one(
                doc! { "status": "published", "published_at": { "$gt": &ts } },
                opts,
            )
            .ok()??;
        doc_to_post(&d)
    }
    fn post_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("posts");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": status, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn post_update_seo_score(&self, id: i64, score: i32, issues_json: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("posts");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "seo_score": score, "seo_issues": issues_json } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn post_archives(&self) -> Vec<(String, String, i64)> {
        let coll = self.db.collection::<Document>("posts");
        let pipeline = vec![
            doc! { "$match": { "status": "published", "published_at": { "$ne": null } } },
            doc! { "$addFields": {
                "_dt": { "$dateFromString": { "dateString": "$published_at", "onError": null } }
            }},
            doc! { "$match": { "_dt": { "$ne": null } } },
            doc! { "$group": {
                "_id": {
                    "year": { "$dateToString": { "format": "%Y", "date": "$_dt" } },
                    "month": { "$dateToString": { "format": "%m", "date": "$_dt" } },
                },
                "count": { "$sum": 1 }
            }},
            doc! { "$sort": { "_id.year": -1, "_id.month": -1 } },
        ];
        let cursor = match coll.aggregate(pipeline, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| {
                let id = d.get_document("_id").ok()?;
                let year = id.get_str("year").ok()?.to_string();
                let month = id.get_str("month").ok()?.to_string();
                let count = d.get_i32("count").unwrap_or(0) as i64;
                Some((year, month, count))
            })
            .collect()
    }

    fn post_by_year_month(&self, year: &str, month: &str, limit: i64, offset: i64) -> Vec<Post> {
        let coll = self.db.collection::<Document>("posts");
        let year_prefix = format!("{}-{}", year, month);
        let filter = doc! {
            "status": "published",
            "published_at": { "$regex": format!("^{}", year_prefix) }
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "published_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_post(&d))
            .collect()
    }

    fn post_count_by_year_month(&self, year: &str, month: &str) -> i64 {
        let coll = self.db.collection::<Document>("posts");
        let year_prefix = format!("{}-{}", year, month);
        coll.count_documents(
            doc! {
                "status": "published",
                "published_at": { "$regex": format!("^{}", year_prefix) }
            },
            None,
        )
        .unwrap_or(0) as i64
    }

    fn post_by_category(&self, category_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        let coll_cc = self.db.collection::<Document>("content_categories");
        let ids: Vec<i64> = coll_cc
            .find(
                doc! { "category_id": category_id, "content_type": "post" },
                None,
            )
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("posts");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "published_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! { "id": { "$in": &ids }, "status": "published" }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_post(&d))
            .collect()
    }

    fn post_count_by_category(&self, category_id: i64) -> i64 {
        let coll_cc = self.db.collection::<Document>("content_categories");
        let ids: Vec<i64> = coll_cc
            .find(
                doc! { "category_id": category_id, "content_type": "post" },
                None,
            )
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return 0;
        }
        let coll = self.db.collection::<Document>("posts");
        coll.count_documents(doc! { "id": { "$in": &ids }, "status": "published" }, None)
            .unwrap_or(0) as i64
    }

    fn post_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<Post> {
        let coll_ct = self.db.collection::<Document>("content_tags");
        let ids: Vec<i64> = coll_ct
            .find(doc! { "tag_id": tag_id, "content_type": "post" }, None)
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("posts");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "published_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! { "id": { "$in": &ids }, "status": "published" }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_post(&d))
            .collect()
    }

    fn post_count_by_tag(&self, tag_id: i64) -> i64 {
        let coll_ct = self.db.collection::<Document>("content_tags");
        let ids: Vec<i64> = coll_ct
            .find(doc! { "tag_id": tag_id, "content_type": "post" }, None)
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return 0;
        }
        let coll = self.db.collection::<Document>("posts");
        coll.count_documents(doc! { "id": { "$in": &ids }, "status": "published" }, None)
            .unwrap_or(0) as i64
    }

    fn portfolio_find_by_id(&self, id: i64) -> Option<PortfolioItem> {
        let coll = self.db.collection::<Document>("portfolio");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_portfolio(&d)
    }
    fn portfolio_find_by_slug(&self, slug: &str) -> Option<PortfolioItem> {
        let coll = self.db.collection::<Document>("portfolio");
        let d = coll.find_one(doc! { "slug": slug }, None).ok()??;
        doc_to_portfolio(&d)
    }
    fn portfolio_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<PortfolioItem> {
        let coll = self.db.collection::<Document>("portfolio");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_portfolio(&d))
            .collect()
    }
    fn portfolio_count(&self, status: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("portfolio");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn portfolio_by_category(
        &self,
        category_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Vec<PortfolioItem> {
        let cat = match self.category_find_by_slug(category_slug) {
            Some(c) => c,
            None => return vec![],
        };
        let cc = self.db.collection::<Document>("content_categories");
        let ids: Vec<i64> = cc
            .find(
                doc! { "category_id": cat.id, "content_type": "portfolio" },
                None,
            )
            .map(|cur| {
                cur.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("portfolio");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let bson_ids: Vec<Bson> = ids.iter().map(|&i| Bson::Int64(i)).collect();
        let cursor = match coll.find(
            doc! { "id": { "$in": bson_ids }, "status": "published" },
            opts,
        ) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_portfolio(&d))
            .collect()
    }
    fn portfolio_create(&self, form: &PortfolioForm) -> Result<i64, String> {
        let id = self.next_id("portfolio")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("portfolio");
        coll.insert_one(
            doc! {
                "id": id,
                "title": &form.title,
                "slug": &form.slug,
                "description_json": form.description_json.as_deref(),
                "description_html": form.description_html.as_deref(),
                "image_path": &form.image_path,
                "thumbnail_path": form.thumbnail_path.as_deref(),
                "meta_title": form.meta_title.as_deref(),
                "meta_description": form.meta_description.as_deref(),
                "sell_enabled": form.sell_enabled.unwrap_or(false),
                "price": form.price,
                "purchase_note": form.purchase_note.as_deref().unwrap_or(""),
                "payment_provider": form.payment_provider.as_deref().unwrap_or(""),
                "download_file_path": form.download_file_path.as_deref().unwrap_or(""),
                "likes": 0_i64,
                "status": &form.status,
                "published_at": form.published_at.as_deref(),
                "created_at": &now,
                "updated_at": &now,
                "seo_score": -1_i32,
                "seo_issues": "[]",
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn portfolio_update(&self, id: i64, form: &PortfolioForm) -> Result<(), String> {
        let coll = self.db.collection::<Document>("portfolio");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": {
                "title": &form.title,
                "slug": &form.slug,
                "description_json": form.description_json.as_deref(),
                "description_html": form.description_html.as_deref(),
                "image_path": &form.image_path,
                "thumbnail_path": form.thumbnail_path.as_deref(),
                "meta_title": form.meta_title.as_deref(),
                "meta_description": form.meta_description.as_deref(),
                "sell_enabled": form.sell_enabled.unwrap_or(false),
                "price": form.price,
                "purchase_note": form.purchase_note.as_deref().unwrap_or(""),
                "payment_provider": form.payment_provider.as_deref().unwrap_or(""),
                "download_file_path": form.download_file_path.as_deref().unwrap_or(""),
                "status": &form.status,
                "published_at": form.published_at.as_deref(),
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn portfolio_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("portfolio");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": status, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn portfolio_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("portfolio");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn portfolio_increment_likes(&self, id: i64) -> Result<i64, String> {
        let coll = self.db.collection::<Document>("portfolio");
        let opts = mongodb::options::FindOneAndUpdateOptions::builder()
            .return_document(mongodb::options::ReturnDocument::After)
            .build();
        let d = coll
            .find_one_and_update(doc! { "id": id }, doc! { "$inc": { "likes": 1_i64 } }, opts)
            .map_err(|e| e.to_string())?
            .ok_or("Not found")?;
        Ok(d.get_i64("likes").unwrap_or(0))
    }
    fn portfolio_decrement_likes(&self, id: i64) -> Result<i64, String> {
        let coll = self.db.collection::<Document>("portfolio");
        let opts = mongodb::options::FindOneAndUpdateOptions::builder()
            .return_document(mongodb::options::ReturnDocument::After)
            .build();
        let d = coll
            .find_one_and_update(
                doc! { "id": id, "likes": { "$gt": 0 } },
                doc! { "$inc": { "likes": -1_i64 } },
                opts,
            )
            .map_err(|e| e.to_string())?
            .ok_or("Not found or already 0")?;
        Ok(d.get_i64("likes").unwrap_or(0))
    }
    fn portfolio_update_seo_score(
        &self,
        id: i64,
        score: i32,
        issues_json: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("portfolio");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "seo_score": score, "seo_issues": issues_json } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn portfolio_by_tag(&self, tag_id: i64, limit: i64, offset: i64) -> Vec<PortfolioItem> {
        let coll_ct = self.db.collection::<Document>("content_tags");
        let ids: Vec<i64> = coll_ct
            .find(doc! { "tag_id": tag_id, "content_type": "portfolio" }, None)
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("portfolio");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! { "id": { "$in": &ids }, "status": "published" }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_portfolio(&d))
            .collect()
    }

    fn portfolio_count_by_tag(&self, tag_id: i64) -> i64 {
        let coll_ct = self.db.collection::<Document>("content_tags");
        let ids: Vec<i64> = coll_ct
            .find(doc! { "tag_id": tag_id, "content_type": "portfolio" }, None)
            .ok()
            .map(|c| {
                c.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("content_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return 0;
        }
        let coll = self.db.collection::<Document>("portfolio");
        coll.count_documents(doc! { "id": { "$in": &ids }, "status": "published" }, None)
            .unwrap_or(0) as i64
    }

    fn comment_find_by_id(&self, id: i64) -> Option<Comment> {
        let coll = self.db.collection::<Document>("comments");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_comment(&d)
    }
    fn comment_list(&self, status: Option<&str>, limit: i64, offset: i64) -> Vec<Comment> {
        let coll = self.db.collection::<Document>("comments");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_comment(&d))
            .collect()
    }
    fn comment_for_post(&self, post_id: i64, content_type: &str) -> Vec<Comment> {
        let coll = self.db.collection::<Document>("comments");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": 1 })
            .build();
        let cursor = match coll.find(
            doc! { "post_id": post_id, "content_type": content_type, "status": "approved" },
            opts,
        ) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_comment(&d))
            .collect()
    }
    fn comment_count(&self, status: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("comments");
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn comment_create(&self, form: &CommentForm) -> Result<i64, String> {
        let id = self.next_id("comments")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("comments");
        coll.insert_one(
            doc! {
                "id": id,
                "post_id": form.post_id,
                "content_type": form.content_type.as_deref().unwrap_or("post"),
                "author_name": &form.author_name,
                "author_email": form.author_email.as_deref(),
                "body": &form.body,
                "status": "pending",
                "parent_id": form.parent_id,
                "created_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn comment_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("comments");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": status } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn comment_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("comments");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn category_find_by_id(&self, id: i64) -> Option<Category> {
        let coll = self.db.collection::<Document>("categories");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_category(&d)
    }
    fn category_find_by_slug(&self, slug: &str) -> Option<Category> {
        let coll = self.db.collection::<Document>("categories");
        let d = coll.find_one(doc! { "slug": slug }, None).ok()??;
        doc_to_category(&d)
    }
    fn category_list(&self, type_filter: Option<&str>) -> Vec<Category> {
        let coll = self.db.collection::<Document>("categories");
        let filter = match type_filter {
            Some(t) => doc! { "type": t },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "name": 1 })
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_category(&d))
            .collect()
    }
    fn category_list_paginated(
        &self,
        type_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Vec<Category> {
        let coll = self.db.collection::<Document>("categories");
        let filter = match type_filter {
            Some(t) => doc! { "type": t },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "name": 1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_category(&d))
            .collect()
    }
    fn category_count(&self, type_filter: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("categories");
        let filter = match type_filter {
            Some(t) => doc! { "type": t },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn category_for_content(&self, content_id: i64, content_type: &str) -> Vec<Category> {
        let cc = self.db.collection::<Document>("content_categories");
        let cat_ids: Vec<i64> = cc
            .find(
                doc! { "content_id": content_id, "content_type": content_type },
                None,
            )
            .map(|cur| {
                cur.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("category_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if cat_ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("categories");
        let bson_ids: Vec<Bson> = cat_ids.iter().map(|&i| Bson::Int64(i)).collect();
        let cursor = match coll.find(doc! { "id": { "$in": bson_ids } }, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_category(&d))
            .collect()
    }
    fn category_count_items(&self, category_id: i64) -> i64 {
        let cc = self.db.collection::<Document>("content_categories");
        cc.count_documents(doc! { "category_id": category_id }, None)
            .unwrap_or(0) as i64
    }
    fn category_create(&self, form: &CategoryForm) -> Result<i64, String> {
        let id = self.next_id("categories")?;
        let coll = self.db.collection::<Document>("categories");
        coll.insert_one(
            doc! {
                "id": id,
                "name": &form.name,
                "slug": &form.slug,
                "type": &form.r#type,
                "show_in_nav": true,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn category_update(&self, id: i64, form: &CategoryForm) -> Result<(), String> {
        let coll = self.db.collection::<Document>("categories");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "name": &form.name, "slug": &form.slug, "type": &form.r#type } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn category_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("categories");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        // Also remove content_categories links
        let cc = self.db.collection::<Document>("content_categories");
        let _ = cc.delete_many(doc! { "category_id": id }, None);
        Ok(())
    }
    fn category_set_show_in_nav(&self, id: i64, show: bool) -> Result<(), String> {
        let coll = self.db.collection::<Document>("categories");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "show_in_nav": show } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn category_list_nav_visible(&self, type_filter: Option<&str>) -> Vec<Category> {
        let coll = self.db.collection::<Document>("categories");
        let mut filter = doc! { "show_in_nav": true };
        if let Some(t) = type_filter {
            filter.insert("type", t);
        }
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "name": 1 })
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_category(&d))
            .collect()
    }
    fn category_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        category_ids: &[i64],
    ) -> Result<(), String> {
        let cc = self.db.collection::<Document>("content_categories");
        cc.delete_many(
            doc! { "content_id": content_id, "content_type": content_type },
            None,
        )
        .map_err(|e| e.to_string())?;
        for &cat_id in category_ids {
            cc.insert_one(
                doc! { "content_id": content_id, "content_type": content_type, "category_id": cat_id },
                None,
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn tag_find_by_id(&self, id: i64) -> Option<Tag> {
        let coll = self.db.collection::<Document>("tags");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_tag(&d)
    }
    fn tag_find_by_slug(&self, slug: &str) -> Option<Tag> {
        let coll = self.db.collection::<Document>("tags");
        let d = coll.find_one(doc! { "slug": slug }, None).ok()??;
        doc_to_tag(&d)
    }
    fn tag_list(&self) -> Vec<Tag> {
        let coll = self.db.collection::<Document>("tags");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "name": 1 })
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_tag(&d))
            .collect()
    }
    fn tag_list_paginated(&self, limit: i64, offset: i64) -> Vec<Tag> {
        let coll = self.db.collection::<Document>("tags");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "name": 1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_tag(&d))
            .collect()
    }
    fn tag_count(&self) -> i64 {
        let coll = self.db.collection::<Document>("tags");
        coll.count_documents(doc! {}, None).unwrap_or(0) as i64
    }
    fn tag_for_content(&self, content_id: i64, content_type: &str) -> Vec<Tag> {
        let ct = self.db.collection::<Document>("content_tags");
        let tag_ids: Vec<i64> = ct
            .find(
                doc! { "content_id": content_id, "content_type": content_type },
                None,
            )
            .map(|cur| {
                cur.filter_map(|r| r.ok())
                    .filter_map(|d| d.get_i64("tag_id").ok())
                    .collect()
            })
            .unwrap_or_default();
        if tag_ids.is_empty() {
            return vec![];
        }
        let coll = self.db.collection::<Document>("tags");
        let bson_ids: Vec<Bson> = tag_ids.iter().map(|&i| Bson::Int64(i)).collect();
        let cursor = match coll.find(doc! { "id": { "$in": bson_ids } }, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_tag(&d))
            .collect()
    }
    fn tag_count_items(&self, tag_id: i64) -> i64 {
        let ct = self.db.collection::<Document>("content_tags");
        ct.count_documents(doc! { "tag_id": tag_id }, None)
            .unwrap_or(0) as i64
    }
    fn tag_create(&self, form: &TagForm) -> Result<i64, String> {
        let id = self.next_id("tags")?;
        let coll = self.db.collection::<Document>("tags");
        coll.insert_one(
            doc! { "id": id, "name": &form.name, "slug": &form.slug },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn tag_update(&self, id: i64, form: &TagForm) -> Result<(), String> {
        let coll = self.db.collection::<Document>("tags");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "name": &form.name, "slug": &form.slug } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn tag_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("tags");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        let ct = self.db.collection::<Document>("content_tags");
        let _ = ct.delete_many(doc! { "tag_id": id }, None);
        Ok(())
    }
    fn tag_set_for_content(
        &self,
        content_id: i64,
        content_type: &str,
        tag_ids: &[i64],
    ) -> Result<(), String> {
        let ct = self.db.collection::<Document>("content_tags");
        ct.delete_many(
            doc! { "content_id": content_id, "content_type": content_type },
            None,
        )
        .map_err(|e| e.to_string())?;
        for &tag_id in tag_ids {
            ct.insert_one(
                doc! { "content_id": content_id, "content_type": content_type, "tag_id": tag_id },
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn tag_find_or_create(&self, name: &str) -> Result<i64, String> {
        let slug = name.to_lowercase().replace(' ', "-");
        if let Some(t) = self.tag_find_by_slug(&slug) {
            return Ok(t.id);
        }
        let form = TagForm {
            name: name.to_string(),
            slug,
        };
        self.tag_create(&form)
    }

    fn design_find_by_id(&self, id: i64) -> Option<Design> {
        let coll = self.db.collection::<Document>("designs");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_design(&d)
    }
    fn design_find_by_slug(&self, slug: &str) -> Option<Design> {
        let coll = self.db.collection::<Document>("designs");
        let d = coll.find_one(doc! { "slug": slug }, None).ok()??;
        doc_to_design(&d)
    }
    fn design_active(&self) -> Option<Design> {
        let coll = self.db.collection::<Document>("designs");
        let d = coll.find_one(doc! { "is_active": true }, None).ok()??;
        doc_to_design(&d)
    }
    fn design_list(&self) -> Vec<Design> {
        let coll = self.db.collection::<Document>("designs");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": 1 })
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_design(&d))
            .collect()
    }
    fn design_activate(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("designs");
        // Deactivate all
        coll.update_many(doc! {}, doc! { "$set": { "is_active": false } }, None)
            .map_err(|e| e.to_string())?;
        // Activate the chosen one
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "is_active": true, "updated_at": chrono::Utc::now().to_rfc3339() } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn design_create(&self, name: &str) -> Result<i64, String> {
        let id = self.next_id("designs")?;
        let now = chrono::Utc::now().to_rfc3339();
        let slug = name.to_lowercase().replace(' ', "-");
        let coll = self.db.collection::<Document>("designs");
        coll.insert_one(
            doc! {
                "id": id,
                "name": name,
                "slug": &slug,
                "description": "",
                "layout_html": "",
                "style_css": "",
                "thumbnail_path": Bson::Null,
                "is_active": false,
                "created_at": &now,
                "updated_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn design_duplicate(&self, id: i64, new_name: &str) -> Result<i64, String> {
        let src = self.design_find_by_id(id).ok_or("Design not found")?;
        let new_id = self.next_id("designs")?;
        let now = chrono::Utc::now().to_rfc3339();
        let slug = new_name.to_lowercase().replace(' ', "-");
        let coll = self.db.collection::<Document>("designs");
        coll.insert_one(
            doc! {
                "id": new_id,
                "name": new_name,
                "slug": &slug,
                "description": &src.description,
                "layout_html": &src.layout_html,
                "style_css": &src.style_css,
                "thumbnail_path": Bson::Null,
                "is_active": false,
                "created_at": &now,
                "updated_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        // Duplicate templates
        let templates = self.design_template_for_design(id);
        for t in templates {
            self.design_template_upsert_full(
                new_id,
                &t.template_type,
                &t.layout_html,
                &t.style_css,
                &t.grapesjs_data,
            )?;
        }
        Ok(new_id)
    }
    fn design_delete(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("designs");
        coll.delete_one(doc! { "id": id }, None)
            .map_err(|e| e.to_string())?;
        let tmpl = self.db.collection::<Document>("design_templates");
        let _ = tmpl.delete_many(doc! { "design_id": id }, None);
        Ok(())
    }

    fn design_template_for_design(&self, design_id: i64) -> Vec<DesignTemplate> {
        let coll = self.db.collection::<Document>("design_templates");
        let cursor = match coll.find(doc! { "design_id": design_id }, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_design_template(&d))
            .collect()
    }
    fn design_template_get(&self, design_id: i64, template_type: &str) -> Option<DesignTemplate> {
        let coll = self.db.collection::<Document>("design_templates");
        let d = coll
            .find_one(
                doc! { "design_id": design_id, "template_type": template_type },
                None,
            )
            .ok()??;
        doc_to_design_template(&d)
    }
    fn design_template_upsert(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
    ) -> Result<(), String> {
        self.design_template_upsert_full(design_id, template_type, layout_html, style_css, "")
    }
    fn design_template_upsert_full(
        &self,
        design_id: i64,
        template_type: &str,
        layout_html: &str,
        style_css: &str,
        grapesjs_data: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("design_templates");
        let now = chrono::Utc::now().to_rfc3339();
        let filter = doc! { "design_id": design_id, "template_type": template_type };
        let existing = coll.find_one(filter.clone(), None).ok().flatten();
        if existing.is_some() {
            coll.update_one(
                filter,
                doc! { "$set": {
                    "layout_html": layout_html,
                    "style_css": style_css,
                    "grapesjs_data": grapesjs_data,
                    "updated_at": &now,
                }},
                None,
            )
            .map_err(|e| e.to_string())?;
        } else {
            let id = self.next_id("design_templates")?;
            coll.insert_one(
                doc! {
                    "id": id,
                    "design_id": design_id,
                    "template_type": template_type,
                    "layout_html": layout_html,
                    "style_css": style_css,
                    "grapesjs_data": grapesjs_data,
                    "updated_at": &now,
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn audit_log(
        &self,
        _user_id: Option<i64>,
        _user_name: Option<&str>,
        _action: &str,
        _entity_type: Option<&str>,
        _entity_id: Option<i64>,
        _entity_title: Option<&str>,
        _details: Option<&str>,
        _ip_address: Option<&str>,
    ) {
        // Fire-and-forget: best effort insert
        let coll = self.db.collection::<Document>("audit_log");
        let _ = coll.insert_one(
            doc! {
                "user_id": _user_id,
                "user_name": _user_name,
                "action": _action,
                "entity_type": _entity_type,
                "entity_id": _entity_id,
                "entity_title": _entity_title,
                "details": _details,
                "ip_address": _ip_address,
                "created_at": chrono::Utc::now().to_rfc3339(),
            },
            None,
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
        let coll = self.db.collection::<Document>("audit_log");
        let mut filter = doc! {};
        if let Some(a) = action_filter {
            filter.insert("action", a);
        }
        if let Some(e) = entity_filter {
            filter.insert("entity_type", e);
        }
        if let Some(u) = user_filter {
            filter.insert("user_id", u);
        }
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_audit(&d))
            .collect()
    }
    fn audit_count(
        &self,
        action_filter: Option<&str>,
        entity_filter: Option<&str>,
        user_filter: Option<i64>,
    ) -> i64 {
        let coll = self.db.collection::<Document>("audit_log");
        let mut filter = doc! {};
        if let Some(a) = action_filter {
            filter.insert("action", a);
        }
        if let Some(e) = entity_filter {
            filter.insert("entity_type", e);
        }
        if let Some(u) = user_filter {
            filter.insert("user_id", u);
        }
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn audit_distinct_actions(&self) -> Vec<String> {
        let coll = self.db.collection::<Document>("audit_log");
        coll.distinct("action", doc! {}, None)
            .map(|vals| {
                vals.into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn audit_distinct_entity_types(&self) -> Vec<String> {
        let coll = self.db.collection::<Document>("audit_log");
        coll.distinct("entity_type", doc! {}, None)
            .map(|vals| {
                vals.into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn audit_cleanup(&self, max_age_days: i64) -> Result<usize, String> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(max_age_days)).to_rfc3339();
        let coll = self.db.collection::<Document>("audit_log");
        let result = coll
            .delete_many(doc! { "created_at": { "$lt": &cutoff } }, None)
            .map_err(|e| e.to_string())?;
        Ok(result.deleted_count as usize)
    }

    fn fw_is_banned(&self, ip: &str) -> bool {
        let coll = self.db.collection::<Document>("fw_bans");
        let now = chrono::Utc::now().to_rfc3339();
        // Active ban: active=true AND (no expiry OR expiry > now)
        let filter = doc! {
            "ip": ip,
            "active": true,
            "$or": [
                { "expires_at": Bson::Null },
                { "expires_at": { "$gt": &now } },
            ],
        };
        coll.count_documents(filter, None).unwrap_or(0) > 0
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
        let id = self.next_id("fw_bans")?;
        let coll = self.db.collection::<Document>("fw_bans");
        coll.insert_one(
            doc! {
                "id": id,
                "ip": ip,
                "reason": reason,
                "detail": detail,
                "banned_at": chrono::Utc::now().to_rfc3339(),
                "expires_at": expires_at,
                "country": country,
                "user_agent": user_agent,
                "active": true,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
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
        let expires = parse_duration_to_expiry(duration);
        self.fw_ban_create(ip, reason, detail, Some(&expires), country, user_agent)
    }
    fn fw_unban(&self, ip: &str) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("fw_bans");
        let result = coll
            .update_many(
                doc! { "ip": ip, "active": true },
                doc! { "$set": { "active": false } },
                None,
            )
            .map_err(|e| e.to_string())?;
        Ok(result.modified_count as usize)
    }
    fn fw_unban_by_id(&self, id: i64) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("fw_bans");
        let result = coll
            .update_one(
                doc! { "id": id },
                doc! { "$set": { "active": false } },
                None,
            )
            .map_err(|e| e.to_string())?;
        Ok(result.modified_count as usize)
    }
    fn fw_active_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        let coll = self.db.collection::<Document>("fw_bans");
        let now = chrono::Utc::now().to_rfc3339();
        let filter = doc! {
            "active": true,
            "$or": [
                { "expires_at": Bson::Null },
                { "expires_at": { "$gt": &now } },
            ],
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "banned_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_fw_ban(&d))
            .collect()
    }
    fn fw_active_count(&self) -> i64 {
        let coll = self.db.collection::<Document>("fw_bans");
        let now = chrono::Utc::now().to_rfc3339();
        let filter = doc! {
            "active": true,
            "$or": [
                { "expires_at": Bson::Null },
                { "expires_at": { "$gt": &now } },
            ],
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn fw_all_bans(&self, limit: i64, offset: i64) -> Vec<FwBan> {
        let coll = self.db.collection::<Document>("fw_bans");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "banned_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_fw_ban(&d))
            .collect()
    }
    fn fw_expire_stale(&self) { /* no-op for now */
    }

    fn fw_event_log(
        &self,
        _ip: &str,
        _event_type: &str,
        _detail: Option<&str>,
        _country: Option<&str>,
        _user_agent: Option<&str>,
        _request_path: Option<&str>,
    ) {
        let coll = self.db.collection::<Document>("fw_events");
        let _ = coll.insert_one(
            doc! {
                "ip": _ip,
                "event_type": _event_type,
                "detail": _detail,
                "country": _country,
                "user_agent": _user_agent,
                "request_path": _request_path,
                "created_at": chrono::Utc::now().to_rfc3339(),
            },
            None,
        );
    }

    fn fw_event_recent(&self, event_type: Option<&str>, limit: i64, offset: i64) -> Vec<FwEvent> {
        let coll = self.db.collection::<Document>("fw_events");
        let filter = match event_type {
            Some(t) => doc! { "event_type": t },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(filter, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_fw_event(&d))
            .collect()
    }
    fn fw_event_count_all(&self, event_type: Option<&str>) -> i64 {
        let coll = self.db.collection::<Document>("fw_events");
        let filter = match event_type {
            Some(t) => doc! { "event_type": t },
            None => doc! {},
        };
        coll.count_documents(filter, None).unwrap_or(0) as i64
    }
    fn fw_event_count_since_hours(&self, hours: i64) -> i64 {
        let coll = self.db.collection::<Document>("fw_events");
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();
        coll.count_documents(doc! { "created_at": { "$gte": &cutoff } }, None)
            .unwrap_or(0) as i64
    }
    fn fw_event_count_for_ip_since(&self, ip: &str, event_type: &str, minutes: i64) -> i64 {
        let coll = self.db.collection::<Document>("fw_events");
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        coll.count_documents(
            doc! { "ip": ip, "event_type": event_type, "created_at": { "$gte": &cutoff } },
            None,
        )
        .unwrap_or(0) as i64
    }
    fn fw_event_top_ips(&self, limit: i64) -> Vec<(String, i64)> {
        let coll = self.db.collection::<Document>("fw_events");
        let pipeline = vec![
            doc! { "$group": { "_id": "$ip", "count": { "$sum": 1 } } },
            doc! { "$sort": { "count": -1 } },
            doc! { "$limit": limit },
        ];
        let cursor = match coll.aggregate(pipeline, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| {
                let ip = d.get_str("_id").ok()?.to_string();
                let count = d
                    .get_i32("count")
                    .map(|c| c as i64)
                    .or_else(|_| d.get_i64("count"))
                    .ok()?;
                Some((ip, count))
            })
            .collect()
    }
    fn fw_event_counts_by_type(&self) -> Vec<(String, i64)> {
        let coll = self.db.collection::<Document>("fw_events");
        let pipeline = vec![
            doc! { "$group": { "_id": "$event_type", "count": { "$sum": 1 } } },
            doc! { "$sort": { "count": -1 } },
        ];
        let cursor = match coll.aggregate(pipeline, None) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| {
                let et = d.get_str("_id").ok()?.to_string();
                let count = d
                    .get_i32("count")
                    .map(|c| c as i64)
                    .or_else(|_| d.get_i64("count"))
                    .ok()?;
                Some((et, count))
            })
            .collect()
    }

    fn analytics_record(
        &self,
        _path: &str,
        _ip_hash: &str,
        _country: Option<&str>,
        _city: Option<&str>,
        _referrer: Option<&str>,
        _user_agent: Option<&str>,
        _device_type: Option<&str>,
        _browser: Option<&str>,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("page_views");
        coll.insert_one(
            doc! {
                "path": _path,
                "ip_hash": _ip_hash,
                "country": _country,
                "city": _city,
                "referrer": _referrer,
                "user_agent": _user_agent,
                "device_type": _device_type,
                "browser": _browser,
                "created_at": chrono::Utc::now().to_rfc3339(),
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn analytics_overview(&self, _from: &str, _to: &str) -> OverviewStats {
        OverviewStats {
            total_views: 0,
            unique_visitors: 0,
            posts_count: 0,
            portfolio_count: 0,
            comments_pending: 0,
            total_likes: 0,
        }
    }
    fn analytics_flow_data(&self, _from: &str, _to: &str) -> Vec<FlowNode> {
        vec![]
    }
    fn analytics_geo_data(&self, _from: &str, _to: &str) -> Vec<CountEntry> {
        vec![]
    }
    fn analytics_stream_data(&self, _from: &str, _to: &str) -> Vec<StreamEntry> {
        vec![]
    }
    fn analytics_calendar_data(&self, _from: &str, _to: &str) -> Vec<DailyCount> {
        vec![]
    }
    fn analytics_top_portfolio(&self, _from: &str, _to: &str, _limit: i64) -> Vec<CountEntry> {
        vec![]
    }
    fn analytics_top_referrers(&self, _from: &str, _to: &str, _limit: i64) -> Vec<CountEntry> {
        vec![]
    }
    fn analytics_tag_relations(&self) -> Vec<TagRelation> {
        vec![]
    }

    fn order_find_by_id(&self, id: i64) -> Option<Order> {
        let coll = self.db.collection::<Document>("orders");
        let d = coll.find_one(doc! { "id": id }, None).ok()??;
        doc_to_order(&d)
    }
    fn order_find_by_provider_order_id(&self, provider_order_id: &str) -> Option<Order> {
        let coll = self.db.collection::<Document>("orders");
        let d = coll
            .find_one(doc! { "provider_order_id": provider_order_id }, None)
            .ok()??;
        doc_to_order(&d)
    }
    fn order_list(&self, limit: i64, offset: i64) -> Vec<Order> {
        let coll = self.db.collection::<Document>("orders");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_order(&d))
            .collect()
    }
    fn order_list_by_status(&self, status: &str, limit: i64, offset: i64) -> Vec<Order> {
        let coll = self.db.collection::<Document>("orders");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! { "status": status }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_order(&d))
            .collect()
    }
    fn order_list_by_email(&self, email: &str, limit: i64, offset: i64) -> Vec<Order> {
        let coll = self.db.collection::<Document>("orders");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .skip(offset as u64)
            .limit(limit)
            .build();
        let cursor = match coll.find(doc! { "buyer_email": email }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_order(&d))
            .collect()
    }
    fn order_list_by_portfolio(&self, portfolio_id: i64) -> Vec<Order> {
        let coll = self.db.collection::<Document>("orders");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .build();
        let cursor = match coll.find(doc! { "portfolio_id": portfolio_id }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_order(&d))
            .collect()
    }
    fn order_count(&self) -> i64 {
        let coll = self.db.collection::<Document>("orders");
        coll.count_documents(doc! {}, None).unwrap_or(0) as i64
    }
    fn order_count_by_status(&self, status: &str) -> i64 {
        let coll = self.db.collection::<Document>("orders");
        coll.count_documents(doc! { "status": status }, None)
            .unwrap_or(0) as i64
    }
    fn order_total_revenue(&self) -> f64 {
        let coll = self.db.collection::<Document>("orders");
        let pipeline = vec![
            doc! { "$match": { "status": "completed" } },
            doc! { "$group": { "_id": Bson::Null, "total": { "$sum": "$amount" } } },
        ];
        let cursor = match coll.aggregate(pipeline, None) {
            Ok(c) => c,
            Err(_) => return 0.0,
        };
        cursor
            .filter_map(|r| r.ok())
            .next()
            .and_then(|d| d.get_f64("total").ok())
            .unwrap_or(0.0)
    }
    fn order_revenue_by_period(&self, days: i64) -> f64 {
        let coll = self.db.collection::<Document>("orders");
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let pipeline = vec![
            doc! { "$match": { "status": "completed", "created_at": { "$gte": &cutoff } } },
            doc! { "$group": { "_id": Bson::Null, "total": { "$sum": "$amount" } } },
        ];
        let cursor = match coll.aggregate(pipeline, None) {
            Ok(c) => c,
            Err(_) => return 0.0,
        };
        cursor
            .filter_map(|r| r.ok())
            .next()
            .and_then(|d| d.get_f64("total").ok())
            .unwrap_or(0.0)
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
        let id = self.next_id("orders")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("orders");
        coll.insert_one(
            doc! {
                "id": id,
                "portfolio_id": portfolio_id,
                "buyer_email": buyer_email,
                "buyer_name": buyer_name,
                "amount": amount,
                "currency": currency,
                "provider": provider,
                "provider_order_id": provider_order_id,
                "status": status,
                "created_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn order_update_status(&self, id: i64, status: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("orders");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "status": status } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn order_update_provider_order_id(
        &self,
        id: i64,
        provider_order_id: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("orders");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "provider_order_id": provider_order_id } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn order_update_buyer_info(
        &self,
        id: i64,
        buyer_email: &str,
        buyer_name: &str,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("orders");
        coll.update_one(
            doc! { "id": id },
            doc! { "$set": { "buyer_email": buyer_email, "buyer_name": buyer_name } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn order_find_completed_by_email_and_portfolio(
        &self,
        email: &str,
        portfolio_id: i64,
    ) -> Option<Order> {
        let coll = self.db.collection::<Document>("orders");
        let opts = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let d = coll
            .find_one(
                doc! { "portfolio_id": portfolio_id, "buyer_email": email, "status": "completed" },
                opts,
            )
            .ok()??;
        doc_to_order(&d)
    }

    fn download_token_find_by_token(&self, token: &str) -> Option<DownloadToken> {
        let coll = self.db.collection::<Document>("download_tokens");
        let d = coll.find_one(doc! { "token": token }, None).ok()??;
        doc_to_download_token(&d)
    }
    fn download_token_find_by_order(&self, order_id: i64) -> Option<DownloadToken> {
        let coll = self.db.collection::<Document>("download_tokens");
        let d = coll.find_one(doc! { "order_id": order_id }, None).ok()??;
        doc_to_download_token(&d)
    }
    fn download_token_increment(&self, id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("download_tokens");
        coll.update_one(
            doc! { "id": id },
            doc! { "$inc": { "downloads_used": 1_i64 } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn download_token_create(
        &self,
        order_id: i64,
        token: &str,
        max_downloads: i64,
        expires_at: NaiveDateTime,
    ) -> Result<i64, String> {
        let id = self.next_id("download_tokens")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("download_tokens");
        coll.insert_one(
            doc! {
                "id": id,
                "order_id": order_id,
                "token": token,
                "downloads_used": 0_i64,
                "max_downloads": max_downloads,
                "expires_at": expires_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "created_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn license_find_by_order(&self, order_id: i64) -> Option<License> {
        let coll = self.db.collection::<Document>("licenses");
        let d = coll.find_one(doc! { "order_id": order_id }, None).ok()??;
        doc_to_license(&d)
    }
    fn license_find_by_key(&self, key: &str) -> Option<License> {
        let coll = self.db.collection::<Document>("licenses");
        let d = coll.find_one(doc! { "license_key": key }, None).ok()??;
        doc_to_license(&d)
    }
    fn license_create(&self, order_id: i64, license_key: &str) -> Result<i64, String> {
        let id = self.next_id("licenses")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("licenses");
        coll.insert_one(
            doc! { "id": id, "order_id": order_id, "license_key": license_key, "created_at": &now },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn passkey_list_for_user(&self, user_id: i64) -> Vec<UserPasskey> {
        let coll = self.db.collection::<Document>("passkeys");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": 1 })
            .build();
        let cursor = match coll.find(doc! { "user_id": user_id }, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_passkey(&d))
            .collect()
    }
    fn passkey_get_by_credential_id(&self, credential_id: &str) -> Option<UserPasskey> {
        let coll = self.db.collection::<Document>("passkeys");
        let d = coll
            .find_one(doc! { "credential_id": credential_id }, None)
            .ok()??;
        doc_to_passkey(&d)
    }
    fn passkey_count_for_user(&self, user_id: i64) -> i64 {
        let coll = self.db.collection::<Document>("passkeys");
        coll.count_documents(doc! { "user_id": user_id }, None)
            .unwrap_or(0) as i64
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
        let id = self.next_id("passkeys")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("passkeys");
        coll.insert_one(
            doc! {
                "id": id,
                "user_id": user_id,
                "credential_id": credential_id,
                "public_key": public_key,
                "sign_count": sign_count,
                "transports": transports,
                "name": name,
                "created_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn passkey_update_sign_count(
        &self,
        credential_id: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("passkeys");
        coll.update_one(
            doc! { "credential_id": credential_id },
            doc! { "$set": { "sign_count": sign_count } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn passkey_update_public_key(
        &self,
        credential_id: &str,
        public_key: &str,
        sign_count: i64,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("passkeys");
        coll.update_one(
            doc! { "credential_id": credential_id },
            doc! { "$set": { "public_key": public_key, "sign_count": sign_count } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn passkey_delete(&self, id: i64, user_id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("passkeys");
        coll.delete_one(doc! { "id": id, "user_id": user_id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn passkey_delete_all_for_user(&self, user_id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("passkeys");
        coll.delete_many(doc! { "user_id": user_id }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn import_list(&self) -> Vec<Import> {
        let coll = self.db.collection::<Document>("imports");
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "id": -1 })
            .build();
        let cursor = match coll.find(doc! {}, opts) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        cursor
            .filter_map(|r| r.ok())
            .filter_map(|d| doc_to_import(&d))
            .collect()
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
        let id = self.next_id("imports")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("imports");
        coll.insert_one(
            doc! {
                "id": id,
                "source": source,
                "filename": filename,
                "posts_count": posts_count,
                "portfolio_count": portfolio_count,
                "comments_count": comments_count,
                "skipped_count": skipped_count,
                "log": log,
                "imported_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn search_create_fts_table(&self) -> Result<(), String> {
        Ok(())
    }
    fn search_rebuild_index(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn search_upsert_item(
        &self,
        _item_type: &str,
        _item_id: i64,
        _title: &str,
        _html_body: &str,
        _slug: &str,
        _image: Option<&str>,
        _date: Option<&str>,
        _is_published: bool,
    ) {
    }
    fn search_remove_item(&self, _item_type: &str, _item_id: i64) {}
    fn search_query(&self, _query: &str, _limit: i64) -> Vec<SearchResult> {
        vec![]
    }

    fn session_create(&self, user_id: i64, token: &str, expires_at: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("sessions");
        coll.insert_one(
            doc! {
                "token": token,
                "user_id": user_id,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "expires_at": expires_at,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_get_user_id(&self, token: &str) -> Option<i64> {
        let coll = self.db.collection::<Document>("sessions");
        let now = chrono::Utc::now().to_rfc3339();
        let doc = coll
            .find_one(doc! { "token": token, "expires_at": { "$gt": &now } }, None)
            .ok()??;
        doc.get_i64("user_id").ok()
    }

    fn session_delete(&self, token: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("sessions");
        coll.delete_one(doc! { "token": token }, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_delete_for_user(&self, user_id: i64) -> Result<(), String> {
        let coll = self.db.collection::<Document>("sessions");
        coll.delete_many(doc! { "user_id": user_id }, None)
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
        let coll = self.db.collection::<Document>("sessions");
        coll.insert_one(
            doc! {
                "token": token,
                "user_id": user_id,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "expires_at": expires_at,
                "ip_address": ip,
                "user_agent": user_agent,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn session_cleanup_expired(&self) {
        let coll = self.db.collection::<Document>("sessions");
        let now = chrono::Utc::now().to_rfc3339();
        let _ = coll.delete_many(doc! { "expires_at": { "$lte": &now } }, None);
    }

    fn session_count_recent_by_ip(&self, ip_hash: &str, minutes: i64) -> i64 {
        let coll = self.db.collection::<Document>("sessions");
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        coll.count_documents(
            doc! { "ip_address": ip_hash, "created_at": { "$gt": &cutoff } },
            None,
        )
        .unwrap_or(0) as i64
    }

    fn magic_link_create(
        &self,
        token: &str,
        email: &str,
        expires_minutes: i64,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("magic_links");
        let now = chrono::Utc::now().to_rfc3339();
        let expires =
            (chrono::Utc::now() + chrono::Duration::minutes(expires_minutes)).to_rfc3339();
        coll.insert_one(
            doc! {
                "token": token,
                "email": email,
                "created_at": &now,
                "expires_at": &expires,
                "used": false,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn magic_link_verify(&self, token: &str) -> Result<String, String> {
        let coll = self.db.collection::<Document>("magic_links");
        let now = chrono::Utc::now().to_rfc3339();
        let d = coll
            .find_one(doc! { "token": token, "expires_at": { "$gt": &now } }, None)
            .map_err(|e| e.to_string())?
            .ok_or("Invalid or expired link")?;
        let used = d.get_bool("used").unwrap_or(false);
        if used {
            return Err("This link has already been used".into());
        }
        let email = d.get_str("email").map_err(|e| e.to_string())?.to_string();
        coll.update_one(
            doc! { "token": token },
            doc! { "$set": { "used": true } },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(email)
    }

    fn like_exists(&self, portfolio_id: i64, ip_hash: &str) -> bool {
        let coll = self.db.collection::<Document>("likes");
        coll.count_documents(
            doc! { "portfolio_id": portfolio_id, "ip_hash": ip_hash },
            None,
        )
        .unwrap_or(0)
            > 0
    }

    fn like_add(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("likes");
        let opts = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        coll.update_one(
            doc! { "portfolio_id": portfolio_id, "ip_hash": ip_hash },
            doc! { "$setOnInsert": { "portfolio_id": portfolio_id, "ip_hash": ip_hash, "created_at": chrono::Utc::now().to_rfc3339() } },
            opts,
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn like_remove(&self, portfolio_id: i64, ip_hash: &str) -> Result<(), String> {
        let coll = self.db.collection::<Document>("likes");
        coll.delete_one(
            doc! { "portfolio_id": portfolio_id, "ip_hash": ip_hash },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn analytics_prune(&self, before_date: &str) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("page_views");
        let result = coll
            .delete_many(doc! { "created_at": { "$lt": before_date } }, None)
            .map_err(|e| e.to_string())?;
        Ok(result.deleted_count as usize)
    }

    fn analytics_count(&self) -> i64 {
        let coll = self.db.collection::<Document>("page_views");
        coll.count_documents(doc! {}, None).unwrap_or(0) as i64
    }

    // ── Health / maintenance ───────────────────────────────────────────

    fn db_backend(&self) -> &str {
        "mongodb"
    }

    fn health_content_stats(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) {
        let count = |coll_name: &str, filter: Document| -> u64 {
            self.db
                .collection::<Document>(coll_name)
                .count_documents(filter, None)
                .unwrap_or(0)
        };
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        (
            count("posts", doc! {}),
            count("posts", doc! { "status": "published" }),
            count("posts", doc! { "status": "draft" }),
            count("portfolio", doc! {}),
            count("comments", doc! {}),
            count("comments", doc! { "status": "pending" }),
            count("categories", doc! {}),
            count("tags", doc! {}),
            count("sessions", doc! {}),
            count("sessions", doc! { "expires_at": { "$lt": &now } }),
        )
    }

    fn health_referenced_files(&self) -> std::collections::HashSet<String> {
        let mut referenced = std::collections::HashSet::new();

        // Post featured images
        let posts = self.db.collection::<Document>("posts");
        if let Ok(cursor) = posts.find(doc! { "featured_image": { "$ne": "" } }, None) {
            for d in cursor.flatten() {
                if let Ok(img) = d.get_str("featured_image") {
                    if let Some(name) = img.split('/').next_back() {
                        referenced.insert(name.to_string());
                    }
                }
                // Also check content_html for upload refs
                if let Ok(body) = d.get_str("content_html") {
                    crate::health::extract_upload_refs(body, &mut referenced);
                }
            }
        }

        // Portfolio images + descriptions
        let portfolio = self.db.collection::<Document>("portfolio");
        if let Ok(cursor) = portfolio.find(doc! {}, None) {
            for d in cursor.flatten() {
                if let Ok(img) = d.get_str("image_path") {
                    if !img.is_empty() {
                        if let Some(name) = img.split('/').next_back() {
                            referenced.insert(name.to_string());
                        }
                    }
                }
                if let Ok(desc) = d.get_str("description_html") {
                    crate::health::extract_upload_refs(desc, &mut referenced);
                }
            }
        }

        // Settings referencing uploads
        let settings = self.db.collection::<Document>("settings");
        if let Ok(cursor) = settings.find(doc! { "value": { "$regex": "/uploads/" } }, None) {
            for d in cursor.flatten() {
                if let Ok(val) = d.get_str("value") {
                    if let Some(name) = val.split('/').next_back() {
                        referenced.insert(name.to_string());
                    }
                }
            }
        }

        referenced
    }

    fn health_session_cleanup(&self) -> Result<u64, String> {
        let coll = self.db.collection::<Document>("sessions");
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let expired = coll
            .count_documents(doc! { "expires_at": { "$lt": &now } }, None)
            .unwrap_or(0);
        coll.delete_many(doc! { "expires_at": { "$lt": &now } }, None)
            .map_err(|e| e.to_string())?;
        Ok(expired)
    }

    fn health_unused_tags_cleanup(&self) -> Result<u64, String> {
        // Get all tag IDs referenced in content_tags junction
        let junc = self.db.collection::<Document>("content_tags");
        let mut used_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        if let Ok(cursor) = junc.find(doc! {}, None) {
            for d in cursor.flatten() {
                if let Ok(tid) = d.get_i64("tag_id") {
                    used_ids.insert(tid);
                }
            }
        }

        let tags_coll = self.db.collection::<Document>("tags");
        let all_tags: Vec<i64> = tags_coll
            .find(doc! {}, None)
            .map(|cursor| {
                cursor
                    .flatten()
                    .filter_map(|d| d.get_i64("id").ok())
                    .collect()
            })
            .unwrap_or_default();

        let unused: Vec<i64> = all_tags
            .into_iter()
            .filter(|id| !used_ids.contains(id))
            .collect();
        let count = unused.len() as u64;
        if count == 0 {
            return Ok(0);
        }
        for id in &unused {
            let _ = tags_coll.delete_one(doc! { "id": id }, None);
        }
        Ok(count)
    }

    fn health_analytics_prune(&self, days: u64) -> Result<u64, String> {
        let coll = self.db.collection::<Document>("analytics_events");
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let count = coll
            .count_documents(doc! { "created_at": { "$lt": &cutoff } }, None)
            .unwrap_or(0);
        coll.delete_many(doc! { "created_at": { "$lt": &cutoff } }, None)
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn health_export_content(&self) -> Result<serde_json::Value, String> {
        let mut export = serde_json::Map::new();

        // Helper to convert BSON docs to JSON values
        let to_json_array = |coll_name: &str| -> Vec<serde_json::Value> {
            let coll = self.db.collection::<Document>(coll_name);
            coll.find(doc! {}, None)
                .map(|cursor| {
                    cursor
                        .flatten()
                        .filter_map(|d| {
                            let json_str = serde_json::to_string(&d).ok()?;
                            serde_json::from_str(&json_str).ok()
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        export.insert(
            "posts".to_string(),
            serde_json::Value::Array(to_json_array("posts")),
        );
        export.insert(
            "portfolio".to_string(),
            serde_json::Value::Array(to_json_array("portfolio")),
        );
        export.insert(
            "categories".to_string(),
            serde_json::Value::Array(to_json_array("categories")),
        );
        export.insert(
            "tags".to_string(),
            serde_json::Value::Array(to_json_array("tags")),
        );
        export.insert(
            "comments".to_string(),
            serde_json::Value::Array(to_json_array("comments")),
        );
        export.insert(
            "content_tags".to_string(),
            serde_json::Value::Array(to_json_array("content_tags")),
        );
        export.insert(
            "content_categories".to_string(),
            serde_json::Value::Array(to_json_array("content_categories")),
        );
        export.insert(
            "settings".to_string(),
            serde_json::Value::Array(to_json_array("settings")),
        );

        Ok(serde_json::Value::Object(export))
    }

    fn magic_link_cleanup(&self) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("magic_links");
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let filter = doc! {
            "$or": [
                { "expires_at": { "$lt": &now } },
                { "used": true }
            ]
        };
        let count = coll.count_documents(filter.clone(), None).unwrap_or(0) as usize;
        coll.delete_many(filter, None).map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn task_publish_scheduled(&self) -> Result<usize, String> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let filter = doc! {
            "status": "scheduled",
            "published_at": { "$lte": &now }
        };
        let update = doc! {
            "$set": { "status": "published", "updated_at": &now }
        };
        let mut total = 0usize;
        for coll_name in &["posts", "portfolio"] {
            let coll = self.db.collection::<Document>(coll_name);
            if let Ok(result) = coll.update_many(filter.clone(), update.clone(), None) {
                total += result.modified_count as usize;
            }
        }
        Ok(total)
    }

    fn task_cleanup_sessions(&self, max_age_days: i64) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("sessions");
        let now = chrono::Utc::now();
        let now_str = now.format("%Y-%m-%dT%H:%M:%S").to_string();
        let cutoff = (now - chrono::Duration::days(max_age_days))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let filter = doc! {
            "$or": [
                { "expires_at": { "$lt": &now_str } },
                { "created_at": { "$lt": &cutoff } }
            ]
        };
        let count = coll.count_documents(filter.clone(), None).unwrap_or(0) as usize;
        coll.delete_many(filter, None).map_err(|e| e.to_string())?;
        Ok(count)
    }

    fn task_cleanup_analytics(&self, max_age_days: i64) -> Result<usize, String> {
        let coll = self.db.collection::<Document>("page_views");
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(max_age_days))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let filter = doc! { "created_at": { "$lt": &cutoff } };
        let count = coll.count_documents(filter.clone(), None).unwrap_or(0) as usize;
        coll.delete_many(filter, None).map_err(|e| e.to_string())?;
        Ok(count)
    }

    // ── Email queue (built-in MTA) ──────────────────────────────────

    fn mta_queue_push(
        &self,
        to: &str,
        from: &str,
        subject: &str,
        body: &str,
    ) -> Result<i64, String> {
        let id = self.next_id("email_queue")?;
        let now = chrono::Utc::now().to_rfc3339();
        let coll = self.db.collection::<Document>("email_queue");
        coll.insert_one(
            doc! {
                "id": id,
                "to_addr": to,
                "from_addr": from,
                "subject": subject,
                "body_text": body,
                "attempts": 0_i64,
                "max_attempts": 5_i64,
                "next_retry_at": &now,
                "status": "pending",
                "error": "",
                "created_at": &now,
            },
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn mta_queue_pending(&self, limit: i64) -> Vec<crate::mta::queue::QueuedEmail> {
        let coll = self.db.collection::<Document>("email_queue");
        let now = chrono::Utc::now().to_rfc3339();
        let filter = doc! {
            "status": "pending",
            "next_retry_at": { "$lte": &now },
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "next_retry_at": 1 })
            .limit(Some(limit))
            .build();
        let cursor = match coll.find(filter, Some(opts)) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        cursor
            .flatten()
            .filter_map(|d| {
                Some(crate::mta::queue::QueuedEmail {
                    id: d.get_i64("id").ok()?,
                    to_addr: d.get_str("to_addr").ok()?.to_string(),
                    from_addr: d.get_str("from_addr").ok()?.to_string(),
                    subject: d.get_str("subject").ok()?.to_string(),
                    body_text: d.get_str("body_text").ok()?.to_string(),
                    attempts: d.get_i64("attempts").ok().unwrap_or(0),
                    max_attempts: d.get_i64("max_attempts").ok().unwrap_or(5),
                    next_retry_at: d.get_str("next_retry_at").ok().unwrap_or("").to_string(),
                    status: d.get_str("status").ok().unwrap_or("pending").to_string(),
                    error: d.get_str("error").ok().unwrap_or("").to_string(),
                    created_at: d.get_str("created_at").ok().unwrap_or("").to_string(),
                })
            })
            .collect()
    }

    fn mta_queue_update_status(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
        next_retry: Option<&str>,
    ) -> Result<(), String> {
        let coll = self.db.collection::<Document>("email_queue");
        let mut update = doc! { "$set": { "status": status } };
        if let Some(e) = error {
            update.get_document_mut("$set").unwrap().insert("error", e);
        }
        if let Some(nr) = next_retry {
            update
                .get_document_mut("$set")
                .unwrap()
                .insert("next_retry_at", nr);
        }
        if status == "sending" {
            update.insert("$inc", doc! { "attempts": 1_i64 });
        }
        coll.update_one(doc! { "id": id }, update, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn mta_queue_sent_last_hour(&self) -> Result<u64, String> {
        let coll = self.db.collection::<Document>("email_queue");
        let one_hour_ago = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let filter = doc! {
            "status": "sent",
            "created_at": { "$gte": &one_hour_ago },
        };
        coll.count_documents(filter, None)
            .map_err(|e| e.to_string())
    }

    fn mta_queue_stats(&self) -> (u64, u64, u64, u64) {
        let coll = self.db.collection::<Document>("email_queue");
        let sent = coll
            .count_documents(doc! { "status": "sent" }, None)
            .unwrap_or(0);
        let pending = coll
            .count_documents(doc! { "status": { "$in": ["pending", "sending"] } }, None)
            .unwrap_or(0);
        let failed = coll
            .count_documents(doc! { "status": "failed" }, None)
            .unwrap_or(0);
        let total = coll.count_documents(doc! {}, None).unwrap_or(0);
        (sent, pending, failed, total)
    }

    fn mta_queue_cleanup(&self, days: u64) -> Result<u64, String> {
        let coll = self.db.collection::<Document>("email_queue");
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let filter = doc! { "created_at": { "$lt": &cutoff } };
        let result = coll.delete_many(filter, None).map_err(|e| e.to_string())?;
        Ok(result.deleted_count)
    }

    fn raw_execute(&self, _sql: &str) -> Result<usize, String> {
        Err("raw_execute not supported on MongoDB".to_string())
    }

    fn raw_query_i64(&self, _sql: &str) -> Result<i64, String> {
        Err("raw_query_i64 not supported on MongoDB".to_string())
    }
}

// ── Helper: Convert BSON Document to User ───────────────────────────

fn doc_to_user(doc: &Document) -> Option<User> {
    Some(User {
        id: doc.get_i64("id").ok()?,
        email: doc.get_str("email").ok()?.to_string(),
        password_hash: doc.get_str("password_hash").ok()?.to_string(),
        display_name: doc.get_str("display_name").ok().unwrap_or("").to_string(),
        role: doc.get_str("role").ok().unwrap_or("subscriber").to_string(),
        status: doc.get_str("status").ok().unwrap_or("active").to_string(),
        avatar: doc.get_str("avatar").ok().unwrap_or("").to_string(),
        mfa_enabled: doc.get_bool("mfa_enabled").unwrap_or(false),
        mfa_secret: doc.get_str("mfa_secret").ok().unwrap_or("").to_string(),
        mfa_recovery_codes: doc
            .get_str("mfa_recovery_codes")
            .ok()
            .unwrap_or("[]")
            .to_string(),
        last_login_at: doc.get_str("last_login_at").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").ok().unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").ok().unwrap_or("").to_string(),
        auth_method: doc
            .get_str("auth_method")
            .ok()
            .unwrap_or("password")
            .to_string(),
        auth_method_fallback: doc
            .get_str("auth_method_fallback")
            .ok()
            .unwrap_or("password")
            .to_string(),
    })
}

// ── Helper: Convert BSON Document to Post ────────────────────────────

fn parse_naive_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn parse_naive_dt_rfc3339(s: &str) -> Option<NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.naive_utc())
        .ok()
        .or_else(|| parse_naive_dt(s))
}

fn doc_to_post(doc: &Document) -> Option<Post> {
    Some(Post {
        id: doc.get_i64("id").ok()?,
        title: doc.get_str("title").ok()?.to_string(),
        slug: doc.get_str("slug").ok()?.to_string(),
        content_json: doc.get_str("content_json").ok().unwrap_or("").to_string(),
        content_html: doc.get_str("content_html").ok().unwrap_or("").to_string(),
        excerpt: doc.get_str("excerpt").ok().map(|s| s.to_string()),
        featured_image: doc.get_str("featured_image").ok().map(|s| s.to_string()),
        meta_title: doc.get_str("meta_title").ok().map(|s| s.to_string()),
        meta_description: doc.get_str("meta_description").ok().map(|s| s.to_string()),
        status: doc.get_str("status").ok().unwrap_or("draft").to_string(),
        published_at: doc
            .get_str("published_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        updated_at: doc
            .get_str("updated_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        seo_score: doc.get_i32("seo_score").unwrap_or(-1),
        seo_issues: doc.get_str("seo_issues").ok().unwrap_or("[]").to_string(),
    })
}

// ── Helper: Convert BSON Document to PortfolioItem ───────────────────

fn doc_to_portfolio(doc: &Document) -> Option<PortfolioItem> {
    Some(PortfolioItem {
        id: doc.get_i64("id").ok()?,
        title: doc.get_str("title").ok()?.to_string(),
        slug: doc.get_str("slug").ok()?.to_string(),
        description_json: doc.get_str("description_json").ok().map(|s| s.to_string()),
        description_html: doc.get_str("description_html").ok().map(|s| s.to_string()),
        image_path: doc.get_str("image_path").ok().unwrap_or("").to_string(),
        thumbnail_path: doc.get_str("thumbnail_path").ok().map(|s| s.to_string()),
        meta_title: doc.get_str("meta_title").ok().map(|s| s.to_string()),
        meta_description: doc.get_str("meta_description").ok().map(|s| s.to_string()),
        sell_enabled: doc.get_bool("sell_enabled").unwrap_or(false),
        price: doc.get_f64("price").ok(),
        purchase_note: doc.get_str("purchase_note").ok().unwrap_or("").to_string(),
        payment_provider: doc
            .get_str("payment_provider")
            .ok()
            .unwrap_or("")
            .to_string(),
        download_file_path: doc
            .get_str("download_file_path")
            .ok()
            .unwrap_or("")
            .to_string(),
        likes: doc.get_i64("likes").unwrap_or(0),
        status: doc.get_str("status").ok().unwrap_or("draft").to_string(),
        published_at: doc
            .get_str("published_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        updated_at: doc
            .get_str("updated_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        seo_score: doc.get_i32("seo_score").unwrap_or(-1),
        seo_issues: doc.get_str("seo_issues").ok().unwrap_or("[]").to_string(),
    })
}

// ── Helper: Convert BSON Document to Comment ─────────────────────────

fn doc_to_comment(doc: &Document) -> Option<Comment> {
    Some(Comment {
        id: doc.get_i64("id").ok()?,
        post_id: doc.get_i64("post_id").ok()?,
        content_type: doc
            .get_str("content_type")
            .ok()
            .unwrap_or("post")
            .to_string(),
        author_name: doc.get_str("author_name").ok().unwrap_or("").to_string(),
        author_email: doc.get_str("author_email").ok().map(|s| s.to_string()),
        body: doc.get_str("body").ok().unwrap_or("").to_string(),
        status: doc.get_str("status").ok().unwrap_or("pending").to_string(),
        parent_id: doc.get_i64("parent_id").ok(),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to Category ────────────────────────

fn doc_to_category(doc: &Document) -> Option<Category> {
    Some(Category {
        id: doc.get_i64("id").ok()?,
        name: doc.get_str("name").ok()?.to_string(),
        slug: doc.get_str("slug").ok()?.to_string(),
        r#type: doc.get_str("type").ok().unwrap_or("blog").to_string(),
        show_in_nav: doc.get_bool("show_in_nav").unwrap_or(true),
    })
}

// ── Helper: Convert BSON Document to Tag ─────────────────────────────

fn doc_to_tag(doc: &Document) -> Option<Tag> {
    Some(Tag {
        id: doc.get_i64("id").ok()?,
        name: doc.get_str("name").ok()?.to_string(),
        slug: doc.get_str("slug").ok()?.to_string(),
    })
}

// ── Helper: Convert BSON Document to Design ──────────────────────────

fn doc_to_design(doc: &Document) -> Option<Design> {
    Some(Design {
        id: doc.get_i64("id").ok()?,
        name: doc.get_str("name").ok()?.to_string(),
        slug: doc.get_str("slug").ok().unwrap_or("").to_string(),
        description: doc.get_str("description").ok().unwrap_or("").to_string(),
        layout_html: doc.get_str("layout_html").ok().unwrap_or("").to_string(),
        style_css: doc.get_str("style_css").ok().unwrap_or("").to_string(),
        thumbnail_path: doc.get_str("thumbnail_path").ok().map(|s| s.to_string()),
        is_active: doc.get_bool("is_active").unwrap_or(false),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        updated_at: doc
            .get_str("updated_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to DesignTemplate ──────────────────

fn doc_to_design_template(doc: &Document) -> Option<DesignTemplate> {
    Some(DesignTemplate {
        id: doc.get_i64("id").ok()?,
        design_id: doc.get_i64("design_id").ok()?,
        template_type: doc.get_str("template_type").ok()?.to_string(),
        layout_html: doc.get_str("layout_html").ok().unwrap_or("").to_string(),
        style_css: doc.get_str("style_css").ok().unwrap_or("").to_string(),
        grapesjs_data: doc.get_str("grapesjs_data").ok().unwrap_or("").to_string(),
        updated_at: doc
            .get_str("updated_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to AuditEntry ──────────────────────

fn doc_to_audit(doc: &Document) -> Option<AuditEntry> {
    Some(AuditEntry {
        id: doc.get_i64("id").unwrap_or(0),
        user_id: doc.get_i64("user_id").ok(),
        user_name: doc.get_str("user_name").ok().map(|s| s.to_string()),
        action: doc.get_str("action").ok()?.to_string(),
        entity_type: doc.get_str("entity_type").ok().map(|s| s.to_string()),
        entity_id: doc.get_i64("entity_id").ok(),
        entity_title: doc.get_str("entity_title").ok().map(|s| s.to_string()),
        details: doc.get_str("details").ok().map(|s| s.to_string()),
        ip_address: doc.get_str("ip_address").ok().map(|s| s.to_string()),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to FwBan ───────────────────────────

fn doc_to_fw_ban(doc: &Document) -> Option<FwBan> {
    Some(FwBan {
        id: doc.get_i64("id").ok()?,
        ip: doc.get_str("ip").ok()?.to_string(),
        reason: doc.get_str("reason").ok().unwrap_or("").to_string(),
        detail: doc.get_str("detail").ok().map(|s| s.to_string()),
        banned_at: doc.get_str("banned_at").ok().unwrap_or("").to_string(),
        expires_at: doc.get_str("expires_at").ok().map(|s| s.to_string()),
        country: doc.get_str("country").ok().map(|s| s.to_string()),
        user_agent: doc.get_str("user_agent").ok().map(|s| s.to_string()),
        active: doc.get_bool("active").unwrap_or(false),
    })
}

// ── Helper: Convert BSON Document to FwEvent ─────────────────────────

fn doc_to_fw_event(doc: &Document) -> Option<FwEvent> {
    Some(FwEvent {
        id: doc.get_i64("id").unwrap_or(0),
        ip: doc.get_str("ip").ok()?.to_string(),
        event_type: doc.get_str("event_type").ok()?.to_string(),
        detail: doc.get_str("detail").ok().map(|s| s.to_string()),
        country: doc.get_str("country").ok().map(|s| s.to_string()),
        user_agent: doc.get_str("user_agent").ok().map(|s| s.to_string()),
        request_path: doc.get_str("request_path").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").ok().unwrap_or("").to_string(),
    })
}

// ── Helper: Convert BSON Document to Order ───────────────────────────

fn doc_to_order(doc: &Document) -> Option<Order> {
    Some(Order {
        id: doc.get_i64("id").ok()?,
        portfolio_id: doc.get_i64("portfolio_id").ok()?,
        buyer_email: doc.get_str("buyer_email").ok()?.to_string(),
        buyer_name: doc.get_str("buyer_name").ok().unwrap_or("").to_string(),
        amount: doc.get_f64("amount").ok()?,
        currency: doc.get_str("currency").ok().unwrap_or("USD").to_string(),
        provider: doc.get_str("provider").ok().unwrap_or("").to_string(),
        provider_order_id: doc
            .get_str("provider_order_id")
            .ok()
            .unwrap_or("")
            .to_string(),
        status: doc.get_str("status").ok().unwrap_or("pending").to_string(),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to DownloadToken ───────────────────

fn doc_to_download_token(doc: &Document) -> Option<DownloadToken> {
    Some(DownloadToken {
        id: doc.get_i64("id").ok()?,
        order_id: doc.get_i64("order_id").ok()?,
        token: doc.get_str("token").ok()?.to_string(),
        downloads_used: doc.get_i64("downloads_used").unwrap_or(0),
        max_downloads: doc.get_i64("max_downloads").unwrap_or(0),
        expires_at: doc
            .get_str("expires_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to License ─────────────────────────

fn doc_to_license(doc: &Document) -> Option<License> {
    Some(License {
        id: doc.get_i64("id").ok()?,
        order_id: doc.get_i64("order_id").ok()?,
        license_key: doc.get_str("license_key").ok()?.to_string(),
        created_at: doc
            .get_str("created_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Convert BSON Document to UserPasskey ─────────────────────

fn doc_to_passkey(doc: &Document) -> Option<UserPasskey> {
    Some(UserPasskey {
        id: doc.get_i64("id").ok()?,
        user_id: doc.get_i64("user_id").ok()?,
        credential_id: doc.get_str("credential_id").ok()?.to_string(),
        public_key: doc.get_str("public_key").ok()?.to_string(),
        sign_count: doc.get_i64("sign_count").unwrap_or(0),
        transports: doc.get_str("transports").ok().unwrap_or("[]").to_string(),
        name: doc.get_str("name").ok().unwrap_or("").to_string(),
        created_at: doc.get_str("created_at").ok().unwrap_or("").to_string(),
    })
}

// ── Helper: Convert BSON Document to Import ──────────────────────────

fn doc_to_import(doc: &Document) -> Option<Import> {
    Some(Import {
        id: doc.get_i64("id").ok()?,
        source: doc.get_str("source").ok()?.to_string(),
        filename: doc.get_str("filename").ok().map(|s| s.to_string()),
        posts_count: doc.get_i64("posts_count").unwrap_or(0),
        portfolio_count: doc.get_i64("portfolio_count").unwrap_or(0),
        comments_count: doc.get_i64("comments_count").unwrap_or(0),
        skipped_count: doc.get_i64("skipped_count").unwrap_or(0),
        log: doc.get_str("log").ok().map(|s| s.to_string()),
        imported_at: doc
            .get_str("imported_at")
            .ok()
            .and_then(parse_naive_dt_rfc3339)?,
    })
}

// ── Helper: Parse duration string (e.g. "24h", "7d", "30m") to expiry datetime ──

fn parse_duration_to_expiry(duration: &str) -> String {
    let s = duration.trim();
    let (num_str, unit) = s.split_at(s.len().saturating_sub(1));
    let num: i64 = num_str.parse().unwrap_or(24);
    let delta = match unit {
        "m" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        _ => chrono::Duration::hours(num),
    };
    (chrono::Utc::now() + delta).to_rfc3339()
}
