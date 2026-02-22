use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

use crate::db::DbPool;

/// In-memory settings cache. Loaded once at startup, refreshed on settings save.
/// All public route dispatching reads from this — zero DB overhead per request.
pub struct SettingsCache {
    inner: RwLock<HashMap<String, String>>,
}

impl SettingsCache {
    pub fn load(pool: &DbPool) -> Self {
        Self {
            inner: RwLock::new(Setting::all(pool)),
        }
    }

    pub fn refresh(&self, pool: &DbPool) {
        if let Ok(mut w) = self.inner.write() {
            *w = Setting::all(pool);
        }
    }

    pub fn load_from_store(store: &dyn crate::store::Store) -> Self {
        Self {
            inner: RwLock::new(store.setting_all()),
        }
    }

    pub fn refresh_from_store(&self, store: &dyn crate::store::Store) {
        if let Ok(mut w) = self.inner.write() {
            *w = store.setting_all();
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.inner.read().ok()?.get(key).cloned()
    }

    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or_else(|| default.to_string())
    }

    pub fn all(&self) -> HashMap<String, String> {
        self.inner
            .read()
            .ok()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

impl Setting {
    pub fn get(pool: &DbPool, key: &str) -> Option<String> {
        let conn = pool.get().ok()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn get_or(pool: &DbPool, key: &str, default: &str) -> String {
        Self::get(pool, key).unwrap_or_else(|| default.to_string())
    }

    pub fn get_bool(pool: &DbPool, key: &str) -> bool {
        Self::get(pool, key)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }

    pub fn get_i64(pool: &DbPool, key: &str) -> i64 {
        Self::get(pool, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn get_f64(pool: &DbPool, key: &str) -> f64 {
        Self::get(pool, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    pub fn set(pool: &DbPool, key: &str, value: &str) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_many(pool: &DbPool, settings: &HashMap<String, String>) -> Result<(), String> {
        let conn = pool.get().map_err(|e| e.to_string())?;
        for (key, value) in settings {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_group(pool: &DbPool, prefix: &str) -> HashMap<String, String> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let pattern = format!("{}%", prefix);
        let mut stmt = match conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1") {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };

        stmt.query_map(params![pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn all(pool: &DbPool) -> HashMap<String, String> {
        let conn = match pool.get() {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let mut stmt = match conn.prepare("SELECT key, value FROM settings") {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };

        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}
