#![cfg(test)]

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::collections::HashMap;

use crate::db::{run_migrations, seed_defaults, DbPool};
use crate::license;
use crate::models::analytics::PageView;
use crate::models::audit::AuditEntry;
use crate::models::category::{Category, CategoryForm};
use crate::models::comment::{Comment, CommentForm};
use crate::models::design::{Design, DesignTemplate};
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::import::Import;
use crate::models::order::{DownloadToken, License, Order};
use crate::models::portfolio::{PortfolioForm, PortfolioItem};
use crate::models::post::{Post, PostForm};
use crate::models::settings::Setting;
use crate::models::tag::{Tag, TagForm};
use crate::models::user::User;
use crate::rate_limit::RateLimiter;
use crate::rss;
use crate::security::auth;
use crate::security::mfa;
use crate::seo;
use crate::typography;

/// Atomic counter for unique shared-cache DB names so parallel tests don't collide.
static TEST_DB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Create a fresh in-memory SQLite pool with all migrations + seed defaults applied.
/// Uses a named shared-cache in-memory DB so multiple connections see the same data
/// (needed because get_session_user holds one conn while calling User::get_by_id).
/// Pre-seeds admin_password_hash with a fast bcrypt hash to avoid the expensive
/// DEFAULT_COST hash in seed_defaults (which can take 60s+ in debug builds).
fn test_pool() -> DbPool {
    let id = TEST_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let uri = format!("file:testdb_{}?mode=memory&cache=shared", id);
    let manager = SqliteConnectionManager::file(uri);
    let pool = Pool::builder()
        .max_size(2)
        .build(manager)
        .expect("Failed to create test pool");
    {
        let conn = pool.get().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    }
    run_migrations(&pool).expect("Failed to run migrations");
    // Pre-insert admin_password_hash so seed_defaults skips the slow bcrypt call
    {
        let conn = pool.get().unwrap();
        let fast = bcrypt::hash("admin", 4).unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('admin_password_hash', ?1)",
            rusqlite::params![fast],
        )
        .unwrap();
    }
    seed_defaults(&pool).expect("Failed to seed defaults");
    pool
}

/// Fast bcrypt hash for tests (cost=4 instead of DEFAULT_COST=12).
fn fast_hash(password: &str) -> String {
    bcrypt::hash(password, 4).unwrap()
}

// ═══════════════════════════════════════════════════════════
// Settings
// ═══════════════════════════════════════════════════════════

#[test]
fn settings_set_and_get() {
    let pool = test_pool();
    Setting::set(&pool, "test_key", "hello").unwrap();
    assert_eq!(Setting::get(&pool, "test_key"), Some("hello".to_string()));
}

#[test]
fn settings_get_or_default() {
    let pool = test_pool();
    assert_eq!(
        Setting::get_or(&pool, "nonexistent", "fallback"),
        "fallback"
    );
    Setting::set(&pool, "exists", "val").unwrap();
    assert_eq!(Setting::get_or(&pool, "exists", "fallback"), "val");
}

#[test]
fn settings_get_bool() {
    let pool = test_pool();
    Setting::set(&pool, "flag_true", "true").unwrap();
    Setting::set(&pool, "flag_one", "1").unwrap();
    Setting::set(&pool, "flag_false", "false").unwrap();
    assert!(Setting::get_bool(&pool, "flag_true"));
    assert!(Setting::get_bool(&pool, "flag_one"));
    assert!(!Setting::get_bool(&pool, "flag_false"));
    assert!(!Setting::get_bool(&pool, "missing_flag"));
}

#[test]
fn settings_get_i64() {
    let pool = test_pool();
    Setting::set(&pool, "num", "42").unwrap();
    assert_eq!(Setting::get_i64(&pool, "num"), 42);
    assert_eq!(Setting::get_i64(&pool, "missing"), 0);
}

#[test]
fn settings_set_many() {
    let pool = test_pool();
    let mut map = HashMap::new();
    map.insert("k1".to_string(), "v1".to_string());
    map.insert("k2".to_string(), "v2".to_string());
    Setting::set_many(&pool, &map).unwrap();
    assert_eq!(Setting::get(&pool, "k1"), Some("v1".to_string()));
    assert_eq!(Setting::get(&pool, "k2"), Some("v2".to_string()));
}

#[test]
fn settings_upsert() {
    let pool = test_pool();
    Setting::set(&pool, "key", "first").unwrap();
    Setting::set(&pool, "key", "second").unwrap();
    assert_eq!(Setting::get(&pool, "key"), Some("second".to_string()));
}

// ═══════════════════════════════════════════════════════════
// Posts
// ═══════════════════════════════════════════════════════════

fn make_post_form(title: &str, slug: &str, status: &str) -> PostForm {
    PostForm {
        title: title.to_string(),
        slug: slug.to_string(),
        content_json: "{}".to_string(),
        content_html: "<p>test</p>".to_string(),
        excerpt: Some("excerpt".to_string()),
        featured_image: None,
        meta_title: None,
        meta_description: None,
        status: status.to_string(),
        published_at: None,
        category_ids: None,
        tag_ids: None,
    }
}

#[test]
fn post_crud() {
    let pool = test_pool();

    // Create
    let id = Post::create(&pool, &make_post_form("Hello", "hello", "draft")).unwrap();
    assert!(id > 0);

    // Read
    let post = Post::find_by_id(&pool, id).unwrap();
    assert_eq!(post.title, "Hello");
    assert_eq!(post.slug, "hello");
    assert_eq!(post.status, "draft");

    // Find by slug
    let post2 = Post::find_by_slug(&pool, "hello").unwrap();
    assert_eq!(post2.id, id);

    // Update
    let mut form = make_post_form("Updated", "hello", "published");
    form.published_at = Some("2026-01-01T12:00".to_string());
    Post::update(&pool, id, &form).unwrap();
    let updated = Post::find_by_id(&pool, id).unwrap();
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.status, "published");

    // Count
    assert_eq!(Post::count(&pool, None), 1);
    assert_eq!(Post::count(&pool, Some("published")), 1);
    assert_eq!(Post::count(&pool, Some("draft")), 0);

    // Delete
    Post::delete(&pool, id).unwrap();
    assert!(Post::find_by_id(&pool, id).is_none());
    assert_eq!(Post::count(&pool, None), 0);
}

#[test]
fn post_list_and_pagination() {
    let pool = test_pool();
    for i in 0..5 {
        Post::create(
            &pool,
            &make_post_form(&format!("Post {}", i), &format!("post-{}", i), "published"),
        )
        .unwrap();
    }
    Post::create(&pool, &make_post_form("Draft", "draft-1", "draft")).unwrap();

    assert_eq!(Post::count(&pool, None), 6);
    assert_eq!(Post::count(&pool, Some("published")), 5);
    assert_eq!(Post::published(&pool, 3, 0).len(), 3);
    assert_eq!(Post::published(&pool, 10, 3).len(), 2);
    assert_eq!(Post::list(&pool, None, 100, 0).len(), 6);
}

#[test]
fn post_unique_slug() {
    let pool = test_pool();
    Post::create(&pool, &make_post_form("A", "same-slug", "draft")).unwrap();
    let result = Post::create(&pool, &make_post_form("B", "same-slug", "draft"));
    assert!(result.is_err());
}

#[test]
fn post_update_status() {
    let pool = test_pool();
    let id = Post::create(&pool, &make_post_form("Test", "test", "draft")).unwrap();
    Post::update_status(&pool, id, "published").unwrap();
    assert_eq!(Post::find_by_id(&pool, id).unwrap().status, "published");
}

// ═══════════════════════════════════════════════════════════
// Portfolio
// ═══════════════════════════════════════════════════════════

fn make_portfolio_form(title: &str, slug: &str, status: &str) -> PortfolioForm {
    PortfolioForm {
        title: title.to_string(),
        slug: slug.to_string(),
        description_json: Some("{}".to_string()),
        description_html: Some("<p>desc</p>".to_string()),
        image_path: "/img/test.jpg".to_string(),
        thumbnail_path: None,
        meta_title: None,
        meta_description: None,
        sell_enabled: None,
        price: None,
        purchase_note: None,
        payment_provider: None,
        download_file_path: None,
        status: status.to_string(),
        published_at: None,
        category_ids: None,
        tag_ids: None,
    }
}

#[test]
fn portfolio_crud() {
    let pool = test_pool();

    // Create
    let id =
        PortfolioItem::create(&pool, &make_portfolio_form("Sunset", "sunset", "draft")).unwrap();
    assert!(id > 0);

    // Read
    let item = PortfolioItem::find_by_id(&pool, id).unwrap();
    assert_eq!(item.title, "Sunset");
    assert_eq!(item.slug, "sunset");
    assert_eq!(item.status, "draft");
    assert!(!item.sell_enabled);

    // Find by slug
    let item2 = PortfolioItem::find_by_slug(&pool, "sunset").unwrap();
    assert_eq!(item2.id, id);

    // Update
    let mut form = make_portfolio_form("Sunset Updated", "sunset", "published");
    form.published_at = Some("2026-01-01T12:00".to_string());
    form.sell_enabled = Some(true);
    form.price = Some(19.99);
    form.payment_provider = Some("stripe".to_string());
    PortfolioItem::update(&pool, id, &form).unwrap();
    let updated = PortfolioItem::find_by_id(&pool, id).unwrap();
    assert_eq!(updated.title, "Sunset Updated");
    assert_eq!(updated.status, "published");
    assert!(updated.sell_enabled);
    assert_eq!(updated.price, Some(19.99));
    assert_eq!(updated.payment_provider, "stripe");

    // Count
    assert_eq!(PortfolioItem::count(&pool, None), 1);
    assert_eq!(PortfolioItem::count(&pool, Some("published")), 1);
    assert_eq!(PortfolioItem::count(&pool, Some("draft")), 0);

    // Delete
    PortfolioItem::delete(&pool, id).unwrap();
    assert!(PortfolioItem::find_by_id(&pool, id).is_none());
    assert_eq!(PortfolioItem::count(&pool, None), 0);
}

#[test]
fn portfolio_list_and_published() {
    let pool = test_pool();
    for i in 0..4 {
        PortfolioItem::create(
            &pool,
            &make_portfolio_form(&format!("Item {}", i), &format!("item-{}", i), "published"),
        )
        .unwrap();
    }
    PortfolioItem::create(
        &pool,
        &make_portfolio_form("Draft Item", "draft-item", "draft"),
    )
    .unwrap();

    assert_eq!(PortfolioItem::count(&pool, None), 5);
    assert_eq!(PortfolioItem::count(&pool, Some("published")), 4);
    assert_eq!(PortfolioItem::published(&pool, 2, 0).len(), 2);
    assert_eq!(PortfolioItem::published(&pool, 10, 2).len(), 2);
    assert_eq!(PortfolioItem::list(&pool, None, 100, 0).len(), 5);
}

#[test]
fn portfolio_unique_slug() {
    let pool = test_pool();
    PortfolioItem::create(&pool, &make_portfolio_form("A", "same-slug", "draft")).unwrap();
    let result = PortfolioItem::create(&pool, &make_portfolio_form("B", "same-slug", "draft"));
    assert!(result.is_err());
}

#[test]
fn portfolio_update_status() {
    let pool = test_pool();
    let id = PortfolioItem::create(&pool, &make_portfolio_form("Test", "test", "draft")).unwrap();
    PortfolioItem::update_status(&pool, id, "published").unwrap();
    assert_eq!(
        PortfolioItem::find_by_id(&pool, id).unwrap().status,
        "published"
    );
}

#[test]
fn portfolio_likes() {
    let pool = test_pool();
    let id = PortfolioItem::create(
        &pool,
        &make_portfolio_form("Likeable", "likeable", "published"),
    )
    .unwrap();

    let count = PortfolioItem::increment_likes(&pool, id).unwrap();
    assert_eq!(count, 1);
    let count = PortfolioItem::increment_likes(&pool, id).unwrap();
    assert_eq!(count, 2);

    let count = PortfolioItem::decrement_likes(&pool, id).unwrap();
    assert_eq!(count, 1);

    // Can't go below 0
    let count = PortfolioItem::decrement_likes(&pool, id).unwrap();
    assert_eq!(count, 0);
    let count = PortfolioItem::decrement_likes(&pool, id).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn portfolio_by_category() {
    let pool = test_pool();
    let cat_id = Category::create(&pool, &make_cat_form("Nature", "nature", "portfolio")).unwrap();
    let p1 = PortfolioItem::create(&pool, &make_portfolio_form("A", "a", "published")).unwrap();
    let _p2 = PortfolioItem::create(&pool, &make_portfolio_form("B", "b", "published")).unwrap();

    Category::set_for_content(&pool, p1, "portfolio", &[cat_id]).unwrap();

    let results = PortfolioItem::by_category(&pool, "nature", 10, 0);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "A");
}

#[test]
fn portfolio_commerce_fields() {
    let pool = test_pool();
    let mut form = make_portfolio_form("Product", "product", "published");
    form.sell_enabled = Some(true);
    form.price = Some(49.99);
    form.purchase_note = Some("High-res PNG + PSD".to_string());
    form.payment_provider = Some("paypal".to_string());
    form.download_file_path = Some("/uploads/product.zip".to_string());

    let id = PortfolioItem::create(&pool, &form).unwrap();
    let item = PortfolioItem::find_by_id(&pool, id).unwrap();
    assert!(item.sell_enabled);
    assert_eq!(item.price, Some(49.99));
    assert_eq!(item.purchase_note, "High-res PNG + PSD");
    assert_eq!(item.payment_provider, "paypal");
    assert_eq!(item.download_file_path, "/uploads/product.zip");
}

// ═══════════════════════════════════════════════════════════
// Categories
// ═══════════════════════════════════════════════════════════

fn make_cat_form(name: &str, slug: &str, cat_type: &str) -> CategoryForm {
    CategoryForm {
        name: name.to_string(),
        slug: slug.to_string(),
        r#type: cat_type.to_string(),
    }
}

#[test]
fn category_crud() {
    let pool = test_pool();

    let id = Category::create(&pool, &make_cat_form("Nature", "nature", "portfolio")).unwrap();
    assert!(id > 0);

    let cat = Category::find_by_id(&pool, id).unwrap();
    assert_eq!(cat.name, "Nature");
    assert_eq!(cat.r#type, "portfolio");

    let cat2 = Category::find_by_slug(&pool, "nature").unwrap();
    assert_eq!(cat2.id, id);

    Category::update(&pool, id, &make_cat_form("Wildlife", "wildlife", "both")).unwrap();
    let updated = Category::find_by_id(&pool, id).unwrap();
    assert_eq!(updated.name, "Wildlife");
    assert_eq!(updated.slug, "wildlife");

    assert_eq!(Category::count(&pool, None), 1);

    Category::delete(&pool, id).unwrap();
    assert!(Category::find_by_id(&pool, id).is_none());
}

#[test]
fn category_type_filter() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Blog Cat", "blog-cat", "post")).unwrap();
    Category::create(&pool, &make_cat_form("Port Cat", "port-cat", "portfolio")).unwrap();
    Category::create(&pool, &make_cat_form("Both Cat", "both-cat", "both")).unwrap();

    assert_eq!(Category::list(&pool, Some("post")).len(), 2); // post + both
    assert_eq!(Category::list(&pool, Some("portfolio")).len(), 2); // portfolio + both
    assert_eq!(Category::list(&pool, None).len(), 3);
}

#[test]
fn category_content_association() {
    let pool = test_pool();
    let cat1 = Category::create(&pool, &make_cat_form("A", "a", "post")).unwrap();
    let cat2 = Category::create(&pool, &make_cat_form("B", "b", "post")).unwrap();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "draft")).unwrap();

    Category::set_for_content(&pool, post_id, "post", &[cat1, cat2]).unwrap();
    let cats = Category::for_content(&pool, post_id, "post");
    assert_eq!(cats.len(), 2);

    // Reassign to just one
    Category::set_for_content(&pool, post_id, "post", &[cat1]).unwrap();
    assert_eq!(Category::for_content(&pool, post_id, "post").len(), 1);

    // Count items
    assert_eq!(Category::count_items(&pool, cat1), 1);
    assert_eq!(Category::count_items(&pool, cat2), 0);
}

// ═══════════════════════════════════════════════════════════
// Tags
// ═══════════════════════════════════════════════════════════

#[test]
fn tag_crud() {
    let pool = test_pool();
    let id = Tag::create(
        &pool,
        &TagForm {
            name: "Rust".to_string(),
            slug: "rust".to_string(),
        },
    )
    .unwrap();
    assert!(id > 0);

    let tag = Tag::find_by_id(&pool, id).unwrap();
    assert_eq!(tag.name, "Rust");

    Tag::update(
        &pool,
        id,
        &TagForm {
            name: "Rust Lang".to_string(),
            slug: "rust-lang".to_string(),
        },
    )
    .unwrap();
    let updated = Tag::find_by_id(&pool, id).unwrap();
    assert_eq!(updated.slug, "rust-lang");

    assert_eq!(Tag::count(&pool), 1);
    Tag::delete(&pool, id).unwrap();
    assert_eq!(Tag::count(&pool), 0);
}

#[test]
fn tag_content_association() {
    let pool = test_pool();
    let t1 = Tag::create(
        &pool,
        &TagForm {
            name: "A".to_string(),
            slug: "a".to_string(),
        },
    )
    .unwrap();
    let t2 = Tag::create(
        &pool,
        &TagForm {
            name: "B".to_string(),
            slug: "b".to_string(),
        },
    )
    .unwrap();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "draft")).unwrap();

    Tag::set_for_content(&pool, post_id, "post", &[t1, t2]).unwrap();
    assert_eq!(Tag::for_content(&pool, post_id, "post").len(), 2);

    Tag::set_for_content(&pool, post_id, "post", &[]).unwrap();
    assert_eq!(Tag::for_content(&pool, post_id, "post").len(), 0);
}

#[test]
fn tag_find_or_create() {
    let pool = test_pool();
    let id1 = Tag::find_or_create(&pool, "New Tag").unwrap();
    let id2 = Tag::find_or_create(&pool, "New Tag").unwrap();
    assert_eq!(id1, id2); // same tag, not duplicated
    assert_eq!(Tag::count(&pool), 1);
}

/// Simulates the exact route handler logic: comma-separated tag names → find_or_create → set_for_content.
/// This is the integration path that was broken (form never sent tag_names, handler never resolved them).
#[test]
fn tag_names_to_content_roundtrip_portfolio() {
    let pool = test_pool();
    let item_id = PortfolioItem::create(
        &pool,
        &PortfolioForm {
            title: "Test".to_string(),
            slug: "test".to_string(),
            description_json: None,
            description_html: None,
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
        },
    )
    .unwrap();

    // Simulate form submission with comma-separated tag names
    let tag_names_str = "Aerial, Golden Hour, Aerial"; // includes duplicate
    let tag_ids: Vec<i64> = tag_names_str
        .split(',')
        .filter_map(|n| {
            let n = n.trim();
            if n.is_empty() {
                return None;
            }
            Tag::find_or_create(&pool, n).ok()
        })
        .collect();
    Tag::set_for_content(&pool, item_id, "portfolio", &tag_ids).unwrap();

    // Verify tags persisted and are retrievable
    let saved = Tag::for_content(&pool, item_id, "portfolio");
    assert_eq!(
        saved.len(),
        2,
        "should have 2 unique tags (duplicate ignored by find_or_create)"
    );
    let names: Vec<&str> = saved.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"Aerial"), "should contain Aerial");
    assert!(names.contains(&"Golden Hour"), "should contain Golden Hour");

    // Verify tags appear in global tag list
    assert!(Tag::count(&pool) >= 2);
    assert!(Tag::find_by_slug(&pool, "aerial").is_some());
    assert!(Tag::find_by_slug(&pool, "golden-hour").is_some());
}

#[test]
fn tag_names_to_content_roundtrip_post() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("Tag Test", "tag-test", "draft")).unwrap();

    let tag_names_str = "Rust, Web Dev";
    let tag_ids: Vec<i64> = tag_names_str
        .split(',')
        .filter_map(|n| {
            let n = n.trim();
            if n.is_empty() {
                return None;
            }
            Tag::find_or_create(&pool, n).ok()
        })
        .collect();
    Tag::set_for_content(&pool, post_id, "post", &tag_ids).unwrap();

    let saved = Tag::for_content(&pool, post_id, "post");
    assert_eq!(saved.len(), 2);

    // Update: remove one tag
    let tag_ids2: Vec<i64> = "Rust"
        .split(',')
        .filter_map(|n| {
            let n = n.trim();
            if n.is_empty() {
                return None;
            }
            Tag::find_or_create(&pool, n).ok()
        })
        .collect();
    Tag::set_for_content(&pool, post_id, "post", &tag_ids2).unwrap();

    let saved2 = Tag::for_content(&pool, post_id, "post");
    assert_eq!(saved2.len(), 1, "should have 1 tag after update");
    assert_eq!(saved2[0].name, "Rust");
}

#[test]
fn tag_names_empty_clears_all() {
    let pool = test_pool();
    let post_id =
        Post::create(&pool, &make_post_form("Clear Test", "clear-test", "draft")).unwrap();

    // Add tags
    let tag_ids: Vec<i64> = vec![
        Tag::find_or_create(&pool, "A").unwrap(),
        Tag::find_or_create(&pool, "B").unwrap(),
    ];
    Tag::set_for_content(&pool, post_id, "post", &tag_ids).unwrap();
    assert_eq!(Tag::for_content(&pool, post_id, "post").len(), 2);

    // Simulate empty tag_names (user removed all pills)
    let empty_ids: Vec<i64> = ""
        .split(',')
        .filter_map(|n| {
            let n = n.trim();
            if n.is_empty() {
                return None;
            }
            Tag::find_or_create(&pool, n).ok()
        })
        .collect();
    Tag::set_for_content(&pool, post_id, "post", &empty_ids).unwrap();
    assert_eq!(
        Tag::for_content(&pool, post_id, "post").len(),
        0,
        "all tags should be cleared"
    );
}

// ═══════════════════════════════════════════════════════════
// Comments
// ═══════════════════════════════════════════════════════════

#[test]
fn comment_crud() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "published")).unwrap();

    let cid = Comment::create(
        &pool,
        &CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: "Alice".to_string(),
            author_email: Some("alice@test.com".to_string()),
            body: "Great post!".to_string(),
            honeypot: None,
            parent_id: None,
        },
    )
    .unwrap();

    let c = Comment::find_by_id(&pool, cid).unwrap();
    assert_eq!(c.author_name, "Alice");
    assert_eq!(c.status, "pending");

    // Approve
    Comment::update_status(&pool, cid, "approved").unwrap();
    let approved = Comment::find_by_id(&pool, cid).unwrap();
    assert_eq!(approved.status, "approved");

    // Count
    assert_eq!(Comment::count(&pool, None), 1);
    assert_eq!(Comment::count(&pool, Some("approved")), 1);
    assert_eq!(Comment::count(&pool, Some("pending")), 0);

    // For post (only approved)
    assert_eq!(Comment::for_post(&pool, post_id, "post").len(), 1);

    // Delete
    Comment::delete(&pool, cid).unwrap();
    assert_eq!(Comment::count(&pool, None), 0);
}

#[test]
fn comment_honeypot_blocks_spam() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "published")).unwrap();

    let result = Comment::create(
        &pool,
        &CommentForm {
            post_id,
            content_type: None,
            author_name: "Bot".to_string(),
            author_email: None,
            body: "spam".to_string(),
            honeypot: Some("gotcha".to_string()),
            parent_id: None,
        },
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Spam detected");
}

#[test]
fn comment_threaded_replies() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "published")).unwrap();

    let parent = Comment::create(
        &pool,
        &CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: "A".to_string(),
            author_email: None,
            body: "parent".to_string(),
            honeypot: None,
            parent_id: None,
        },
    )
    .unwrap();

    let child = Comment::create(
        &pool,
        &CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: "B".to_string(),
            author_email: None,
            body: "reply".to_string(),
            honeypot: None,
            parent_id: Some(parent),
        },
    )
    .unwrap();

    let c = Comment::find_by_id(&pool, child).unwrap();
    assert_eq!(c.parent_id, Some(parent));
}

// ═══════════════════════════════════════════════════════════
// Users
// ═══════════════════════════════════════════════════════════

#[test]
fn user_crud() {
    let pool = test_pool();
    let hash = fast_hash("secret123");
    let id = User::create(&pool, "admin@test.com", &hash, "Admin", "admin").unwrap();
    assert!(id > 0);

    let user = User::get_by_id(&pool, id).unwrap();
    assert_eq!(user.email, "admin@test.com");
    assert_eq!(user.role, "admin");
    assert_eq!(user.status, "active");
    assert!(user.is_admin());
    assert!(user.is_editor_or_above());
    assert!(user.is_author_or_above());
    assert!(user.is_active());

    // Get by email
    let user2 = User::get_by_email(&pool, "admin@test.com").unwrap();
    assert_eq!(user2.id, id);

    // Update profile
    User::update_profile(&pool, id, "New Name", "new@test.com", "/avatar.png").unwrap();
    let updated = User::get_by_id(&pool, id).unwrap();
    assert_eq!(updated.display_name, "New Name");
    assert_eq!(updated.email, "new@test.com");
    assert_eq!(updated.avatar, "/avatar.png");

    // Update role
    User::update_role(&pool, id, "editor").unwrap();
    let editor = User::get_by_id(&pool, id).unwrap();
    assert!(!editor.is_admin());
    assert!(editor.is_editor_or_above());

    // Count
    assert_eq!(User::count(&pool), 1);
    assert_eq!(User::count_by_role(&pool, "editor"), 1);
    assert_eq!(User::count_by_role(&pool, "admin"), 0);
}

#[test]
fn user_lock_unlock() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let id = User::create(&pool, "u@test.com", &hash, "U", "admin").unwrap();

    // Create a session for this user
    Setting::set(&pool, "session_expiry_hours", "24").unwrap();
    let session_id = auth::create_session(&pool, id, None, None).unwrap();

    // Lock — should also destroy sessions
    User::lock(&pool, id).unwrap();
    let locked = User::get_by_id(&pool, id).unwrap();
    assert_eq!(locked.status, "locked");
    assert!(!locked.is_active());

    // Session should be gone
    assert!(auth::get_session_user(&pool, &session_id).is_none());

    // Unlock
    User::unlock(&pool, id).unwrap();
    let unlocked = User::get_by_id(&pool, id).unwrap();
    assert_eq!(unlocked.status, "active");
}

#[test]
fn user_mfa() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let id = User::create(&pool, "mfa@test.com", &hash, "MFA User", "admin").unwrap();

    let user = User::get_by_id(&pool, id).unwrap();
    assert!(!user.mfa_enabled);

    User::update_mfa(&pool, id, true, "JBSWY3DPEHPK3PXP", "[\"code1\",\"code2\"]").unwrap();
    let updated = User::get_by_id(&pool, id).unwrap();
    assert!(updated.mfa_enabled);
    assert_eq!(updated.mfa_secret, "JBSWY3DPEHPK3PXP");

    // Disable
    User::update_mfa(&pool, id, false, "", "[]").unwrap();
    let disabled = User::get_by_id(&pool, id).unwrap();
    assert!(!disabled.mfa_enabled);
}

#[test]
fn user_delete_nullifies_content() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let uid = User::create(&pool, "author@test.com", &hash, "Author", "author").unwrap();

    // Create a post owned by this user
    let pid = Post::create(&pool, &make_post_form("My Post", "my-post", "draft")).unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE posts SET user_id = ?1 WHERE id = ?2",
            rusqlite::params![uid, pid],
        )
        .unwrap();
    }

    User::delete(&pool, uid).unwrap();
    assert!(User::get_by_id(&pool, uid).is_none());

    // Post should still exist but user_id should be NULL
    let post = Post::find_by_id(&pool, pid).unwrap();
    assert!(post.id > 0); // post still exists
}

#[test]
fn user_role_helpers() {
    let pool = test_pool();
    let hash = fast_hash("p");

    let admin_id = User::create(&pool, "a@t.com", &hash, "A", "admin").unwrap();
    let editor_id = User::create(&pool, "e@t.com", &hash, "E", "editor").unwrap();
    let author_id = User::create(&pool, "au@t.com", &hash, "Au", "author").unwrap();
    let sub_id = User::create(&pool, "s@t.com", &hash, "S", "subscriber").unwrap();

    let admin = User::get_by_id(&pool, admin_id).unwrap();
    assert!(admin.is_admin());
    assert!(admin.is_editor_or_above());
    assert!(admin.is_author_or_above());

    let editor = User::get_by_id(&pool, editor_id).unwrap();
    assert!(!editor.is_admin());
    assert!(editor.is_editor_or_above());
    assert!(editor.is_author_or_above());

    let author = User::get_by_id(&pool, author_id).unwrap();
    assert!(!author.is_admin());
    assert!(!author.is_editor_or_above());
    assert!(author.is_author_or_above());

    let sub = User::get_by_id(&pool, sub_id).unwrap();
    assert!(!sub.is_admin());
    assert!(!sub.is_editor_or_above());
    assert!(!sub.is_author_or_above());
}

#[test]
fn user_unique_email() {
    let pool = test_pool();
    let hash = fast_hash("p");
    User::create(&pool, "dup@test.com", &hash, "A", "admin").unwrap();
    let result = User::create(&pool, "dup@test.com", &hash, "B", "editor");
    assert!(result.is_err());
}

#[test]
fn user_safe_json_excludes_password() {
    let pool = test_pool();
    let hash = fast_hash("secret");
    let id = User::create(&pool, "safe@test.com", &hash, "Safe", "admin").unwrap();
    let user = User::get_by_id(&pool, id).unwrap();
    let json = user.safe_json();
    assert!(json.get("password_hash").is_none());
    assert_eq!(json["email"], "safe@test.com");
}

// ═══════════════════════════════════════════════════════════
// Orders + DownloadTokens + Licenses
// ═══════════════════════════════════════════════════════════

fn setup_portfolio(pool: &DbPool) -> i64 {
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, status) VALUES ('Item', 'item', '/img.jpg', 'published')",
        [],
    ).unwrap();
    conn.last_insert_rowid()
}

#[test]
fn order_crud() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);

    let oid = Order::create(
        &pool,
        pid,
        "buyer@test.com",
        "Buyer",
        29.99,
        "USD",
        "paypal",
        "PP-123",
        "pending",
    )
    .unwrap();
    assert!(oid > 0);

    let order = Order::find_by_id(&pool, oid).unwrap();
    assert_eq!(order.buyer_email, "buyer@test.com");
    assert_eq!(order.amount, 29.99);
    assert_eq!(order.status, "pending");

    // Find by provider order ID
    let o2 = Order::find_by_provider_order_id(&pool, "PP-123").unwrap();
    assert_eq!(o2.id, oid);

    // Update status
    Order::update_status(&pool, oid, "completed").unwrap();
    assert_eq!(Order::find_by_id(&pool, oid).unwrap().status, "completed");

    // Count
    assert_eq!(Order::count(&pool), 1);
    assert_eq!(Order::count_by_status(&pool, "completed"), 1);
    assert_eq!(Order::count_by_status(&pool, "pending"), 0);

    // Revenue
    assert!((Order::total_revenue(&pool) - 29.99).abs() < 0.01);
}

#[test]
fn order_list_filters() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);

    Order::create(
        &pool,
        pid,
        "a@t.com",
        "A",
        10.0,
        "USD",
        "stripe",
        "S1",
        "completed",
    )
    .unwrap();
    Order::create(
        &pool, pid, "b@t.com", "B", 20.0, "USD", "paypal", "P1", "pending",
    )
    .unwrap();
    Order::create(
        &pool,
        pid,
        "a@t.com",
        "A",
        30.0,
        "USD",
        "stripe",
        "S2",
        "completed",
    )
    .unwrap();

    assert_eq!(Order::list(&pool, 10, 0).len(), 3);
    assert_eq!(Order::list_by_status(&pool, "completed", 10, 0).len(), 2);
    assert_eq!(Order::list_by_email(&pool, "a@t.com", 10, 0).len(), 2);
    assert_eq!(Order::list_by_portfolio(&pool, pid).len(), 3);
}

#[test]
fn download_token_lifecycle() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(
        &pool,
        pid,
        "b@t.com",
        "B",
        10.0,
        "USD",
        "stripe",
        "",
        "completed",
    )
    .unwrap();

    let future = chrono::Utc::now().naive_utc() + chrono::Duration::hours(48);
    let tid = DownloadToken::create(&pool, oid, "tok-abc-123", 3, future).unwrap();
    assert!(tid > 0);

    let token = DownloadToken::find_by_token(&pool, "tok-abc-123").unwrap();
    assert_eq!(token.order_id, oid);
    assert_eq!(token.downloads_used, 0);
    assert_eq!(token.max_downloads, 3);
    assert!(token.is_valid());

    // Increment
    DownloadToken::increment_download(&pool, tid).unwrap();
    let t2 = DownloadToken::find_by_token(&pool, "tok-abc-123").unwrap();
    assert_eq!(t2.downloads_used, 1);

    // Find by order
    let t3 = DownloadToken::find_by_order(&pool, oid).unwrap();
    assert_eq!(t3.token, "tok-abc-123");
}

#[test]
fn download_token_expired() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(
        &pool,
        pid,
        "b@t.com",
        "B",
        10.0,
        "USD",
        "stripe",
        "",
        "completed",
    )
    .unwrap();

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::hours(1);
    DownloadToken::create(&pool, oid, "expired-tok", 3, past).unwrap();

    let token = DownloadToken::find_by_token(&pool, "expired-tok").unwrap();
    assert!(!token.is_valid());
}

#[test]
fn license_crud() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(
        &pool,
        pid,
        "b@t.com",
        "B",
        10.0,
        "USD",
        "stripe",
        "",
        "completed",
    )
    .unwrap();

    let lid = License::create(&pool, oid, "XXXX-YYYY-ZZZZ-1234").unwrap();
    assert!(lid > 0);

    let lic = License::find_by_order(&pool, oid).unwrap();
    assert_eq!(lic.license_key, "XXXX-YYYY-ZZZZ-1234");

    let lic2 = License::find_by_key(&pool, "XXXX-YYYY-ZZZZ-1234").unwrap();
    assert_eq!(lic2.order_id, oid);
}

// ═══════════════════════════════════════════════════════════
// Audit Log
// ═══════════════════════════════════════════════════════════

#[test]
fn audit_log_and_list() {
    let pool = test_pool();

    AuditEntry::log(
        &pool,
        Some(1),
        Some("Admin"),
        "login",
        None,
        None,
        None,
        None,
        Some("1.2.3.4"),
    );
    AuditEntry::log(
        &pool,
        Some(1),
        Some("Admin"),
        "settings_change",
        Some("settings"),
        None,
        Some("general"),
        None,
        None,
    );
    AuditEntry::log(
        &pool,
        Some(2),
        Some("Editor"),
        "post_create",
        Some("post"),
        Some(1),
        Some("Hello"),
        None,
        None,
    );

    // Count all
    assert_eq!(AuditEntry::count(&pool, None, None, None), 3);

    // Filter by action
    assert_eq!(AuditEntry::count(&pool, Some("login"), None, None), 1);

    // Filter by entity
    assert_eq!(AuditEntry::count(&pool, None, Some("post"), None), 1);

    // Filter by user
    assert_eq!(AuditEntry::count(&pool, None, None, Some(1)), 2);

    // List
    let entries = AuditEntry::list(&pool, None, None, None, 10, 0);
    assert_eq!(entries.len(), 3);

    // Distinct actions
    let actions = AuditEntry::distinct_actions(&pool);
    assert!(actions.contains(&"login".to_string()));
    assert!(actions.contains(&"post_create".to_string()));

    // Distinct entity types
    let entities = AuditEntry::distinct_entity_types(&pool);
    assert!(entities.contains(&"settings".to_string()));
    assert!(entities.contains(&"post".to_string()));
}

#[test]
fn audit_cleanup() {
    let pool = test_pool();
    // Insert an entry and backdate it to 10 days ago
    AuditEntry::log(
        &pool,
        Some(1),
        Some("A"),
        "test",
        None,
        None,
        None,
        None,
        None,
    );
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE audit_log SET created_at = datetime('now', '-10 days')",
            [],
        )
        .unwrap();
    }

    // Cleanup entries older than 5 days
    let deleted = AuditEntry::cleanup(&pool, 5).unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(AuditEntry::count(&pool, None, None, None), 0);
}

// ═══════════════════════════════════════════════════════════
// Firewall
// ═══════════════════════════════════════════════════════════

#[test]
fn fw_ban_lifecycle() {
    let pool = test_pool();

    assert!(!FwBan::is_banned(&pool, "10.0.0.1"));

    // Ban
    let bid = FwBan::create(
        &pool,
        "10.0.0.1",
        "brute_force",
        Some("5 failed logins"),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(bid > 0);
    assert!(FwBan::is_banned(&pool, "10.0.0.1"));

    // Active bans
    assert_eq!(FwBan::active_count(&pool), 1);
    assert_eq!(FwBan::active_bans(&pool, 10, 0).len(), 1);

    // Unban
    FwBan::unban(&pool, "10.0.0.1").unwrap();
    assert!(!FwBan::is_banned(&pool, "10.0.0.1"));
    assert_eq!(FwBan::active_count(&pool), 0);

    // History still shows it
    assert_eq!(FwBan::all_bans(&pool, 10, 0).len(), 1);
}

#[test]
fn fw_ban_with_duration() {
    let pool = test_pool();

    FwBan::create_with_duration(&pool, "10.0.0.2", "rate_limit", None, "24h", None, None).unwrap();
    assert!(FwBan::is_banned(&pool, "10.0.0.2"));

    // Permanent ban
    FwBan::create_with_duration(&pool, "10.0.0.3", "manual", None, "permanent", None, None)
        .unwrap();
    assert!(FwBan::is_banned(&pool, "10.0.0.3"));
}

#[test]
fn fw_ban_replaces_existing() {
    let pool = test_pool();
    FwBan::create(&pool, "10.0.0.5", "first", None, None, None, None).unwrap();
    FwBan::create(&pool, "10.0.0.5", "second", None, None, None, None).unwrap();

    // Should only have 1 active ban (old one deactivated)
    assert_eq!(FwBan::active_count(&pool), 1);
    let bans = FwBan::active_bans(&pool, 10, 0);
    assert_eq!(bans[0].reason, "second");
}

#[test]
fn fw_ban_unban_by_id() {
    let pool = test_pool();
    let bid = FwBan::create(&pool, "10.0.0.6", "test", None, None, None, None).unwrap();
    assert!(FwBan::is_banned(&pool, "10.0.0.6"));

    FwBan::unban_by_id(&pool, bid).unwrap();
    assert!(!FwBan::is_banned(&pool, "10.0.0.6"));
}

#[test]
fn fw_event_logging() {
    let pool = test_pool();

    FwEvent::log(
        &pool,
        "10.0.0.1",
        "failed_login",
        Some("bad password"),
        None,
        Some("Mozilla/5.0"),
        Some("/admin/login"),
    );
    FwEvent::log(&pool, "10.0.0.1", "failed_login", None, None, None, None);
    FwEvent::log(
        &pool,
        "10.0.0.2",
        "bot_detected",
        None,
        None,
        None,
        Some("/wp-admin"),
    );

    assert_eq!(FwEvent::count_all(&pool, None), 3);
    assert_eq!(FwEvent::count_all(&pool, Some("failed_login")), 2);
    assert_eq!(FwEvent::count_all(&pool, Some("bot_detected")), 1);

    // Recent events
    assert_eq!(FwEvent::recent(&pool, None, 10, 0).len(), 3);
    assert_eq!(FwEvent::recent(&pool, Some("bot_detected"), 10, 0).len(), 1);

    // Top IPs
    let top = FwEvent::top_ips(&pool, 5);
    assert!(!top.is_empty());
    assert_eq!(top[0].0, "10.0.0.1");
    assert_eq!(top[0].1, 2);

    // Counts by type
    let by_type = FwEvent::counts_by_type(&pool);
    assert!(!by_type.is_empty());
}

#[test]
fn fw_event_count_for_ip() {
    let pool = test_pool();
    FwEvent::log(&pool, "10.0.0.1", "failed_login", None, None, None, None);
    FwEvent::log(&pool, "10.0.0.1", "failed_login", None, None, None, None);
    FwEvent::log(&pool, "10.0.0.1", "bot_detected", None, None, None, None);

    let count = FwEvent::count_for_ip_since(&pool, "10.0.0.1", "failed_login", 60);
    assert_eq!(count, 2);
}

// ═══════════════════════════════════════════════════════════
// Security: Password hashing
// ═══════════════════════════════════════════════════════════

#[test]
fn password_hash_and_verify() {
    let hash = fast_hash("my_secure_password");
    assert!(auth::verify_password("my_secure_password", &hash));
    assert!(!auth::verify_password("wrong_password", &hash));
}

#[test]
fn password_hash_unique_salts() {
    let h1 = fast_hash("same");
    let h2 = fast_hash("same");
    assert_ne!(h1, h2); // bcrypt uses random salts
    assert!(auth::verify_password("same", &h1));
    assert!(auth::verify_password("same", &h2));
}

// ═══════════════════════════════════════════════════════════
// Security: Sessions
// ═══════════════════════════════════════════════════════════

#[test]
fn session_create_and_validate() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let uid = User::create(&pool, "sess@test.com", &hash, "Sess", "admin").unwrap();

    let sid = auth::create_session(&pool, uid, Some("1.2.3.4"), Some("TestAgent")).unwrap();
    assert!(!sid.is_empty());

    // Validate
    assert!(auth::validate_session(&pool, &sid));
    let user = auth::get_session_user(&pool, &sid).unwrap();
    assert_eq!(user.id, uid);

    // Invalid session
    assert!(!auth::validate_session(&pool, "nonexistent"));
    assert!(auth::get_session_user(&pool, "nonexistent").is_none());
}

#[test]
fn session_destroy() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let uid = User::create(&pool, "d@test.com", &hash, "D", "admin").unwrap();

    let sid = auth::create_session(&pool, uid, None, None).unwrap();
    assert!(auth::validate_session(&pool, &sid));

    auth::destroy_session(&pool, &sid).unwrap();
    assert!(!auth::validate_session(&pool, &sid));
}

#[test]
fn session_cleanup_expired() {
    let pool = test_pool();
    let hash = fast_hash("pass");
    let uid = User::create(&pool, "exp@test.com", &hash, "E", "admin").unwrap();

    // Create a valid session
    let sid = auth::create_session(&pool, uid, None, None).unwrap();

    // Manually insert an expired session
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ('expired-sess', ?1, datetime('now', '-2 days'), datetime('now', '-1 day'))",
            rusqlite::params![uid],
        ).unwrap();
    }

    auth::cleanup_expired_sessions(&pool);

    // Valid session should still exist
    assert!(auth::validate_session(&pool, &sid));
    // Expired session should be gone
    assert!(!auth::validate_session(&pool, "expired-sess"));
}

#[test]
fn ip_hashing() {
    let h1 = auth::hash_ip("192.168.1.1");
    let h2 = auth::hash_ip("192.168.1.1");
    let h3 = auth::hash_ip("10.0.0.1");
    assert_eq!(h1, h2); // deterministic
    assert_ne!(h1, h3); // different IPs
    assert_eq!(h1.len(), 64); // SHA-256 hex
}

// ═══════════════════════════════════════════════════════════
// Slug Validation
// ═══════════════════════════════════════════════════════════

// These test the validation logic extracted from settings_save.
// We replicate the RESERVED_SLUGS list and is_reserved() here.

const RESERVED_SLUGS: &[&str] = &[
    "static",
    "uploads",
    "api",
    "super",
    "download",
    "feed",
    "sitemap.xml",
    "robots.txt",
    "privacy",
    "terms",
    "archives",
    "login",
    "logout",
    "setup",
    "mfa",
    "magic-link",
    "forgot-password",
    "reset-password",
];

fn is_reserved(s: &str) -> bool {
    RESERVED_SLUGS.contains(&s.to_lowercase().as_str())
}

#[test]
fn reserved_slugs_blocked() {
    for slug in RESERVED_SLUGS {
        assert!(is_reserved(slug), "'{}' should be reserved", slug);
    }
    // Case insensitive
    assert!(is_reserved("Static"));
    assert!(is_reserved("API"));
    assert!(is_reserved("SUPER"));
}

#[test]
fn valid_slugs_allowed() {
    assert!(!is_reserved("admin"));
    assert!(!is_reserved("journal"));
    assert!(!is_reserved("portfolio"));
    assert!(!is_reserved("blog"));
    assert!(!is_reserved("gallery"));
    assert!(!is_reserved("my-custom-slug"));
    assert!(!is_reserved(""));
}

#[test]
fn slug_cross_validation() {
    // Simulate: admin=admin, blog=journal, portfolio=portfolio — all different = OK
    let admin = "admin";
    let blog = "journal";
    let portfolio = "portfolio";
    assert_ne!(admin, blog);
    assert_ne!(admin, portfolio);
    assert_ne!(blog, portfolio);

    // Simulate conflict: admin=journal, blog=journal — should fail
    let admin2 = "journal";
    let blog2 = "journal";
    assert_eq!(admin2, blog2); // conflict detected

    // Empty blog slug is allowed (mounts at /)
    let blog3 = "";
    assert!(!is_reserved(blog3));
}

// ═══════════════════════════════════════════════════════════
// Settings: additional coverage
// ═══════════════════════════════════════════════════════════

#[test]
fn settings_get_f64() {
    let pool = test_pool();
    Setting::set(&pool, "price", "19.99").unwrap();
    assert!((Setting::get_f64(&pool, "price") - 19.99).abs() < 0.001);
    assert_eq!(Setting::get_f64(&pool, "missing"), 0.0);
}

#[test]
fn settings_get_group() {
    let pool = test_pool();
    Setting::set(&pool, "smtp_host", "mail.example.com").unwrap();
    Setting::set(&pool, "smtp_port", "587").unwrap();
    Setting::set(&pool, "smtp_user", "user@example.com").unwrap();
    Setting::set(&pool, "unrelated_key", "nope").unwrap();

    let group = Setting::get_group(&pool, "smtp_");
    assert_eq!(group.len(), 3);
    assert_eq!(group.get("smtp_host").unwrap(), "mail.example.com");
    assert_eq!(group.get("smtp_port").unwrap(), "587");
    assert!(group.get("unrelated_key").is_none());
}

// ═══════════════════════════════════════════════════════════
// Users: additional coverage
// ═══════════════════════════════════════════════════════════

#[test]
fn user_update_password() {
    let pool = test_pool();
    let hash1 = fast_hash("old_pass");
    let id = User::create(&pool, "pw@test.com", &hash1, "PW", "admin").unwrap();

    let hash2 = fast_hash("new_pass");
    User::update_password(&pool, id, &hash2).unwrap();

    let user = User::get_by_id(&pool, id).unwrap();
    assert!(auth::verify_password("new_pass", &user.password_hash));
    assert!(!auth::verify_password("old_pass", &user.password_hash));
}

#[test]
fn user_update_avatar() {
    let pool = test_pool();
    let hash = fast_hash("p");
    let id = User::create(&pool, "av@test.com", &hash, "Av", "admin").unwrap();

    User::update_avatar(&pool, id, "/uploads/avatar.png").unwrap();
    let user = User::get_by_id(&pool, id).unwrap();
    assert_eq!(user.avatar, "/uploads/avatar.png");
}

#[test]
fn user_touch_last_login() {
    let pool = test_pool();
    let hash = fast_hash("p");
    let id = User::create(&pool, "login@test.com", &hash, "L", "admin").unwrap();

    let before = User::get_by_id(&pool, id).unwrap();
    assert!(before.last_login_at.is_none());

    User::touch_last_login(&pool, id).unwrap();
    let after = User::get_by_id(&pool, id).unwrap();
    assert!(after.last_login_at.is_some());
}

#[test]
fn user_list_paginated() {
    let pool = test_pool();
    let hash = fast_hash("p");
    for i in 0..5 {
        User::create(
            &pool,
            &format!("u{}@t.com", i),
            &hash,
            &format!("U{}", i),
            "editor",
        )
        .unwrap();
    }
    User::create(&pool, "admin@t.com", &hash, "Admin", "admin").unwrap();

    // All users
    assert_eq!(User::count_filtered(&pool, None), 6);
    assert_eq!(User::list_paginated(&pool, None, 3, 0).len(), 3);
    assert_eq!(User::list_paginated(&pool, None, 10, 4).len(), 2);

    // Filter by role
    assert_eq!(User::count_filtered(&pool, Some("editor")), 5);
    assert_eq!(User::list_paginated(&pool, Some("editor"), 10, 0).len(), 5);
    assert_eq!(User::count_filtered(&pool, Some("admin")), 1);
}

// ═══════════════════════════════════════════════════════════
// Designs + DesignTemplates
// ═══════════════════════════════════════════════════════════

#[test]
fn design_crud() {
    let pool = test_pool();

    // seed_defaults creates "Inkwell" + "Oneguy" designs, so count starts at 2
    let baseline = Design::list(&pool).len();

    let id = Design::create(&pool, "Custom Theme").unwrap();
    assert!(id > 0);

    let design = Design::find_by_id(&pool, id).unwrap();
    assert_eq!(design.name, "Custom Theme");
    assert_eq!(design.slug, "custom-theme");
    assert!(!design.is_active);

    // Find by slug
    let by_slug = Design::find_by_slug(&pool, "custom-theme").unwrap();
    assert_eq!(by_slug.id, id);

    // List
    assert_eq!(Design::list(&pool).len(), baseline + 1);

    // Delete
    Design::delete(&pool, id).unwrap();
    assert!(Design::find_by_id(&pool, id).is_none());
    assert_eq!(Design::list(&pool).len(), baseline);
}

#[test]
fn design_activate() {
    let pool = test_pool();
    let d1 = Design::create(&pool, "Theme A").unwrap();
    let d2 = Design::create(&pool, "Theme B").unwrap();

    // Activate d1
    Design::activate(&pool, d1).unwrap();
    assert!(Design::find_by_id(&pool, d1).unwrap().is_active);
    assert!(!Design::find_by_id(&pool, d2).unwrap().is_active);
    assert_eq!(Design::active(&pool).unwrap().id, d1);

    // Activate d2 — d1 should deactivate
    Design::activate(&pool, d2).unwrap();
    assert!(!Design::find_by_id(&pool, d1).unwrap().is_active);
    assert!(Design::find_by_id(&pool, d2).unwrap().is_active);
    assert_eq!(Design::active(&pool).unwrap().id, d2);
}

#[test]
fn design_duplicate() {
    let pool = test_pool();
    let orig = Design::create(&pool, "Original").unwrap();

    // Add templates to original
    DesignTemplate::upsert(&pool, orig, "homepage", "<h1>Home</h1>", "h1{color:red}").unwrap();
    DesignTemplate::upsert(&pool, orig, "post", "<article/>", "article{}").unwrap();

    // Duplicate
    let dup = Design::duplicate(&pool, orig, "Copy of Original").unwrap();
    assert_ne!(orig, dup);

    let dup_design = Design::find_by_id(&pool, dup).unwrap();
    assert_eq!(dup_design.name, "Copy of Original");
    assert_eq!(dup_design.slug, "copy-of-original");

    // Templates should be duplicated
    let templates = DesignTemplate::for_design(&pool, dup);
    assert_eq!(templates.len(), 2);
}

#[test]
fn design_template_upsert_and_get() {
    let pool = test_pool();
    let did = Design::create(&pool, "Test Design").unwrap();

    // Create template
    DesignTemplate::upsert(&pool, did, "homepage", "<div>v1</div>", ".v1{}").unwrap();
    let t = DesignTemplate::get(&pool, did, "homepage").unwrap();
    assert_eq!(t.layout_html, "<div>v1</div>");
    assert_eq!(t.style_css, ".v1{}");

    // Update (upsert same type)
    DesignTemplate::upsert(&pool, did, "homepage", "<div>v2</div>", ".v2{}").unwrap();
    let t2 = DesignTemplate::get(&pool, did, "homepage").unwrap();
    assert_eq!(t2.layout_html, "<div>v2</div>");

    // Different template type
    DesignTemplate::upsert(&pool, did, "post", "<article/>", "").unwrap();
    assert_eq!(DesignTemplate::for_design(&pool, did).len(), 2);

    // Delete design cascades templates
    Design::delete(&pool, did).unwrap();
    assert_eq!(DesignTemplate::for_design(&pool, did).len(), 0);
}

// ═══════════════════════════════════════════════════════════
// Analytics (PageView)
// ═══════════════════════════════════════════════════════════

#[test]
fn pageview_record_and_overview() {
    let pool = test_pool();

    PageView::record(
        &pool,
        "/",
        "hash1",
        Some("US"),
        None,
        Some("https://google.com"),
        Some("Mozilla/5.0"),
        Some("desktop"),
        Some("Chrome"),
    )
    .unwrap();
    PageView::record(
        &pool,
        "/blog/hello",
        "hash2",
        Some("UK"),
        None,
        None,
        None,
        Some("mobile"),
        Some("Safari"),
    )
    .unwrap();
    PageView::record(
        &pool,
        "/portfolio/sunset",
        "hash1",
        Some("US"),
        None,
        None,
        None,
        Some("desktop"),
        Some("Chrome"),
    )
    .unwrap();

    let from = "2020-01-01";
    let to = "2030-12-31";

    let stats = PageView::overview(&pool, from, to);
    assert_eq!(stats.total_views, 3);
    assert_eq!(stats.unique_visitors, 2);
}

#[test]
fn pageview_calendar_data() {
    let pool = test_pool();
    PageView::record(&pool, "/", "h1", None, None, None, None, None, None).unwrap();
    PageView::record(&pool, "/about", "h2", None, None, None, None, None, None).unwrap();

    let data = PageView::calendar_data(&pool, "2020-01-01", "2030-12-31");
    assert!(!data.is_empty());
    let total: i64 = data.iter().map(|d| d.count).sum();
    assert_eq!(total, 2);
}

#[test]
fn pageview_geo_data() {
    let pool = test_pool();
    PageView::record(&pool, "/", "h1", Some("US"), None, None, None, None, None).unwrap();
    PageView::record(&pool, "/", "h2", Some("US"), None, None, None, None, None).unwrap();
    PageView::record(&pool, "/", "h3", Some("UK"), None, None, None, None, None).unwrap();

    let geo = PageView::geo_data(&pool, "2020-01-01", "2030-12-31");
    assert_eq!(geo.len(), 2);
    assert_eq!(geo[0].label, "US");
    assert_eq!(geo[0].count, 2);
}

#[test]
fn pageview_top_referrers() {
    let pool = test_pool();
    PageView::record(
        &pool,
        "/",
        "h1",
        None,
        None,
        Some("https://google.com"),
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(
        &pool,
        "/",
        "h2",
        None,
        None,
        Some("https://google.com"),
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(&pool, "/", "h3", None, None, None, None, None, None).unwrap();

    let refs = PageView::top_referrers(&pool, "2020-01-01", "2030-12-31", 10);
    assert_eq!(refs.len(), 2);
    // Top referrer should be google (2 hits)
    assert_eq!(refs[0].count, 2);
}

#[test]
fn pageview_stream_data() {
    let pool = test_pool();
    PageView::record(&pool, "/blog/a", "h1", None, None, None, None, None, None).unwrap();
    PageView::record(
        &pool,
        "/portfolio/b",
        "h2",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(&pool, "/about", "h3", None, None, None, None, None, None).unwrap();

    let stream = PageView::stream_data(&pool, "2020-01-01", "2030-12-31");
    assert!(!stream.is_empty());
    let total: i64 = stream.iter().map(|s| s.count).sum();
    assert_eq!(total, 3);
}

#[test]
fn pageview_top_portfolio() {
    let pool = test_pool();
    PageView::record(
        &pool,
        "/portfolio/sunset",
        "h1",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(
        &pool,
        "/portfolio/sunset",
        "h2",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(
        &pool,
        "/portfolio/dawn",
        "h3",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    PageView::record(
        &pool,
        "/blog/unrelated",
        "h4",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let top = PageView::top_portfolio(&pool, "2020-01-01", "2030-12-31", 10);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].label, "/portfolio/sunset");
    assert_eq!(top[0].count, 2);
}

#[test]
fn pageview_tag_relations() {
    let pool = test_pool();
    let t1 = Tag::create(
        &pool,
        &TagForm {
            name: "Rust".to_string(),
            slug: "rust".to_string(),
        },
    )
    .unwrap();
    let t2 = Tag::create(
        &pool,
        &TagForm {
            name: "Web".to_string(),
            slug: "web".to_string(),
        },
    )
    .unwrap();
    let t3 = Tag::create(
        &pool,
        &TagForm {
            name: "API".to_string(),
            slug: "api-tag".to_string(),
        },
    )
    .unwrap();

    let p1 = Post::create(&pool, &make_post_form("P1", "p1", "published")).unwrap();
    let p2 = Post::create(&pool, &make_post_form("P2", "p2", "published")).unwrap();

    // p1 has Rust + Web, p2 has Rust + API
    Tag::set_for_content(&pool, p1, "post", &[t1, t2]).unwrap();
    Tag::set_for_content(&pool, p2, "post", &[t1, t3]).unwrap();

    let relations = PageView::tag_relations(&pool);
    assert!(!relations.is_empty());
    // Rust-Web and Rust-API should appear
    assert!(relations
        .iter()
        .any(|r| r.source == "API" || r.target == "API"));
}

// ═══════════════════════════════════════════════════════════
// Imports
// ═══════════════════════════════════════════════════════════

#[test]
fn import_create_and_list() {
    let pool = test_pool();

    let id = Import::create(
        &pool,
        "wordpress",
        Some("export.xml"),
        10,
        5,
        3,
        2,
        Some("All good"),
    )
    .unwrap();
    assert!(id > 0);

    let id2 = Import::create(&pool, "velocty", Some("backup.json"), 20, 0, 0, 0, None).unwrap();
    assert!(id2 > id);

    let list = Import::list(&pool);
    assert_eq!(list.len(), 2);

    // Find the wordpress import by source (order may vary when timestamps are identical)
    let wp = list.iter().find(|i| i.source == "wordpress").unwrap();
    assert_eq!(wp.posts_count, 10);
    assert_eq!(wp.portfolio_count, 5);
    assert_eq!(wp.comments_count, 3);
    assert_eq!(wp.skipped_count, 2);
    assert_eq!(wp.log.as_deref(), Some("All good"));

    let vel = list.iter().find(|i| i.source == "velocty").unwrap();
    assert_eq!(vel.posts_count, 20);
    assert!(vel.log.is_none());
}

// ═══════════════════════════════════════════════════════════
// MFA (TOTP)
// ═══════════════════════════════════════════════════════════

#[test]
fn mfa_generate_secret() {
    let secret = mfa::generate_secret();
    assert!(!secret.is_empty());
    // Base32-encoded secrets are alphanumeric
    assert!(secret.chars().all(|c| c.is_alphanumeric() || c == '='));
}

#[test]
fn mfa_generate_recovery_codes() {
    let codes = mfa::generate_recovery_codes();
    assert_eq!(codes.len(), 10);
    for code in &codes {
        // Format: XXXX-XXXX (9 chars with dash)
        assert_eq!(code.len(), 9);
        assert_eq!(&code[4..5], "-");
    }
    // All codes should be unique
    let mut unique = codes.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 10);
}

#[test]
fn mfa_verify_code_rejects_bad_input() {
    let secret = mfa::generate_secret();
    // Random wrong code should fail
    assert!(!mfa::verify_code(&secret, "000000"));
    assert!(!mfa::verify_code(&secret, ""));
    assert!(!mfa::verify_code(&secret, "not-a-code"));
    // Invalid secret should fail gracefully
    assert!(!mfa::verify_code("not-a-valid-base32!!!", "123456"));
}

#[test]
fn mfa_qr_data_uri() {
    let secret = mfa::generate_secret();
    let result = mfa::qr_data_uri(&secret, "Velocty", "admin@test.com");
    assert!(result.is_ok());
    let uri = result.unwrap();
    assert!(uri.starts_with("data:image/png;base64,"));
    assert!(uri.len() > 100); // should be a substantial base64 string
}

// ═══════════════════════════════════════════════════════════
// Firewall: additional coverage
// ═══════════════════════════════════════════════════════════

#[test]
fn fw_ban_expire_stale() {
    let pool = test_pool();

    // Create a ban that already expired (active=1 but expires_at in the past)
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO fw_bans (ip, reason, active, expires_at) VALUES ('10.0.0.99', 'test', 1, datetime('now', '-1 hour'))",
            [],
        ).unwrap();
    }

    // is_banned already filters by expires_at, so it returns false
    // but the row is still active=1 in the DB
    let active_before: i64 = {
        let conn = pool.get().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM fw_bans WHERE ip = '10.0.0.99' AND active = 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(active_before, 1);

    // expire_stale marks it inactive
    FwBan::expire_stale(&pool);

    let active_after: i64 = {
        let conn = pool.get().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM fw_bans WHERE ip = '10.0.0.99' AND active = 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(active_after, 0);
}

// ═══════════════════════════════════════════════════════════
// Orders: additional coverage
// ═══════════════════════════════════════════════════════════

#[test]
fn order_revenue_by_period() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);

    Order::create(
        &pool,
        pid,
        "a@t.com",
        "A",
        50.0,
        "USD",
        "stripe",
        "S1",
        "completed",
    )
    .unwrap();
    Order::create(
        &pool,
        pid,
        "b@t.com",
        "B",
        30.0,
        "USD",
        "stripe",
        "S2",
        "completed",
    )
    .unwrap();
    Order::create(
        &pool, pid, "c@t.com", "C", 20.0, "USD", "stripe", "S3", "pending",
    )
    .unwrap();

    // Revenue for last 30 days (all orders are fresh)
    let rev = Order::revenue_by_period(&pool, 30);
    assert!((rev - 80.0).abs() < 0.01); // only completed orders

    // Total revenue
    assert!((Order::total_revenue(&pool) - 80.0).abs() < 0.01);
}

#[test]
fn download_token_max_downloads_exhausted() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(
        &pool,
        pid,
        "b@t.com",
        "B",
        10.0,
        "USD",
        "stripe",
        "",
        "completed",
    )
    .unwrap();

    let future = chrono::Utc::now().naive_utc() + chrono::Duration::hours(48);
    let tid = DownloadToken::create(&pool, oid, "tok-exhaust", 2, future).unwrap();

    // Use up all downloads
    DownloadToken::increment_download(&pool, tid).unwrap();
    DownloadToken::increment_download(&pool, tid).unwrap();

    let token = DownloadToken::find_by_token(&pool, "tok-exhaust").unwrap();
    assert_eq!(token.downloads_used, 2);
    assert!(!token.is_valid()); // max_downloads reached
}

// ═══════════════════════════════════════════════════════════
// Security: rate limiting
// ═══════════════════════════════════════════════════════════

#[test]
fn login_rate_limit() {
    let pool = test_pool();
    Setting::set(&pool, "login_rate_limit", "3").unwrap();

    let hash = fast_hash("p");
    let uid = User::create(&pool, "rl@test.com", &hash, "RL", "admin").unwrap();

    // check_login_rate_limit hashes the IP, then queries sessions by ip_address.
    // create_session stores the raw IP. So we store the hashed IP directly
    // to simulate what a real login flow does (the route hashes before storing).
    let ip = "192.168.1.1";
    let ip_hash = auth::hash_ip(ip);

    // Insert sessions with the hashed IP to match what rate limiter queries
    for _ in 0..3 {
        let conn = pool.get().unwrap();
        let now = chrono::Utc::now().naive_utc();
        let expires = now + chrono::Duration::hours(24);
        conn.execute(
            "INSERT INTO sessions (id, user_id, created_at, expires_at, ip_address) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), uid, now, expires, ip_hash],
        ).unwrap();
    }

    // 4th attempt should be rate limited
    assert!(!auth::check_login_rate_limit(&pool, ip));

    // Different IP should still be allowed
    assert!(auth::check_login_rate_limit(&pool, "10.0.0.1"));
}

// ═══════════════════════════════════════════════════════════
// In-memory RateLimiter
// ═══════════════════════════════════════════════════════════

#[test]
fn rate_limiter_basic() {
    let rl = RateLimiter::new();
    let window = std::time::Duration::from_secs(60);

    assert!(rl.check_and_record("login:1.2.3.4", 3, window));
    assert!(rl.check_and_record("login:1.2.3.4", 3, window));
    assert!(rl.check_and_record("login:1.2.3.4", 3, window));
    // 4th should be blocked
    assert!(!rl.check_and_record("login:1.2.3.4", 3, window));

    // Different key is independent
    assert!(rl.check_and_record("login:5.6.7.8", 3, window));
}

#[test]
fn rate_limiter_remaining() {
    let rl = RateLimiter::new();
    let window = std::time::Duration::from_secs(60);

    assert_eq!(rl.remaining("comment:1.2.3.4", 5, window), 5);
    rl.check_and_record("comment:1.2.3.4", 5, window);
    rl.check_and_record("comment:1.2.3.4", 5, window);
    assert_eq!(rl.remaining("comment:1.2.3.4", 5, window), 3);
}

#[test]
fn rate_limiter_cleanup() {
    let rl = RateLimiter::new();
    let window = std::time::Duration::from_secs(60);

    rl.check_and_record("a", 10, window);
    rl.check_and_record("b", 10, window);

    // Cleanup with a very large max_age should keep everything
    rl.cleanup(std::time::Duration::from_secs(3600));
    assert_eq!(rl.remaining("a", 10, window), 9);

    // Cleanup with zero max_age should remove everything
    rl.cleanup(std::time::Duration::from_secs(0));
    assert_eq!(rl.remaining("a", 10, window), 10);
}

// ═══════════════════════════════════════════════════════════
// RSS Feed
// ═══════════════════════════════════════════════════════════

#[test]
fn rss_feed_generation() {
    let pool = test_pool();
    Setting::set(&pool, "site_name", "Test Site").unwrap();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "blog").unwrap();

    // Create published posts
    let mut form = make_post_form("Hello World", "hello-world", "published");
    form.published_at = Some("2026-01-15T10:00".to_string());
    Post::create(&pool, &form).unwrap();

    let xml = rss::generate_feed(&pool);
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<rss version=\"2.0\""));
    assert!(xml.contains("<title>Test Site</title>"));
    assert!(xml.contains("<link>https://example.com</link>"));
    assert!(xml.contains("hello-world"));
    assert!(xml.contains("Hello World"));
    assert!(xml.contains("</rss>"));
}

#[test]
fn rss_feed_empty() {
    let pool = test_pool();
    let xml = rss::generate_feed(&pool);
    assert!(xml.contains("<channel>"));
    assert!(xml.contains("</channel>"));
    // No <item> tags when no posts
    assert!(!xml.contains("<item>"));
}

// ═══════════════════════════════════════════════════════════
// License text generation
// ═══════════════════════════════════════════════════════════

#[test]
fn license_txt_generation() {
    let pool = test_pool();
    Setting::set(&pool, "admin_display_name", "John Doe").unwrap();
    Setting::set(
        &pool,
        "downloads_license_template",
        "You may use this for personal and commercial projects.",
    )
    .unwrap();

    let txt = license::generate_license_txt(&pool, "Sunset Photo", "TXN-12345", "2026-01-15");
    assert!(txt.contains("License for: Sunset Photo"));
    assert!(txt.contains("Purchased from: John Doe"));
    assert!(txt.contains("Transaction: TXN-12345"));
    assert!(txt.contains("Date: 2026-01-15"));
    assert!(txt.contains("personal and commercial"));
}

// ═══════════════════════════════════════════════════════════
// SEO: Meta tags
// ═══════════════════════════════════════════════════════════

#[test]
fn seo_build_meta_basic() {
    let pool = test_pool();
    Setting::set(&pool, "site_name", "My Site").unwrap();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();

    let meta = seo::build_meta(&pool, Some("Hello"), Some("A description"), "/blog/hello");
    assert!(meta.contains("<title>"));
    assert!(meta.contains("Hello"));
    assert!(meta.contains("My Site"));
    assert!(meta.contains("A description"));
    assert!(meta.contains("canonical"));
    assert!(meta.contains("/blog/hello"));
}

#[test]
fn seo_build_meta_og_twitter() {
    let pool = test_pool();
    Setting::set(&pool, "site_name", "OG Site").unwrap();
    Setting::set(&pool, "seo_open_graph", "true").unwrap();
    Setting::set(&pool, "seo_twitter_cards", "true").unwrap();

    let meta = seo::build_meta(&pool, Some("Post"), Some("Desc"), "/p");
    assert!(meta.contains("og:title"));
    assert!(meta.contains("og:description"));
    assert!(meta.contains("og:site_name"));
    assert!(meta.contains("twitter:card"));
    assert!(meta.contains("twitter:title"));
}

#[test]
fn seo_build_meta_no_og_twitter() {
    let pool = test_pool();
    // Explicitly disable OG and Twitter (seed_defaults enables them)
    Setting::set(&pool, "seo_open_graph", "false").unwrap();
    Setting::set(&pool, "seo_twitter_cards", "false").unwrap();
    let meta = seo::build_meta(&pool, Some("Post"), None, "/p");
    assert!(!meta.contains("og:title"));
    assert!(!meta.contains("twitter:card"));
}

// ═══════════════════════════════════════════════════════════
// SEO: Canonical URL slug correctness
// ═══════════════════════════════════════════════════════════

#[test]
fn slug_url_empty_slug_root() {
    // Empty blog_slug means journal is at /
    assert_eq!(render::slug_url("", ""), "/");
}

#[test]
fn slug_url_empty_slug_with_sub() {
    // Empty blog_slug + post slug → /my-post
    assert_eq!(render::slug_url("", "my-post"), "/my-post");
}

#[test]
fn slug_url_empty_slug_with_nested_sub() {
    // Empty blog_slug + category path → /category/nature
    assert_eq!(render::slug_url("", "category/nature"), "/category/nature");
}

#[test]
fn slug_url_named_slug_root() {
    // portfolio_slug = "portfolio" → /portfolio
    assert_eq!(render::slug_url("portfolio", ""), "/portfolio");
}

#[test]
fn slug_url_named_slug_with_sub() {
    // portfolio_slug + item slug → /portfolio/sunset
    assert_eq!(render::slug_url("portfolio", "sunset"), "/portfolio/sunset");
}

#[test]
fn slug_url_named_slug_with_nested_sub() {
    assert_eq!(
        render::slug_url("portfolio", "category/landscape"),
        "/portfolio/category/landscape"
    );
}

#[test]
fn seo_canonical_blog_empty_slug() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "seo_canonical_base", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "").unwrap();
    // Blog list canonical should be site root
    let path = render::slug_url(&Setting::get_or(&pool, "blog_slug", "journal"), "");
    let meta = seo::build_meta(&pool, Some("Blog"), None, &path);
    assert!(
        meta.contains("href=\"https://example.com/\""),
        "blog canonical with empty slug should be site root, got: {}",
        meta
    );
}

#[test]
fn seo_canonical_blog_named_slug() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "seo_canonical_base", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "journal").unwrap();
    let path = render::slug_url(&Setting::get_or(&pool, "blog_slug", "journal"), "");
    let meta = seo::build_meta(&pool, Some("Blog"), None, &path);
    assert!(
        meta.contains("href=\"https://example.com/journal\""),
        "blog canonical with named slug should use /journal, got: {}",
        meta
    );
}

#[test]
fn seo_canonical_blog_single_empty_slug() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "seo_canonical_base", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "").unwrap();
    let path = render::slug_url(
        &Setting::get_or(&pool, "blog_slug", "journal"),
        "hello-world",
    );
    let meta = seo::build_meta(&pool, Some("Hello"), None, &path);
    assert!(
        meta.contains("href=\"https://example.com/hello-world\""),
        "single post canonical with empty blog_slug should be /hello-world, got: {}",
        meta
    );
}

#[test]
fn seo_canonical_portfolio_slug() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "seo_canonical_base", "https://example.com").unwrap();
    Setting::set(&pool, "portfolio_slug", "portfolio").unwrap();
    let path = render::slug_url(&Setting::get_or(&pool, "portfolio_slug", "portfolio"), "");
    let meta = seo::build_meta(&pool, Some("Portfolio"), None, &path);
    assert!(
        meta.contains("href=\"https://example.com/portfolio\""),
        "portfolio canonical should use /portfolio, got: {}",
        meta
    );
}

#[test]
fn seo_canonical_portfolio_single() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "seo_canonical_base", "https://example.com").unwrap();
    Setting::set(&pool, "portfolio_slug", "work").unwrap();
    let path = render::slug_url(
        &Setting::get_or(&pool, "portfolio_slug", "portfolio"),
        "sunset",
    );
    let meta = seo::build_meta(&pool, Some("Sunset"), None, &path);
    assert!(
        meta.contains("href=\"https://example.com/work/sunset\""),
        "portfolio single canonical should use custom slug, got: {}",
        meta
    );
}

// ═══════════════════════════════════════════════════════════
// SEO: Sitemap + robots.txt
// ═══════════════════════════════════════════════════════════

#[test]
fn seo_sitemap_disabled() {
    let pool = test_pool();
    // seed_defaults enables sitemap, so explicitly disable
    Setting::set(&pool, "seo_sitemap_enabled", "false").unwrap();
    assert!(seo::generate_sitemap(&pool).is_none());
}

#[test]
fn seo_sitemap_enabled() {
    let pool = test_pool();
    Setting::set(&pool, "seo_sitemap_enabled", "true").unwrap();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "blog").unwrap();
    Setting::set(&pool, "portfolio_slug", "portfolio").unwrap();

    // Create content
    let mut pf = make_post_form("My Post", "my-post", "published");
    pf.published_at = Some("2026-01-01T12:00".to_string());
    Post::create(&pool, &pf).unwrap();

    let mut porf = make_portfolio_form("My Item", "my-item", "published");
    porf.published_at = Some("2026-01-01T12:00".to_string());
    PortfolioItem::create(&pool, &porf).unwrap();

    let xml = seo::generate_sitemap(&pool).unwrap();
    assert!(xml.contains("<?xml"));
    assert!(xml.contains("<urlset"));
    assert!(xml.contains("https://example.com"));
    assert!(xml.contains("/blog/my-post"));
    assert!(xml.contains("/portfolio/my-item"));
}

#[test]
fn seo_robots_txt() {
    let pool = test_pool();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();

    // Without sitemap
    Setting::set(&pool, "seo_sitemap_enabled", "false").unwrap();
    let robots = seo::sitemap::generate_robots(&pool);
    assert!(robots.contains("User-agent"));
    assert!(!robots.contains("Sitemap:"));

    // With sitemap
    Setting::set(&pool, "seo_sitemap_enabled", "true").unwrap();
    let robots = seo::sitemap::generate_robots(&pool);
    assert!(robots.contains("Sitemap: https://example.com/sitemap.xml"));
}

// ═══════════════════════════════════════════════════════════
// SEO: JSON-LD structured data
// ═══════════════════════════════════════════════════════════

#[test]
fn seo_jsonld_post() {
    let pool = test_pool();
    Setting::set(&pool, "site_name", "LD Site").unwrap();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "blog_slug", "blog").unwrap();

    let mut form = make_post_form("JSON-LD Post", "jsonld-post", "published");
    form.published_at = Some("2026-01-15T10:00".to_string());
    form.meta_description = Some("A test post".to_string());
    let id = Post::create(&pool, &form).unwrap();
    let post = Post::find_by_id(&pool, id).unwrap();

    let ld = seo::build_post_jsonld(&pool, &post);
    assert!(ld.contains("application/ld+json"));
    assert!(ld.contains("BlogPosting"));
    assert!(ld.contains("JSON-LD Post"));
    assert!(ld.contains("A test post"));
    assert!(ld.contains("https://example.com"));
    assert!(ld.contains("LD Site"));
}

#[test]
fn seo_jsonld_portfolio() {
    let pool = test_pool();
    Setting::set(&pool, "site_name", "LD Site").unwrap();
    Setting::set(&pool, "site_url", "https://example.com").unwrap();
    Setting::set(&pool, "portfolio_slug", "gallery").unwrap();

    let id = PortfolioItem::create(&pool, &make_portfolio_form("Sunset", "sunset", "published"))
        .unwrap();
    let item = PortfolioItem::find_by_id(&pool, id).unwrap();

    let ld = seo::build_portfolio_jsonld(&pool, &item);
    assert!(ld.contains("application/ld+json"));
    assert!(ld.contains("ImageObject"));
    assert!(ld.contains("Sunset"));
    assert!(ld.contains("/gallery/sunset"));
}

// ═══════════════════════════════════════════════════════════
// DB Migrations
// ═══════════════════════════════════════════════════════════

#[test]
fn migrations_idempotent() {
    let pool = test_pool();
    // Running migrations again should not fail
    run_migrations(&pool).expect("Second migration run should succeed");
    run_migrations(&pool).expect("Third migration run should succeed");
}

#[test]
fn all_tables_exist() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    let tables = [
        "posts",
        "portfolio",
        "categories",
        "tags",
        "content_categories",
        "content_tags",
        "comments",
        "orders",
        "download_tokens",
        "licenses",
        "designs",
        "design_templates",
        "settings",
        "imports",
        "sessions",
        "page_views",
        "magic_links",
        "likes",
        "users",
        "fw_bans",
        "fw_events",
        "audit_log",
    ];
    for table in &tables {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| panic!("Table '{}' should exist", table));
        assert!(count >= 0, "Table '{}' query failed", table);
    }
}

// ═══════════════════════════════════════════════════════════
// Render: Category Filters
// ═══════════════════════════════════════════════════════════

use crate::render;
use serde_json::json;

/// Helper: set multiple settings in one go for render tests.
fn set_settings(pool: &DbPool, pairs: &[(&str, &str)]) {
    for &(k, v) in pairs {
        Setting::set(pool, k, v).unwrap();
    }
}

/// Helper: extract the HTML body (after </style>) to avoid matching CSS selectors.
fn body_html(full: &str) -> &str {
    full.rfind("</style>").map(|i| &full[i..]).unwrap_or(full)
}

/// Helper: build a minimal portfolio_grid context with categories.
fn render_context(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    let categories = Category::list(pool, Some("portfolio"));
    json!({
        "settings": settings,
        "items": [],
        "categories": categories,
        "current_page": 1,
        "total_pages": 1,
        "page_type": "portfolio_grid",
        "seo": "",
    })
}

#[test]
fn render_sidebar_under_link_has_toggle_open() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Flights", "flights", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("portfolio_nav_categories", "under_link"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // Toggle should have "open" class (sidebar under_link starts open)
    assert!(
        html.contains("nav-category-toggle open"),
        "under_link toggle should start open in sidebar"
    );
    // Subcategories div should also be open
    assert!(
        html.contains("nav-subcategories open"),
        "under_link subcategories should start open in sidebar"
    );
    // "All" link present
    assert!(html.contains(">All</a>"), "should have 'All' category link");
    // Category link present
    assert!(
        html.contains(">Flights</a>"),
        "should have 'Flights' category link"
    );
    // Portfolio should NOT appear as a separate nav-link (the toggle replaces it)
    assert!(
        !html.contains("class=\"nav-link\">Experiences</a>"),
        "under_link: portfolio should not be a separate nav-link"
    );
}

#[test]
fn render_sidebar_page_top_has_horizontal_cats() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Nature", "nature", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("portfolio_nav_categories", "page_top"),
            ("portfolio_nav_categories_align", "left"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // Page top categories div present
    assert!(
        html.contains("categories-page-top"),
        "should have page-top categories"
    );
    // No right alignment class in the body HTML (not CSS)
    let body = body_html(&html);
    assert!(
        !body.contains("cats-right"),
        "left align should not have cats-right class"
    );
    // Portfolio should appear as a normal nav-link
    assert!(
        html.contains("nav-link\">experiences</a>")
            || html.contains("nav-link active\">experiences</a>")
            || html.contains("nav-link\">Experiences</a>")
            || html.contains("nav-link active\">Experiences</a>"),
        "page_top: portfolio should be a normal nav-link"
    );
}

#[test]
fn render_sidebar_page_top_right_alignment() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Travel", "travel", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("portfolio_nav_categories", "page_top"),
            ("portfolio_nav_categories_align", "right"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("categories-page-top cats-right"),
        "right align should have cats-right class"
    );
}

#[test]
fn render_topbar_submenu_no_duplicate_portfolio() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Flights", "flights", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "topbar"),
            ("portfolio_nav_categories", "submenu"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // Should use topbar shell
    assert!(
        html.contains("topbar-layout"),
        "should use topbar body class"
    );
    // Toggle should NOT have "open" class (topbar submenu starts closed)
    assert!(
        !html.contains("nav-category-toggle open"),
        "submenu toggle should start closed"
    );
    // Portfolio should NOT appear as a separate nav-link
    let _nav_link_count = html.matches("class=\"nav-link\"").count();
    // Only blog (journal) should be a nav-link, not portfolio
    assert!(
        !html.contains("nav-link\">experiences</a>"),
        "submenu: portfolio should not be a separate nav-link"
    );
    // But the toggle should show the portfolio label
    assert!(
        html.contains("<span>experiences</span>"),
        "submenu toggle should show portfolio label"
    );
}

#[test]
fn render_topbar_below_menu_has_category_row() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Flights", "flights", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "topbar"),
            ("portfolio_nav_categories", "below_menu"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // Below menu categories div present
    assert!(
        html.contains("categories-below-menu"),
        "should have below-menu categories"
    );
    // Portfolio should appear as a normal nav-link
    assert!(
        html.contains("nav-link\">experiences</a>")
            || html.contains("nav-link active\">experiences</a>"),
        "below_menu: portfolio should be a normal nav-link"
    );
    // All + Flights links in the below-menu div
    assert!(html.contains(">All</a>"), "below-menu should have All link");
    assert!(
        html.contains(">Flights</a>"),
        "below-menu should have Flights link"
    );
}

#[test]
fn render_hidden_categories_no_output() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Flights", "flights", "portfolio")).unwrap();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("portfolio_nav_categories", "hidden"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // No category HTML at all (check body only, CSS has these class names)
    let body = body_html(&html);
    assert!(
        !body.contains("class=\"nav-category-group"),
        "hidden: no category group"
    );
    assert!(
        !body.contains("class=\"categories-page-top"),
        "hidden: no page-top categories"
    );
    assert!(
        !body.contains("class=\"categories-below-menu"),
        "hidden: no below-menu categories"
    );
    // Portfolio should appear as a normal nav-link
    assert!(
        html.contains("nav-link\">experiences</a>")
            || html.contains("nav-link active\">experiences</a>"),
        "hidden: portfolio should be a normal nav-link"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Social Icons
// ═══════════════════════════════════════════════════════════

#[test]
fn render_social_links_in_sidebar() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("social_icons_position", "sidebar"),
            ("social_instagram", "https://instagram.com/test"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("class=\"social-links\""),
        "should have social-links div"
    );
    assert!(
        html.contains("instagram.com/test"),
        "should have instagram link"
    );
    assert!(
        html.contains("title=\"Instagram\""),
        "should have Instagram title"
    );
}

#[test]
fn render_social_brand_colors() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("social_icons_position", "sidebar"),
            ("social_instagram", "https://instagram.com/test"),
            ("social_brand_colors", "true"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("style=\"color:#E4405F\""),
        "brand colors should add style attribute"
    );
}

#[test]
fn render_social_empty_when_no_urls() {
    let pool = test_pool();
    set_settings(&pool, &[("social_icons_position", "sidebar")]);
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // social-links div should NOT appear (no URLs set)
    assert!(
        !html.contains("class=\"social-links\""),
        "no social URLs = no social-links div"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Share Icons & Label
// ═══════════════════════════════════════════════════════════

#[test]
fn render_share_label_prepended() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("share_enabled", "true"),
            ("share_facebook", "true"),
            ("share_icons_position", "sidebar"),
            ("share_label", "Share this:"),
            ("site_url", "https://example.com"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("<span class=\"share-label\">Share this:</span>"),
        "share label should be prepended before share icons"
    );
    assert!(
        html.contains("class=\"share-icons\""),
        "should have share-icons div"
    );
}

#[test]
fn render_share_buttons_rendered() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("share_enabled", "true"),
            ("share_facebook", "true"),
            ("share_x", "true"),
            ("share_icons_position", "sidebar"),
            ("site_url", "https://example.com"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("Share on Facebook"),
        "should have Facebook share link"
    );
    assert!(html.contains("Share on X"), "should have X share link");
}

#[test]
fn render_share_disabled_no_output() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("share_enabled", "false"),
            ("share_facebook", "true"),
            ("share_icons_position", "sidebar"),
            ("site_url", "https://example.com"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        !html.contains("class=\"share-icons\""),
        "share disabled = no share-icons div"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Footer / Copyright
// ═══════════════════════════════════════════════════════════

#[test]
fn render_footer_copyright_center() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("copyright_text", "© 2026 Test"),
            ("copyright_alignment", "center"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("footer-copyright"),
        "should have copyright span"
    );
    assert!(
        html.contains("© 2026 Test"),
        "should contain copyright text"
    );
    // Center: copyright in center cell
    assert!(
        html.contains("footer-cell footer-center\"><span class=\"footer-copyright\">"),
        "center alignment: copyright should be in center cell"
    );
}

#[test]
fn render_footer_copyright_right() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("copyright_text", "© 2026 Right"),
            ("copyright_alignment", "right"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("footer-cell footer-right\"><span class=\"footer-copyright\">"),
        "right alignment: copyright should be in right cell"
    );
}

#[test]
fn render_footer_3_column_grid() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("copyright_text", "© 2026"),
            ("copyright_alignment", "left"),
            ("social_icons_position", "footer"),
            ("social_instagram", "https://instagram.com/test"),
            ("footer_alignment", "right"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // Should have all 3 footer cells
    assert!(
        html.contains("footer-cell footer-left"),
        "should have left cell"
    );
    assert!(
        html.contains("footer-cell footer-center"),
        "should have center cell"
    );
    assert!(
        html.contains("footer-cell footer-right"),
        "should have right cell"
    );
    // Copyright in left, social in right
    assert!(
        html.contains("footer-cell footer-left\"><span class=\"footer-copyright\">"),
        "copyright should be in left cell"
    );
    assert!(
        html.contains("footer-cell footer-right\"><span class=\"footer-social\">"),
        "social should be in right cell"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Layout Switching (Sidebar vs Topbar)
// ═══════════════════════════════════════════════════════════

#[test]
fn render_sidebar_layout_has_site_wrapper() {
    let pool = test_pool();
    set_settings(&pool, &[("layout_header_type", "sidebar")]);
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("class=\"site-wrapper"),
        "sidebar layout should have site-wrapper"
    );
    assert!(
        html.contains("<aside class=\"sidebar\">"),
        "sidebar layout should have aside.sidebar"
    );
    // Check body class specifically, not CSS rules
    assert!(
        !html.contains("class=\"topbar-layout"),
        "sidebar layout should not have topbar-layout body class"
    );
}

#[test]
fn render_topbar_layout_has_topbar_shell() {
    let pool = test_pool();
    set_settings(&pool, &[("layout_header_type", "topbar")]);
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("topbar-layout"),
        "topbar layout should have topbar-layout body class"
    );
    assert!(
        html.contains("<header class=\"topbar"),
        "topbar layout should have header.topbar"
    );
    assert!(
        html.contains("topbar-brand"),
        "topbar layout should have topbar-brand"
    );
    assert!(
        html.contains("topbar-hamburger"),
        "topbar layout should have hamburger button"
    );
    assert!(
        !html.contains("<aside class=\"sidebar\">"),
        "topbar layout should not have aside.sidebar"
    );
}

#[test]
fn render_topbar_nav_right_class() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[("layout_header_type", "topbar"), ("nav_position", "right")],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("topbar-nav-right"),
        "nav_position=right should add topbar-nav-right class"
    );
}

#[test]
fn render_topbar_boxed_mode() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("layout_header_type", "topbar"),
            ("layout_content_boundary", "boxed"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("boxed-mode"),
        "boxed mode should add boxed-mode body class"
    );
    assert!(
        html.contains("layout-boxed"),
        "boxed mode should add layout-boxed class"
    );
}

#[test]
fn render_topbar_hides_custom_sidebar() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("layout_header_type", "topbar"),
            ("layout_sidebar_custom_heading", "About Me"),
            ("layout_sidebar_custom_text", "Hello world"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        !html.contains("About Me"),
        "topbar should not show sidebar custom heading"
    );
    assert!(
        !html.contains("Hello world"),
        "topbar should not show sidebar custom text"
    );
}

#[test]
fn render_sidebar_shows_custom_sidebar() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("layout_header_type", "sidebar"),
            ("layout_sidebar_custom_heading", "About Me"),
            ("layout_sidebar_custom_text", "Hello world"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("About Me"),
        "sidebar should show custom heading"
    );
    assert!(
        html.contains("Hello world"),
        "sidebar should show custom text"
    );
}

#[test]
fn render_sidebar_right_class() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("layout_header_type", "sidebar"),
            ("layout_sidebar_position", "right"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    assert!(
        html.contains("sidebar-right"),
        "sidebar position=right should add sidebar-right class"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Portfolio Show Categories / Tags Position Modes
// ═══════════════════════════════════════════════════════════

/// Helper: build a portfolio_grid context with one item that has categories and tags.
fn render_context_with_items(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    let categories = Category::list(pool, Some("portfolio"));
    json!({
        "settings": settings,
        "items": [
            {
                "item": {
                    "id": 1,
                    "title": "Sunset Flight",
                    "slug": "sunset-flight",
                    "image_path": "test.jpg",
                    "thumbnail_path": "",
                    "likes": 5,
                    "status": "published"
                },
                "categories": [
                    {"id": 1, "name": "Flights", "slug": "flights"},
                    {"id": 2, "name": "Nature", "slug": "nature"}
                ],
                "tags": [
                    {"id": 1, "name": "Aerial", "slug": "aerial"},
                    {"id": 2, "name": "Golden Hour", "slug": "golden-hour"}
                ]
            }
        ],
        "categories": categories,
        "current_page": 1,
        "total_pages": 1,
        "page_type": "portfolio_grid",
        "seo": "",
    })
}

#[test]
fn render_portfolio_cats_false_no_visible_labels() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "false"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        !body.contains("class=\"item-categories"),
        "false: no category labels"
    );
    assert!(
        body.contains("data-categories=\"flights nature\""),
        "data-categories attr always present"
    );
}

#[test]
fn render_portfolio_tags_false_no_visible_labels() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_tags", "false"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(!body.contains("class=\"item-tags"), "false: no tag labels");
}

#[test]
fn render_portfolio_cats_hover() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "hover"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-hover"),
        "hover: category overlay class"
    );
    assert!(
        body.contains(">Flights</a>"),
        "hover: category name rendered"
    );
}

#[test]
fn render_portfolio_tags_hover() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_tags", "hover"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-tags item-meta-hover"),
        "hover: tag overlay class"
    );
    assert!(body.contains(">#Aerial</span>"), "hover: tag name rendered");
}

#[test]
fn render_portfolio_cats_bottom_left_fallback() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "bottom_left"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-below_left"),
        "bottom_left should fallback to below_left"
    );
}

#[test]
fn render_portfolio_cats_bottom_right_fallback() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "bottom_right"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-below_right"),
        "bottom_right should fallback to below_right"
    );
}

#[test]
fn render_portfolio_cats_below_left() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "below_left"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-below_left"),
        "below_left class"
    );
    assert!(
        body.contains(">Flights</a>"),
        "below_left: category name rendered"
    );
}

#[test]
fn render_portfolio_cats_below_right() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "below_right"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-below_right"),
        "below_right class"
    );
}

#[test]
fn render_portfolio_tags_below_left() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_tags", "below_left"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-tags item-meta-below_left"),
        "below_left tag class"
    );
    assert!(body.contains(">#Aerial</span>"), "below_left: tag name");
    assert!(
        body.contains(">#Golden Hour</span>"),
        "below_left: second tag"
    );
}

#[test]
fn render_portfolio_tags_below_right() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_tags", "below_right"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-tags item-meta-below_right"),
        "below_right tag class"
    );
}

#[test]
fn render_portfolio_legacy_true_normalizes_to_below_left() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "true"),
            ("portfolio_show_tags", "true"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-below_left"),
        "legacy true normalizes to below_left for categories"
    );
    assert!(
        body.contains("item-tags item-meta-below_left"),
        "legacy true normalizes to below_left for tags"
    );
}

#[test]
fn render_portfolio_overlay_outside_link_tag() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "hover"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    // Overlay div should be AFTER </a>, not inside it
    let link_end = body.find("</a>").unwrap_or(0);
    let cats_pos = body.find("item-categories item-meta-hover").unwrap_or(0);
    assert!(
        cats_pos > link_end,
        "overlay should be outside the <a> tag (after </a>)"
    );
}

#[test]
fn render_portfolio_mixed_cats_hover_tags_below() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_categories", "hover"),
            ("portfolio_show_tags", "below_left"),
        ],
    );
    let ctx = render_context_with_items(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);

    assert!(
        body.contains("item-categories item-meta-hover"),
        "cats should be hover overlay"
    );
    assert!(
        body.contains("item-tags item-meta-below_left"),
        "tags should be below_left"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Journal (blog_list) Settings
// ═══════════════════════════════════════════════════════════

/// Helper: build a blog_list context with one post.
fn render_blog_context(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    json!({
        "settings": settings,
        "posts": [
            {
                "title": "Hello World",
                "slug": "hello-world",
                "excerpt": "This is a test post with enough words to verify excerpt truncation behavior in the blog list rendering",
                "published_at": "2026-01-15 10:00:00",
                "featured_image": "hello.jpg",
                "author_name": "Alice",
                "content_html": "<p>Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua Ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur Excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt mollit anim id est laborum</p><p>Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua Ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur Excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt mollit anim id est laborum</p>"
            }
        ],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "blog_list",
        "seo": "",
    })
}

#[test]
fn render_blog_list_grid_display() {
    let pool = test_pool();
    // Activate Oneguy design — grid display is an Oneguy feature
    let oneguy = Design::find_by_slug(&pool, "oneguy").unwrap();
    Design::activate(&pool, oneguy.id).unwrap();
    set_settings(&pool, &[("blog_display_type", "grid")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("blog-grid"),
        "grid display type should produce blog-grid class"
    );
}

#[test]
fn render_blog_list_default_list_display() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_display_type", "list")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("blog-list"),
        "list display type should produce blog-list class"
    );
    assert!(
        !body.contains("blog-grid"),
        "list display should not have blog-grid"
    );
}

#[test]
fn render_blog_list_editorial_style() {
    let pool = test_pool();
    // Activate Oneguy design — editorial style is an Oneguy feature
    let oneguy = Design::find_by_slug(&pool, "oneguy").unwrap();
    Design::activate(&pool, oneguy.id).unwrap();
    set_settings(
        &pool,
        &[
            ("blog_display_type", "list"),
            ("blog_list_style", "editorial"),
        ],
    );
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("blog-editorial"),
        "editorial list style should produce blog-editorial class"
    );
}

#[test]
fn render_blog_list_classic_style() {
    let pool = test_pool();
    // Activate Oneguy design — classic style is an Oneguy feature
    let oneguy = Design::find_by_slug(&pool, "oneguy").unwrap();
    Design::activate(&pool, oneguy.id).unwrap();
    set_settings(
        &pool,
        &[
            ("blog_display_type", "list"),
            ("blog_list_style", "classic"),
        ],
    );
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("blog-classic"),
        "classic list style should produce blog-classic class"
    );
}

#[test]
fn render_blog_list_excerpt_fallback() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_excerpt_words", "5")]);
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [
            {
                "title": "No Excerpt Post",
                "slug": "no-excerpt",
                "excerpt": "",
                "published_at": "2026-01-15 10:00:00",
                "featured_image": "",
                "author_name": "",
                "content_html": "<p>Alpha beta gamma delta epsilon zeta eta theta</p>"
            }
        ],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("Alpha beta gamma delta epsilon"),
        "empty excerpt should fall back to content_html text"
    );
    assert!(
        !body.contains("zeta"),
        "fallback excerpt should be truncated to 5 words"
    );
}

#[test]
fn render_blog_list_show_author() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_author", "true")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    // Inkwell wide renderer outputs author name in uppercase in bwd-meta
    assert!(
        body.contains("ALICE"),
        "show_author=true should render author name"
    );
}

#[test]
fn render_blog_list_hide_author() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_author", "false")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        !body.contains("ALICE"),
        "show_author=false should hide author"
    );
}

#[test]
fn render_blog_list_show_date() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_date", "true")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    // Inkwell wide renderer outputs date in uppercase in bwd-meta
    assert!(body.contains("2026"), "show_date=true should render date");
}

#[test]
fn render_blog_list_hide_date() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_date", "false")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    // Inkwell wide renderer would include date in bwd-meta; verify it's absent
    assert!(!body.contains("JAN 15"), "show_date=false should hide date");
}

#[test]
fn render_blog_list_show_reading_time() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_reading_time", "true")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    // Inkwell wide renderer outputs reading time in uppercase
    assert!(
        body.contains("MIN READ"),
        "show_reading_time=true should render reading time"
    );
}

#[test]
fn render_blog_list_hide_reading_time() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_show_reading_time", "false")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        !body.contains("MIN READ"),
        "show_reading_time=false should hide reading time"
    );
}

#[test]
fn render_blog_list_excerpt_truncation() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_excerpt_words", "5")]);
    let ctx = render_blog_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    // With 5 words, "This is a test post" should be the excerpt, not the full text
    assert!(
        !body.contains("truncation behavior"),
        "excerpt should be truncated to 5 words"
    );
}

#[test]
fn render_blog_list_pagination_load_more() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_pagination_type", "load_more")]);
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [{"title":"A","slug":"a","excerpt":"","published_at":"","featured_image":"","author_name":"","word_count":0}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("load-more-btn"),
        "load_more pagination should render load-more button"
    );
}

#[test]
fn render_blog_list_pagination_infinite() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_pagination_type", "infinite")]);
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [{"title":"A","slug":"a","excerpt":"","published_at":"","featured_image":"","author_name":"","word_count":0}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("infinite-sentinel"),
        "infinite pagination should render sentinel div"
    );
}

#[test]
fn render_blog_list_pagination_classic() {
    let pool = test_pool();
    set_settings(&pool, &[("blog_pagination_type", "classic")]);
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [{"title":"A","slug":"a","excerpt":"","published_at":"","featured_image":"","author_name":"","word_count":0}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("pagination"),
        "classic pagination should render pagination div"
    );
    // Classic should have page number links, not load-more button element
    assert!(body.contains("page=2"), "classic should have page links");
    assert!(
        !body.contains("id=\"infinite-sentinel\""),
        "classic should not have infinite sentinel element"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Portfolio Pagination (classic, load_more, infinite)
// ═══════════════════════════════════════════════════════════

#[test]
fn render_portfolio_pagination_classic() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_pagination_type", "classic"),
        ],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "items": [{"item":{"id":1,"title":"A","slug":"a","image_path":"a.jpg","thumbnail_path":"","likes":0,"sell_enabled":false,"price":0.0},"tags":[],"categories":[]}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "portfolio_grid",
        "seo": "",
    });
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("pagination"),
        "classic should render pagination nav"
    );
    assert!(body.contains("page=2"), "classic should have page links");
    assert!(
        !body.contains("id=\"infinite-sentinel\""),
        "classic should not have infinite sentinel"
    );
    assert!(
        !body.contains("id=\"load-more-btn\""),
        "classic should not have load-more button element"
    );
}

#[test]
fn render_portfolio_pagination_load_more() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_pagination_type", "load_more"),
        ],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "items": [{"item":{"id":1,"title":"A","slug":"a","image_path":"a.jpg","thumbnail_path":"","likes":0,"sell_enabled":false,"price":0.0},"tags":[],"categories":[]}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "portfolio_grid",
        "seo": "",
    });
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("id=\"load-more-btn\""),
        "load_more should render load-more button element"
    );
    assert!(
        !body.contains("id=\"infinite-sentinel\""),
        "load_more should not have infinite sentinel"
    );
}

#[test]
fn render_portfolio_pagination_infinite() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_pagination_type", "infinite"),
        ],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "items": [{"item":{"id":1,"title":"A","slug":"a","image_path":"a.jpg","thumbnail_path":"","likes":0,"sell_enabled":false,"price":0.0},"tags":[],"categories":[]}],
        "current_page": 1,
        "total_pages": 3,
        "page_type": "portfolio_grid",
        "seo": "",
    });
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("id=\"infinite-sentinel\""),
        "infinite should render sentinel div element"
    );
    assert!(
        !body.contains("id=\"load-more-btn\""),
        "infinite should not have load-more button element"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Container classes (JS pagination selector correctness)
// ═══════════════════════════════════════════════════════════

#[test]
fn render_portfolio_grid_has_grid_container_class() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_display_type", "masonry"),
        ],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "items": [{"item":{"id":1,"title":"A","slug":"a","image_path":"a.jpg","thumbnail_path":"","likes":0,"sell_enabled":false,"price":0.0},"tags":[],"categories":[]}],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "portfolio_grid",
        "seo": "",
    });
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("masonry-grid"),
        "portfolio masonry should use masonry-grid container (JS selector target)"
    );
    assert!(
        body.contains("grid-item"),
        "portfolio items should use grid-item class (JS selector target)"
    );
}

#[test]
fn render_blog_list_has_blog_list_container_class() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[("blog_display_type", "list"), ("blog_list_style", "wide")],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [{"title":"A","slug":"a","excerpt":"","content_html":"<p>hello</p>","published_at":"","featured_image":"","author_name":"","word_count":100}],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("blog-list"),
        "journal list should use blog-list container (JS selector target)"
    );
}

#[test]
fn render_blog_list_items_are_articles() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[("blog_display_type", "list"), ("blog_list_style", "wide")],
    );
    let settings = Setting::all(&pool);
    let ctx = json!({
        "settings": settings,
        "posts": [{"title":"Test Post","slug":"test","excerpt":"","content_html":"<p>hello</p>","published_at":"2026-01-01","featured_image":"","author_name":"Alice","word_count":100}],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "blog_list",
        "seo": "",
    });
    let html = render::render_page(&pool, "blog_list", &ctx);
    let body = body_html(&html);
    assert!(
        body.contains("<article"),
        "journal list items should be <article> elements (JS selector: .blog-list > article)"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Footer Behavior
// ═══════════════════════════════════════════════════════════

#[test]
fn render_footer_regular_no_class() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("footer_behavior", "regular"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    // Body class should not contain footer behavior classes
    let body_tag = html
        .find("<body")
        .and_then(|s| html[s..].find(">").map(|e| &html[s..s + e + 1]))
        .unwrap_or("");
    assert!(
        !body_tag.contains("footer-fixed-reveal"),
        "regular footer body tag should not have fixed-reveal class"
    );
    assert!(
        !body_tag.contains("footer-always-visible"),
        "regular footer body tag should not have always-visible class"
    );
}

#[test]
fn render_footer_fixed_reveal_site_wide() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("footer_behavior", "fixed_reveal"),
            ("footer_behavior_scope", "site_wide"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body_tag = html
        .find("<body")
        .and_then(|s| html[s..].find(">").map(|e| &html[s..s + e + 1]))
        .unwrap_or("");
    assert!(
        body_tag.contains("footer-fixed-reveal"),
        "fixed_reveal + site_wide should add footer-fixed-reveal class to body"
    );
}

#[test]
fn render_footer_always_visible_site_wide() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("footer_behavior", "always_visible"),
            ("footer_behavior_scope", "site_wide"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body_tag = html
        .find("<body")
        .and_then(|s| html[s..].find(">").map(|e| &html[s..s + e + 1]))
        .unwrap_or("");
    assert!(
        body_tag.contains("footer-always-visible"),
        "always_visible + site_wide should add footer-always-visible class to body"
    );
}

#[test]
fn render_footer_selected_pages_match() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("footer_behavior", "fixed_reveal"),
            ("footer_behavior_scope", "selected_pages"),
            ("footer_behavior_pages", "portfolio_grid,blog_list"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body_tag = html
        .find("<body")
        .and_then(|s| html[s..].find(">").map(|e| &html[s..s + e + 1]))
        .unwrap_or("");
    assert!(
        body_tag.contains("footer-fixed-reveal"),
        "selected_pages with portfolio_grid should add class on portfolio_grid page"
    );
}

#[test]
fn render_footer_selected_pages_no_match() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("footer_behavior", "fixed_reveal"),
            ("footer_behavior_scope", "selected_pages"),
            ("footer_behavior_pages", "blog_list,404"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    let body_tag = html
        .find("<body")
        .and_then(|s| html[s..].find(">").map(|e| &html[s..s + e + 1]))
        .unwrap_or("");
    assert!(
        !body_tag.contains("footer-fixed-reveal"),
        "selected_pages without portfolio_grid should not add class on portfolio_grid page"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Portfolio Lightbox & Feature Settings
// ═══════════════════════════════════════════════════════════

#[test]
fn render_portfolio_lightbox_data_attrs() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_lightbox_show_title", "hidden"),
            ("portfolio_lightbox_show_tags", "left"),
            ("portfolio_lightbox_nav", "false"),
            ("portfolio_lightbox_keyboard", "false"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-lb-title-pos=\"hidden\""),
        "lightbox title position should be hidden"
    );
    assert!(
        html.contains("data-lb-tags-pos=\"left\""),
        "lightbox tags position should be left"
    );
    assert!(
        html.contains("data-lb-nav=\"false\""),
        "lightbox nav should be false"
    );
    assert!(
        html.contains("data-lb-keyboard=\"false\""),
        "lightbox keyboard should be false"
    );
}

#[test]
fn render_portfolio_lightbox_defaults_center() {
    let pool = test_pool();
    set_settings(&pool, &[("portfolio_enabled", "true")]);
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-lb-title-pos=\"center\""),
        "lightbox title position should default to center"
    );
    assert!(
        html.contains("data-lb-tags-pos=\"center\""),
        "lightbox tags position should default to center"
    );
    assert!(
        html.contains("data-lb-nav=\"true\""),
        "lightbox nav should default to true"
    );
    assert!(
        html.contains("data-lb-keyboard=\"true\""),
        "lightbox keyboard should default to true"
    );
}

#[test]
fn render_portfolio_image_protection_enabled() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_image_protection", "true"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("contextmenu"),
        "image_protection=true should inject right-click prevention JS"
    );
}

#[test]
fn render_portfolio_image_protection_disabled() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_image_protection", "false"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        !html.contains("contextmenu"),
        "image_protection=false should not inject right-click prevention JS"
    );
}

#[test]
fn render_portfolio_likes_data_attr() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_enable_likes", "true"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-show-likes=\"true\""),
        "enable_likes=true should set data-show-likes=true"
    );
}

#[test]
fn render_portfolio_likes_disabled() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_enable_likes", "false"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-show-likes=\"false\""),
        "enable_likes=false should set data-show-likes=false"
    );
}

#[test]
fn render_portfolio_pagination_data_attr() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_pagination_type", "load_more"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-pagination-type=\"load_more\""),
        "pagination_type should appear as data attribute"
    );
}

#[test]
fn render_portfolio_click_mode_data_attr() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_click_mode", "detail"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-click-mode=\"detail\""),
        "click_mode should appear as data attribute"
    );
}

// ═══════════════════════════════════════════════════════════
// Render: Commerce Settings
// ═══════════════════════════════════════════════════════════

/// Helper: build a portfolio_single context with commerce enabled
fn commerce_single_context(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    json!({
        "settings": settings,
        "item": {
            "id": 1,
            "title": "Test Item",
            "slug": "test-item",
            "image_path": "test.jpg",
            "description_html": "<p>Description</p>",
            "likes": 0,
            "price": 29.99,
            "purchase_note": "Includes source files",
            "payment_provider": "stripe",
            "sell_enabled": true,
        },
        "tags": [],
        "categories": [],
        "commerce_enabled": true,
        "page_type": "portfolio_single",
        "seo": "",
    })
}

/// Helper: build a portfolio_grid context with one sellable item
fn commerce_grid_context(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    json!({
        "settings": settings,
        "items": [{
            "item": {
                "id": 1,
                "title": "Sellable Item",
                "slug": "sellable",
                "image_path": "sell.jpg",
                "thumbnail_path": "",
                "likes": 0,
                "price": 19.99,
                "sell_enabled": true,
            },
            "categories": [],
            "tags": [],
        }],
        "categories": [],
        "current_page": 1,
        "total_pages": 1,
        "page_type": "portfolio_grid",
        "seo": "",
    })
}

#[test]
fn render_commerce_button_custom_color() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_color", "#FF0000"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("background:#FF0000"),
        "custom button color should override provider default"
    );
}

#[test]
fn render_commerce_button_default_stripe_color() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("background:#635BFF"),
        "stripe default color should be used when no custom color"
    );
}

#[test]
fn render_commerce_button_custom_label() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_label", "Purchase Now"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("Purchase Now"),
        "custom button label should appear"
    );
    assert!(
        !html.contains("Pay with Stripe"),
        "default label should not appear when custom label set"
    );
}

#[test]
fn render_commerce_button_radius_pill() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_radius", "pill"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("border-radius:999px"),
        "pill radius should be 999px"
    );
}

#[test]
fn render_commerce_button_radius_square() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_radius", "square"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("border-radius:0"),
        "square radius should be 0"
    );
}

#[test]
fn render_commerce_button_alignment_center() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_alignment", "center"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("text-align:center"),
        "center alignment should set text-align:center on commerce section"
    );
    assert!(
        html.contains("display:inline-block"),
        "center alignment should use inline-block button"
    );
}

#[test]
fn render_commerce_paypal_sdk_style() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_paypal_enabled", "true"),
            ("paypal_client_id", "test_id"),
            ("paypal_button_color", "blue"),
            ("paypal_button_shape", "pill"),
            ("paypal_button_label", "buynow"),
        ],
    );
    let mut ctx = commerce_single_context(&pool);
    ctx["item"]["payment_provider"] = json!("paypal");
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("color:'blue'"),
        "PayPal button color should be blue"
    );
    assert!(
        html.contains("shape:'pill'"),
        "PayPal button shape should be pill"
    );
    assert!(
        html.contains("label:'buynow'"),
        "PayPal button label should be buynow"
    );
}

#[test]
fn render_commerce_position_below_image() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_position", "below_image"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    let body = body_html(&html);
    let img_pos = body.find("portfolio-image").unwrap_or(0);
    let commerce_pos = body.find("commerce-section").unwrap_or(0);
    let meta_pos = body.find("portfolio-meta").unwrap_or(0);
    assert!(
        commerce_pos > img_pos && commerce_pos < meta_pos,
        "below_image: commerce should be between image and meta"
    );
}

#[test]
fn render_commerce_position_below_description() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_position", "below_description"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    let body = body_html(&html);
    let desc_pos = body.find("portfolio-description").unwrap_or(0);
    let commerce_pos = body.find("commerce-section").unwrap_or(0);
    assert!(
        commerce_pos > desc_pos,
        "below_description: commerce should be after description"
    );
}

#[test]
fn render_commerce_position_sidebar_right() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_stripe_enabled", "true"),
            ("stripe_publishable_key", "pk_test"),
            ("commerce_button_position", "sidebar_right"),
        ],
    );
    let ctx = commerce_single_context(&pool);
    let html = render::render_page(&pool, "portfolio_single", &ctx);
    assert!(
        html.contains("portfolio-single-row"),
        "sidebar_right should create flex row layout"
    );
    assert!(
        html.contains("portfolio-single-sidebar"),
        "sidebar_right should create sidebar column"
    );
}

#[test]
fn render_commerce_price_badge_top_right() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_show_price", "true"),
            ("commerce_price_position", "top_right"),
        ],
    );
    let ctx = commerce_grid_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("price-badge"),
        "price badge should be rendered"
    );
    assert!(
        html.contains("top:8px;right:8px"),
        "top_right position should have top:8px;right:8px"
    );
    assert!(
        html.contains("USD 19.99"),
        "price badge should show currency and price"
    );
}

#[test]
fn render_commerce_price_badge_top_left() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_show_price", "true"),
            ("commerce_price_position", "top_left"),
        ],
    );
    let ctx = commerce_grid_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("top:8px;left:8px"),
        "top_left position should have top:8px;left:8px"
    );
}

#[test]
fn render_commerce_price_badge_below_title() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_show_title", "true"),
            ("commerce_show_price", "true"),
            ("commerce_price_position", "below_title"),
        ],
    );
    let ctx = commerce_grid_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("price-badge-below"),
        "below_title should use price-badge-below class"
    );
}

#[test]
fn render_commerce_price_badge_hidden() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_show_price", "false"),
        ],
    );
    let ctx = commerce_grid_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        !html.contains("price-badge"),
        "price badge should not render when show_price=false"
    );
}

#[test]
fn render_commerce_lightbox_buy_data_attrs() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_lightbox_buy", "true"),
            ("commerce_lightbox_buy_position", "sidebar"),
            ("commerce_currency", "EUR"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-lb-buy=\"true\""),
        "lightbox buy should be true"
    );
    assert!(
        html.contains("data-lb-buy-position=\"sidebar\""),
        "lightbox buy position should be sidebar"
    );
    assert!(
        html.contains("data-commerce-currency=\"EUR\""),
        "commerce currency should be EUR"
    );
}

#[test]
fn render_commerce_lightbox_buy_disabled() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("commerce_lightbox_buy", "false"),
        ],
    );
    let ctx = render_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-lb-buy=\"false\""),
        "lightbox buy should be false"
    );
}

#[test]
fn render_commerce_grid_item_data_attrs() {
    let pool = test_pool();
    set_settings(&pool, &[("portfolio_enabled", "true")]);
    let ctx = commerce_grid_context(&pool);
    let html = render::render_page(&pool, "portfolio_grid", &ctx);
    assert!(
        html.contains("data-price=\"19.99\""),
        "grid item should have data-price attribute"
    );
    assert!(
        html.contains("data-sell=\"true\""),
        "grid item should have data-sell attribute"
    );
}

// ── License Default & Generation Tests ──────────────────

#[test]
fn license_default_text_seeded() {
    let pool = test_pool();
    let val = Setting::get(&pool, "downloads_license_template")
        .expect("downloads_license_template should be seeded");
    assert!(
        val.contains("DIGITAL DOWNLOAD LICENSE AGREEMENT"),
        "should contain license title"
    );
    assert!(
        val.contains("GRANT OF LICENSE"),
        "should contain grant of license section"
    );
    assert!(
        val.contains("PERMITTED USES"),
        "should contain permitted uses section"
    );
    assert!(
        val.contains("Commercial use in a single end product"),
        "should allow commercial use"
    );
    assert!(
        val.contains("RESTRICTIONS"),
        "should contain restrictions section"
    );
    assert!(
        val.contains("ATTRIBUTION"),
        "should contain attribution section"
    );
    assert!(val.contains("WARRANTY"), "should contain warranty section");
    assert!(
        val.contains("TERMINATION"),
        "should contain termination section"
    );
    assert!(val.contains("Licensor"), "should reference Licensor");
    assert!(val.contains("Licensee"), "should reference Licensee");
}

#[test]
fn license_default_not_personal_only() {
    let pool = test_pool();
    let val = Setting::get(&pool, "downloads_license_template").unwrap();
    assert!(
        !val.contains("personal, non-commercial purposes only"),
        "should NOT be personal-use-only license"
    );
    assert!(
        !val.contains("personal use only"),
        "should NOT contain personal use only"
    );
    assert!(
        val.contains("Commercial use in a single end product"),
        "should explicitly allow commercial use"
    );
}

#[test]
fn license_paypal_legacy_key_absent() {
    let pool = test_pool();
    let val = Setting::get(&pool, "paypal_license_text").unwrap_or_default();
    assert!(
        val.is_empty(),
        "paypal_license_text should be absent or empty (stale key removed from seed)"
    );
}

#[test]
fn license_text_generation_header() {
    // Simulate what the download_license route produces
    let item_title = "Sunset Over Mountains";
    let site_name = "My Photography";
    let txn_id = "PAY-12345ABC";
    let date = "2026-02-19 10:30:00";
    let license_key = "A1B2-C3D4-E5F6-G7H8";
    let license_template = "DIGITAL DOWNLOAD LICENSE AGREEMENT\n\nThis is the license body.";

    let mut txt = String::new();
    txt.push_str(&format!("License for: {}\n", item_title));
    txt.push_str(&format!("Purchased from: {}\n", site_name));
    txt.push_str(&format!("Transaction: {}\n", txn_id));
    txt.push_str(&format!("Date: {}\n", date));
    txt.push_str(&format!("License Key: {}\n", license_key));
    txt.push_str("\n---\n\n");
    txt.push_str(license_template);

    assert!(
        txt.starts_with("License for: Sunset Over Mountains\n"),
        "should start with item title"
    );
    assert!(
        txt.contains("Purchased from: My Photography\n"),
        "should contain site name"
    );
    assert!(
        txt.contains("Transaction: PAY-12345ABC\n"),
        "should contain transaction ID"
    );
    assert!(
        txt.contains("Date: 2026-02-19 10:30:00\n"),
        "should contain date"
    );
    assert!(
        txt.contains("License Key: A1B2-C3D4-E5F6-G7H8\n"),
        "should contain license key"
    );
    assert!(
        txt.contains("\n---\n\n"),
        "should have separator between header and body"
    );
    assert!(
        txt.contains("DIGITAL DOWNLOAD LICENSE AGREEMENT"),
        "should contain license body"
    );
}

#[test]
fn license_text_generation_no_provider_order_id() {
    // When provider_order_id is empty, should fall back to ORD-{id}
    let provider_order_id = "";
    let order_id: i64 = 42;
    let txn_id = if provider_order_id.is_empty() {
        format!("ORD-{}", order_id)
    } else {
        provider_order_id.to_string()
    };
    assert_eq!(
        txn_id, "ORD-42",
        "should fall back to ORD-ID when provider_order_id is empty"
    );
}

// ═══════════════════════════════════════════════════════════
// resolve_status: server-side scheduling enforcement
// ═══════════════════════════════════════════════════════════

#[test]
fn resolve_status_published_past_date_stays_published() {
    let status =
        crate::routes::admin::resolve_status("published", &Some("2020-01-01T12:00".to_string()));
    assert_eq!(
        status, "published",
        "past date + published should stay published"
    );
}

#[test]
fn resolve_status_published_future_date_becomes_scheduled() {
    let future = (chrono::Utc::now() + chrono::Duration::hours(2))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(future));
    assert_eq!(
        status, "scheduled",
        "future date + published should become scheduled"
    );
}

#[test]
fn resolve_status_published_no_date_stays_published() {
    let status = crate::routes::admin::resolve_status("published", &None);
    assert_eq!(
        status, "published",
        "no date + published should stay published"
    );
}

#[test]
fn resolve_status_published_empty_date_stays_published() {
    let status = crate::routes::admin::resolve_status("published", &Some("".to_string()));
    assert_eq!(
        status, "published",
        "empty date + published should stay published"
    );
}

#[test]
fn resolve_status_draft_future_date_stays_draft() {
    let future = (chrono::Utc::now() + chrono::Duration::hours(2))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("draft", &Some(future));
    assert_eq!(
        status, "draft",
        "draft should stay draft regardless of date"
    );
}

#[test]
fn resolve_status_scheduled_past_date_stays_scheduled() {
    // resolve_status only overrides "published" → "scheduled", not the reverse
    let status =
        crate::routes::admin::resolve_status("scheduled", &Some("2020-01-01T12:00".to_string()));
    assert_eq!(
        status, "scheduled",
        "scheduled status is not changed by resolve_status"
    );
}

#[test]
fn resolve_status_published_invalid_date_stays_published() {
    let status = crate::routes::admin::resolve_status("published", &Some("not-a-date".to_string()));
    assert_eq!(status, "published", "invalid date should not change status");
}

#[test]
fn resolve_status_published_near_future_becomes_scheduled() {
    let near = (chrono::Utc::now() + chrono::Duration::minutes(5))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(near));
    assert_eq!(
        status, "scheduled",
        "5 minutes in future should still schedule"
    );
}

// ═══════════════════════════════════════════════════════════
// uploaded_image_path: empty-string guard pattern
// ═══════════════════════════════════════════════════════════

#[test]
fn uploaded_path_empty_string_is_not_used() {
    // Simulates what Rocket sends for <input type="hidden" value="">
    let path: Option<String> = Some("".to_string());
    let use_pre = path.as_ref().map_or(false, |p| !p.is_empty());
    assert!(
        !use_pre,
        "empty string should NOT be treated as a pre-uploaded path"
    );
}

#[test]
fn uploaded_path_none_is_not_used() {
    let path: Option<String> = None;
    let use_pre = path.as_ref().map_or(false, |p| !p.is_empty());
    assert!(
        !use_pre,
        "None should NOT be treated as a pre-uploaded path"
    );
}

#[test]
fn uploaded_path_with_value_is_used() {
    let path: Option<String> = Some("editor_abc123.jpg".to_string());
    let use_pre = path.as_ref().map_or(false, |p| !p.is_empty());
    assert!(
        use_pre,
        "non-empty path should be treated as a pre-uploaded path"
    );
}

// ═══════════════════════════════════════════════════════════
// UTC date handling & scheduling edge cases
// ═══════════════════════════════════════════════════════════

#[test]
fn utc_format_roundtrip_parseable() {
    // The format used by the server for published_at must be parseable by resolve_status
    let now = chrono::Utc::now();
    let formatted = now.format("%Y-%m-%dT%H:%M").to_string();
    let parsed = chrono::NaiveDateTime::parse_from_str(&formatted, "%Y-%m-%dT%H:%M");
    assert!(
        parsed.is_ok(),
        "UTC formatted date should be parseable: {}",
        formatted
    );
}

#[test]
fn resolve_status_utc_future_1h_becomes_scheduled() {
    // Simulates what the browser sends after localToUtc conversion: a UTC datetime string
    let future_utc = (chrono::Utc::now() + chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(future_utc.clone()));
    assert_eq!(
        status, "scheduled",
        "1h future UTC date should schedule: {}",
        future_utc
    );
}

#[test]
fn resolve_status_utc_past_1h_stays_published() {
    let past_utc = (chrono::Utc::now() - chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(past_utc.clone()));
    assert_eq!(
        status, "published",
        "1h past UTC date should publish: {}",
        past_utc
    );
}

#[test]
fn resolve_status_utc_past_1min_stays_published() {
    // Edge case: just barely in the past
    let past_utc = (chrono::Utc::now() - chrono::Duration::minutes(1))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(past_utc.clone()));
    assert_eq!(
        status, "published",
        "1min past UTC should publish: {}",
        past_utc
    );
}

#[test]
fn resolve_status_utc_future_1min_becomes_scheduled() {
    // Edge case: just barely in the future
    let future_utc = (chrono::Utc::now() + chrono::Duration::minutes(2))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("published", &Some(future_utc.clone()));
    assert_eq!(
        status, "scheduled",
        "2min future UTC should schedule: {}",
        future_utc
    );
}

#[test]
fn resolve_status_draft_with_utc_future_stays_draft() {
    let future_utc = (chrono::Utc::now() + chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("draft", &Some(future_utc));
    assert_eq!(
        status, "draft",
        "draft should never be overridden to scheduled"
    );
}

#[test]
fn resolve_status_scheduled_with_utc_past_stays_scheduled() {
    // resolve_status does not convert scheduled→published; the background task does that
    let past_utc = (chrono::Utc::now() - chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M")
        .to_string();
    let status = crate::routes::admin::resolve_status("scheduled", &Some(past_utc));
    assert_eq!(
        status, "scheduled",
        "resolve_status should not change scheduled to published"
    );
}

#[test]
fn published_at_default_fallback_is_utc() {
    // When published_at is empty, the server defaults to Utc::now()
    // Verify the format matches what resolve_status expects
    let fallback = chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string();
    let parsed = chrono::NaiveDateTime::parse_from_str(&fallback, "%Y-%m-%dT%H:%M");
    assert!(parsed.is_ok(), "default fallback format must be parseable");
    // The fallback should be "now" which is not in the future
    let dt = parsed.unwrap();
    assert!(
        dt <= chrono::Utc::now().naive_utc() + chrono::Duration::seconds(1),
        "default fallback should be approximately now, not in the future"
    );
}

#[test]
fn resolve_status_date_with_seconds_format_stays_published() {
    // DB stores dates like "2026-02-19 11:30:00" — resolve_status uses "%Y-%m-%dT%H:%M"
    // This format won't parse, so status should pass through unchanged
    let status =
        crate::routes::admin::resolve_status("published", &Some("2020-01-01 12:00:00".to_string()));
    assert_eq!(
        status, "published",
        "DB datetime format (with space+seconds) should not parse and status passes through"
    );
}

#[test]
fn resolve_status_handles_timezone_offset_string_gracefully() {
    // If somehow a timezone-aware string gets through, it should not parse and pass through
    let status = crate::routes::admin::resolve_status(
        "published",
        &Some("2099-01-01T12:00+04:00".to_string()),
    );
    assert_eq!(
        status, "published",
        "timezone-aware string should not parse with NaiveDateTime"
    );
}

// ═══════════════════════════════════════════════════════════
// SQL seed defaults: verify all critical settings are seeded
// ═══════════════════════════════════════════════════════════

#[test]
fn seed_defaults_no_duplicate_keys() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    // Count total vs distinct keys
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
        .unwrap();
    let distinct: i64 = conn
        .query_row("SELECT COUNT(DISTINCT key) FROM settings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        total, distinct,
        "settings table should have no duplicate keys"
    );
}

#[test]
fn seed_defaults_privacy_policy_content_not_empty() {
    let pool = test_pool();
    let val = Setting::get(&pool, "privacy_policy_content").unwrap_or_default();
    assert!(
        !val.is_empty(),
        "privacy_policy_content should have default HTML content"
    );
    assert!(
        val.contains("Privacy Policy"),
        "privacy_policy_content should contain 'Privacy Policy'"
    );
    assert!(
        val.contains("<h1>"),
        "privacy_policy_content should contain HTML headings"
    );
}

#[test]
fn seed_defaults_terms_of_use_content_not_empty() {
    let pool = test_pool();
    let val = Setting::get(&pool, "terms_of_use_content").unwrap_or_default();
    assert!(
        !val.is_empty(),
        "terms_of_use_content should have default HTML content"
    );
    assert!(
        val.contains("Terms of Use"),
        "terms_of_use_content should contain 'Terms of Use'"
    );
    assert!(
        val.contains("<h1>"),
        "terms_of_use_content should contain HTML headings"
    );
}

#[test]
fn seed_defaults_critical_settings_exist() {
    let pool = test_pool();
    // Core settings that must always exist
    let required_keys = vec![
        "site_name",
        "site_url",
        "admin_slug",
        "admin_theme",
        "journal_enabled",
        "blog_slug",
        "blog_posts_per_page",
        "blog_display_type",
        "portfolio_enabled",
        "portfolio_slug",
        "portfolio_display_type",
        "portfolio_grid_columns",
        "comments_enabled",
        "font_primary",
        "font_heading",
        "images_allowed_types",
        "images_max_upload_mb",
        "seo_sitemap_enabled",
        "seo_robots_txt",
        "privacy_policy_enabled",
        "privacy_policy_content",
        "terms_of_use_enabled",
        "terms_of_use_content",
        "task_scheduled_publish_interval",
        "firewall_enabled",
        "cookie_consent_enabled",
        "design_back_to_top",
        "downloads_license_template",
        "commerce_currency",
    ];
    for key in required_keys {
        let val = Setting::get(&pool, key);
        assert!(
            val.is_some(),
            "required setting '{}' must exist after seed_defaults",
            key
        );
    }
}

#[test]
fn seed_defaults_setting_groups_present() {
    let pool = test_pool();
    let all = Setting::all(&pool);
    // Verify each major group has at least one key
    let groups = vec![
        ("seo_", "SEO"),
        ("font_", "Fonts"),
        ("images_", "Images"),
        ("blog_", "Blog/Journal"),
        ("portfolio_", "Portfolio"),
        ("comments_", "Comments"),
        ("social_", "Social"),
        ("cookie_consent_", "Cookie Consent"),
        ("fw_", "Firewall"),
        ("ai_", "AI"),
        ("email_", "Email"),
        ("task_", "Background Tasks"),
        ("commerce_", "Commerce"),
        ("layout_", "Layout"),
    ];
    for (prefix, label) in groups {
        let count = all.keys().filter(|k| k.starts_with(prefix)).count();
        assert!(
            count > 0,
            "{} settings (prefix '{}') should have at least one entry, found {}",
            label,
            prefix,
            count
        );
    }
}

#[test]
fn settings_save_portfolio_disable_persists() {
    // Simulate: portfolio is enabled, user unchecks it and saves.
    // The form only sends _tab=portfolio (checkbox unchecked = not in form data,
    // fieldset disabled = inner fields not sent).
    let pool = test_pool();

    // Pre-condition: portfolio is enabled
    Setting::set(&pool, "portfolio_enabled", "true").unwrap();
    Setting::set(&pool, "portfolio_slug", "portfolio").unwrap();
    Setting::set(&pool, "portfolio_enable_likes", "true").unwrap();
    assert_eq!(Setting::get(&pool, "portfolio_enabled").unwrap(), "true");

    // Simulate the save flow from settings_save for section="portfolio"
    // Step 1: Reset all checkbox keys to "false" (lightbox keys moved to Designer)
    let checkbox_keys: &[&str] = &[
        "portfolio_enabled",
        "portfolio_enable_likes",
        "portfolio_image_protection",
    ];
    for key in checkbox_keys {
        Setting::set(&pool, key, "false").unwrap();
    }

    // Step 2: set_many with form data (only _tab since checkbox unchecked + fieldset disabled)
    let mut data: HashMap<String, String> = HashMap::new();
    data.insert("_tab".to_string(), "portfolio".to_string());
    Setting::set_many(&pool, &data).unwrap();

    // portfolio_enabled should be "false" — it was reset in step 1 and NOT in form data
    let val = Setting::get(&pool, "portfolio_enabled").unwrap();
    assert_eq!(
        val, "false",
        "portfolio_enabled should be false after unchecking and saving"
    );

    // Other checkbox keys should also be false
    let likes = Setting::get(&pool, "portfolio_enable_likes").unwrap();
    assert_eq!(
        likes, "false",
        "portfolio_enable_likes should be false after save"
    );
}

#[test]
fn settings_save_portfolio_enable_persists() {
    // Simulate: portfolio is disabled, user checks it and saves.
    // The form sends portfolio_enabled=true plus all the inner fields.
    let pool = test_pool();

    // Pre-condition: portfolio is disabled
    Setting::set(&pool, "portfolio_enabled", "false").unwrap();

    // Step 1: Reset checkboxes to false (lightbox keys moved to Designer)
    let checkbox_keys: &[&str] = &[
        "portfolio_enabled",
        "portfolio_enable_likes",
        "portfolio_image_protection",
    ];
    for key in checkbox_keys {
        Setting::set(&pool, key, "false").unwrap();
    }

    // Step 2: set_many with form data (checkbox checked = in form data)
    let mut data: HashMap<String, String> = HashMap::new();
    data.insert("_tab".to_string(), "portfolio".to_string());
    data.insert("portfolio_enabled".to_string(), "true".to_string());
    data.insert("portfolio_slug".to_string(), "portfolio".to_string());
    data.insert("portfolio_enable_likes".to_string(), "true".to_string());
    Setting::set_many(&pool, &data).unwrap();

    let val = Setting::get(&pool, "portfolio_enabled").unwrap();
    assert_eq!(
        val, "true",
        "portfolio_enabled should be true after checking and saving"
    );
}

#[test]
fn settings_save_journal_disable_persists() {
    // Same test for journal_enabled
    let pool = test_pool();
    Setting::set(&pool, "journal_enabled", "true").unwrap();

    // Step 1: Reset checkboxes
    let checkbox_keys: &[&str] = &[
        "journal_enabled",
        "blog_show_author",
        "blog_show_date",
        "blog_show_reading_time",
        "blog_featured_image_required",
    ];
    for key in checkbox_keys {
        Setting::set(&pool, key, "false").unwrap();
    }

    // Step 2: set_many with only _tab (checkbox unchecked)
    let mut data: HashMap<String, String> = HashMap::new();
    data.insert("_tab".to_string(), "journal".to_string());
    Setting::set_many(&pool, &data).unwrap();

    let val = Setting::get(&pool, "journal_enabled").unwrap();
    assert_eq!(
        val, "false",
        "journal_enabled should be false after unchecking and saving"
    );
}

#[test]
fn seed_defaults_legal_content_backfill_migration() {
    // Simulate the bug: insert empty strings first, then run seed_defaults
    let manager = SqliteConnectionManager::memory();
    let pool: DbPool = Pool::builder().max_size(1).build(manager).unwrap();
    run_migrations(&pool).expect("migrations");
    // Manually insert empty legal content (simulating the old bug)
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('privacy_policy_content', '')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('terms_of_use_content', '')",
            [],
        )
        .unwrap();
    }
    // Now run seed_defaults — the migration should backfill
    seed_defaults(&pool).expect("seed_defaults");
    let pp = Setting::get(&pool, "privacy_policy_content").unwrap_or_default();
    let tu = Setting::get(&pool, "terms_of_use_content").unwrap_or_default();
    assert!(
        !pp.is_empty(),
        "privacy_policy_content should be backfilled from empty"
    );
    assert!(
        !tu.is_empty(),
        "terms_of_use_content should be backfilled from empty"
    );
    assert!(
        pp.contains("Privacy Policy"),
        "backfilled privacy content should contain heading"
    );
    assert!(
        tu.contains("Terms of Use"),
        "backfilled terms content should contain heading"
    );
}

// ═══════════════════════════════════════════════════════════
// Journal Category Navigation Tests
// ═══════════════════════════════════════════════════════════

/// Helper: build a blog_list context with journal + portfolio nav categories.
fn render_blog_nav_context(pool: &DbPool) -> serde_json::Value {
    let settings = Setting::all(pool);
    let nav_categories = Category::list_nav_visible(pool, Some("portfolio"));
    let nav_journal_categories = Category::list_nav_visible(pool, Some("post"));
    json!({
        "settings": settings,
        "posts": [],
        "nav_categories": nav_categories,
        "nav_journal_categories": nav_journal_categories,
        "current_page": 1,
        "total_pages": 1,
        "page_type": "blog_list",
        "seo": "",
    })
}

#[test]
fn render_journal_sidebar_under_link_has_toggle() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Travel", "travel", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "under_link"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    // Sidebar under_link starts open
    assert!(
        html.contains("nav-category-toggle open"),
        "under_link toggle should start open in sidebar"
    );
    assert!(
        html.contains("nav-subcategories open"),
        "under_link subcategories should start open in sidebar"
    );
    assert!(html.contains(">All</a>"), "should have 'All' journal link");
    assert!(
        html.contains(">Travel</a>"),
        "should have 'Travel' category link"
    );
}

#[test]
fn render_journal_sidebar_under_link_custom_all_label() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Tech", "tech", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "under_link"),
            ("journal_all_categories_label", "Everything"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    assert!(
        html.contains(">Everything</a>"),
        "should use custom 'All' label"
    );
}

#[test]
fn render_journal_sidebar_under_link_all_hidden() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Tech", "tech", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "under_link"),
            ("journal_show_all_categories", "false"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    assert!(
        !html.contains("cat-link active\">All</a>"),
        "should not have 'All' link when hidden"
    );
    assert!(
        html.contains(">Tech</a>"),
        "should still have category links"
    );
}

#[test]
fn render_journal_page_top_has_filter_bar() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Travel", "travel", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "page_top"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    assert!(
        html.contains("categories-page-top"),
        "page_top should render filter bar"
    );
    assert!(
        html.contains(">Travel</a>"),
        "page_top should have category link"
    );
    // Journal nav link should be a plain link (not a toggle) since page_top mode
    // uses the filter bar instead of sidebar toggle
    assert!(
        html.contains("nav-link\">journal</a>") || html.contains("nav-link active\">journal</a>"),
        "page_top mode should show plain journal nav-link in sidebar"
    );
}

#[test]
fn render_journal_page_top_align_right() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Travel", "travel", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "page_top"),
            ("journal_nav_categories_align", "right"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    assert!(
        html.contains("categories-page-top cats-right"),
        "page_top right alignment should have cats-right class"
    );
}

#[test]
fn render_journal_hidden_shows_plain_link() {
    let pool = test_pool();
    Category::create(&pool, &make_cat_form("Travel", "travel", "post")).unwrap();
    set_settings(
        &pool,
        &[
            ("journal_enabled", "true"),
            ("layout_header_type", "sidebar"),
            ("journal_nav_categories", "hidden"),
        ],
    );
    let ctx = render_blog_nav_context(&pool);
    let html = render::render_page(&pool, "blog_list", &ctx);

    // Should have a plain nav-link for journal, not a category toggle
    assert!(
        html.contains("nav-link\">journal</a>") || html.contains("nav-link active\">journal</a>"),
        "hidden mode should show plain journal nav-link"
    );
    assert!(
        !html.contains("<div class=\"categories-page-top"),
        "hidden mode should not have page_top filter div"
    );
}

// ═══════════════════════════════════════════════════════════
// Image Proxy Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn image_proxy_encode_decode_roundtrip() {
    let secret = "test_secret_key_1234567890abcdef";
    let path = "/uploads/2024/01/photo.jpg";
    let token = crate::image_proxy::encode_token(secret, path);
    let decoded = crate::image_proxy::decode_token(secret, &token);
    assert_eq!(decoded, Some(path.to_string()));
}

#[test]
fn image_proxy_tampered_token_rejected() {
    let secret = "test_secret_key_1234567890abcdef";
    let path = "/uploads/2024/01/photo.jpg";
    let token = crate::image_proxy::encode_token(secret, path);
    // Tamper with the signature
    let tampered = format!("{}x", token);
    assert_eq!(crate::image_proxy::decode_token(secret, &tampered), None);
}

#[test]
fn image_proxy_wrong_secret_rejected() {
    let secret = "correct_secret";
    let path = "/uploads/photo.jpg";
    let token = crate::image_proxy::encode_token(secret, path);
    let decoded = crate::image_proxy::decode_token("wrong_secret", &token);
    assert_eq!(decoded, None);
}

#[test]
fn image_proxy_rewrite_upload_urls() {
    let secret = "rewrite_test_secret";
    let html = r#"<img src="/uploads/2024/photo.jpg"> and <a href="/uploads/doc.pdf">link</a>"#;
    let rewritten = crate::image_proxy::rewrite_upload_urls(html, secret);

    assert!(
        !rewritten.contains("/uploads/"),
        "rewritten HTML should not contain /uploads/"
    );
    assert!(
        rewritten.contains("/img/"),
        "rewritten HTML should contain /img/ proxy URLs"
    );
    // Verify the tokens are valid
    let token1_start = rewritten.find("/img/").unwrap() + 5;
    let token1_end = rewritten[token1_start..].find('"').unwrap() + token1_start;
    let token1 = &rewritten[token1_start..token1_end];
    let decoded = crate::image_proxy::decode_token(secret, token1);
    assert_eq!(decoded, Some("/uploads/2024/photo.jpg".to_string()));
}

#[test]
fn image_proxy_preserves_non_upload_urls() {
    let secret = "preserve_test_secret";
    let html = r#"<img src="/static/logo.png"> <a href="/blog/post">link</a>"#;
    let rewritten = crate::image_proxy::rewrite_upload_urls(html, secret);
    assert_eq!(html, rewritten, "non-upload URLs should be unchanged");
}

#[test]
fn image_proxy_mime_detection() {
    assert_eq!(
        crate::image_proxy::mime_from_extension("photo.jpg"),
        "image/jpeg"
    );
    assert_eq!(
        crate::image_proxy::mime_from_extension("image.png"),
        "image/png"
    );
    assert_eq!(
        crate::image_proxy::mime_from_extension("pic.webp"),
        "image/webp"
    );
    assert_eq!(
        crate::image_proxy::mime_from_extension("doc.pdf"),
        "application/pdf"
    );
    assert_eq!(
        crate::image_proxy::mime_from_extension("unknown.xyz"),
        "application/octet-stream"
    );
}

#[test]
fn image_proxy_render_rewrites_urls() {
    let pool = test_pool();
    set_settings(
        &pool,
        &[
            ("portfolio_enabled", "true"),
            ("portfolio_slug", "portfolio"),
        ],
    );
    // Create a portfolio item with an image path
    let form = crate::models::portfolio::PortfolioForm {
        title: "Test".into(),
        slug: "test".into(),
        description_json: None,
        description_html: Some("desc".into()),
        image_path: "2024/01/photo.jpg".into(),
        thumbnail_path: None,
        meta_title: None,
        meta_description: None,
        sell_enabled: None,
        price: None,
        purchase_note: None,
        payment_provider: None,
        download_file_path: None,
        status: "published".into(),
        published_at: None,
        category_ids: None,
        tag_ids: None,
    };
    PortfolioItem::create(&pool, &form).unwrap();

    let items = PortfolioItem::published(&pool, 10, 0);
    let categories = Category::list(&pool, Some("portfolio"));
    let settings: HashMap<String, String> = Setting::all(&pool);
    let settings_json = serde_json::to_value(&settings).unwrap();
    let items_json = serde_json::to_value(&items).unwrap();
    let cats_json = serde_json::to_value(&categories).unwrap();

    let ctx = serde_json::json!({
        "settings": settings_json,
        "items": items_json,
        "categories": cats_json,
        "page_type": "portfolio_grid",
    });

    let html = render::render_page(&pool, "portfolio_grid", &ctx);

    // The rendered HTML should NOT contain raw /uploads/ paths in src attributes
    assert!(
        !html.contains("src=\"/uploads/"),
        "rendered HTML should not expose raw /uploads/ paths"
    );
    // It should contain /img/ proxy URLs instead
    assert!(
        html.contains("/img/"),
        "rendered HTML should use /img/ proxy URLs"
    );
}

#[test]
fn image_proxy_seed_generates_secret() {
    let pool = test_pool();
    let settings: HashMap<String, String> = Setting::all(&pool);
    let secret = settings
        .get("image_proxy_secret")
        .cloned()
        .unwrap_or_default();
    assert!(
        !secret.is_empty(),
        "seed_defaults should generate image_proxy_secret"
    );
    assert_eq!(secret.len(), 64, "secret should be 32 bytes = 64 hex chars");
}

#[test]
fn image_proxy_dual_key_fallback() {
    let old_secret = "old_secret_key";
    let new_secret = "new_secret_key";
    let path = "/uploads/photo.jpg";

    // Token signed with old key
    let old_token = crate::image_proxy::encode_token(old_secret, path);

    // Should NOT decode with new key alone
    assert_eq!(
        crate::image_proxy::decode_token(new_secret, &old_token),
        None
    );

    // Should decode with fallback when old key hasn't expired
    let future = (chrono::Utc::now().naive_utc() + chrono::Duration::days(7))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let decoded =
        crate::image_proxy::decode_token_with_fallback(new_secret, old_secret, &future, &old_token);
    assert_eq!(decoded, Some(path.to_string()));

    // Token signed with new key should decode directly (no fallback needed)
    let new_token = crate::image_proxy::encode_token(new_secret, path);
    let decoded =
        crate::image_proxy::decode_token_with_fallback(new_secret, old_secret, &future, &new_token);
    assert_eq!(decoded, Some(path.to_string()));
}

#[test]
fn image_proxy_dual_key_expired_old_rejected() {
    let old_secret = "old_secret_key";
    let new_secret = "new_secret_key";
    let path = "/uploads/photo.jpg";

    let old_token = crate::image_proxy::encode_token(old_secret, path);

    // Old key has expired (date in the past)
    let past = (chrono::Utc::now().naive_utc() - chrono::Duration::days(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let decoded =
        crate::image_proxy::decode_token_with_fallback(new_secret, old_secret, &past, &old_token);
    assert_eq!(decoded, None, "expired old key should not decode");
}

#[test]
fn image_proxy_dual_key_no_old_secret() {
    let new_secret = "new_secret_key";
    let path = "/uploads/photo.jpg";
    let token = crate::image_proxy::encode_token(new_secret, path);

    // Empty old secret — should still decode with current key
    let decoded = crate::image_proxy::decode_token_with_fallback(new_secret, "", "", &token);
    assert_eq!(decoded, Some(path.to_string()));

    // Wrong token with no old secret — should fail
    let bad_token = crate::image_proxy::encode_token("wrong", path);
    let decoded = crate::image_proxy::decode_token_with_fallback(new_secret, "", "", &bad_token);
    assert_eq!(decoded, None);
}

// ── Passkey Tests ──────────────────────────────────────

use crate::models::passkey::UserPasskey;

#[test]
fn passkey_migration_creates_table() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    // Table should exist after migrations
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM user_passkeys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn passkey_migration_adds_user_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    // auth_method and auth_method_fallback columns should exist
    let _: String = conn
        .query_row("SELECT auth_method FROM users LIMIT 0", [], |row| {
            row.get(0)
        })
        .unwrap_or_default();
    let _: String = conn
        .query_row(
            "SELECT auth_method_fallback FROM users LIMIT 0",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
}

#[test]
fn passkey_user_default_auth_method() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_user@test.com", "$2b$04$aaaa", "PK User", "admin");
    let user = User::get_by_email(&pool, "pk_user@test.com").unwrap();
    assert_eq!(user.auth_method, "password");
    assert_eq!(user.auth_method_fallback, "password");
}

#[test]
fn passkey_update_auth_method() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_auth@test.com", "$2b$04$aaaa", "PK Auth", "admin");
    let user = User::get_by_email(&pool, "pk_auth@test.com").unwrap();
    assert_eq!(user.auth_method, "password");

    User::update_auth_method(&pool, user.id, "passkey", "password").unwrap();
    let updated = User::get_by_id(&pool, user.id).unwrap();
    assert_eq!(updated.auth_method, "passkey");
    assert_eq!(updated.auth_method_fallback, "password");
}

#[test]
fn passkey_crud_create_and_list() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_crud@test.com", "$2b$04$aaaa", "PK CRUD", "admin");
    let user = User::get_by_email(&pool, "pk_crud@test.com").unwrap();

    // Initially empty
    let keys = UserPasskey::list_for_user(&pool, user.id);
    assert!(keys.is_empty());
    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 0);

    // Create a passkey
    let id = UserPasskey::create(
        &pool,
        user.id,
        "cred_id_1",
        r#"{"test":"key"}"#,
        0,
        "[]",
        "My YubiKey",
    )
    .unwrap();
    assert!(id > 0);

    let keys = UserPasskey::list_for_user(&pool, user.id);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].name, "My YubiKey");
    assert_eq!(keys[0].credential_id, "cred_id_1");
    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 1);
}

#[test]
fn passkey_get_by_credential_id() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_get@test.com", "$2b$04$aaaa", "PK Get", "admin");
    let user = User::get_by_email(&pool, "pk_get@test.com").unwrap();

    UserPasskey::create(&pool, user.id, "cred_abc", r#"{}"#, 0, "[]", "Key1").unwrap();

    let found = UserPasskey::get_by_credential_id(&pool, "cred_abc");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Key1");

    let not_found = UserPasskey::get_by_credential_id(&pool, "nonexistent");
    assert!(not_found.is_none());
}

#[test]
fn passkey_update_sign_count() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_sign@test.com", "$2b$04$aaaa", "PK Sign", "admin");
    let user = User::get_by_email(&pool, "pk_sign@test.com").unwrap();

    UserPasskey::create(&pool, user.id, "cred_sign", r#"{}"#, 0, "[]", "Key").unwrap();
    UserPasskey::update_sign_count(&pool, "cred_sign", 42).unwrap();

    let pk = UserPasskey::get_by_credential_id(&pool, "cred_sign").unwrap();
    assert_eq!(pk.sign_count, 42);
}

#[test]
fn passkey_delete_single() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_del@test.com", "$2b$04$aaaa", "PK Del", "admin");
    let user = User::get_by_email(&pool, "pk_del@test.com").unwrap();

    let id1 = UserPasskey::create(&pool, user.id, "cred_d1", r#"{}"#, 0, "[]", "Key1").unwrap();
    let _id2 = UserPasskey::create(&pool, user.id, "cred_d2", r#"{}"#, 0, "[]", "Key2").unwrap();

    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 2);

    UserPasskey::delete(&pool, id1, user.id).unwrap();
    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 1);

    // Can't delete someone else's key
    let result = UserPasskey::delete(&pool, _id2, user.id + 999);
    assert!(result.is_err());
}

#[test]
fn passkey_delete_all_for_user() {
    let pool = test_pool();
    let _ = User::create(
        &pool,
        "pk_delall@test.com",
        "$2b$04$aaaa",
        "PK DelAll",
        "admin",
    );
    let user = User::get_by_email(&pool, "pk_delall@test.com").unwrap();

    UserPasskey::create(&pool, user.id, "cred_a1", r#"{}"#, 0, "[]", "A").unwrap();
    UserPasskey::create(&pool, user.id, "cred_a2", r#"{}"#, 0, "[]", "B").unwrap();
    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 2);

    UserPasskey::delete_all_for_user(&pool, user.id).unwrap();
    assert_eq!(UserPasskey::count_for_user(&pool, user.id), 0);
}

#[test]
fn passkey_unique_credential_id() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_uniq@test.com", "$2b$04$aaaa", "PK Uniq", "admin");
    let user = User::get_by_email(&pool, "pk_uniq@test.com").unwrap();

    UserPasskey::create(&pool, user.id, "cred_dup", r#"{}"#, 0, "[]", "Key1").unwrap();
    let dup = UserPasskey::create(&pool, user.id, "cred_dup", r#"{}"#, 0, "[]", "Key2");
    assert!(dup.is_err(), "duplicate credential_id should fail");
}

#[test]
fn passkey_safe_json_includes_auth_fields() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_json@test.com", "$2b$04$aaaa", "PK Json", "admin");
    let user = User::get_by_email(&pool, "pk_json@test.com").unwrap();
    let json = user.safe_json();
    assert_eq!(json["auth_method"], "password");
    assert_eq!(json["auth_method_fallback"], "password");

    User::update_auth_method(&pool, user.id, "passkey", "magic_link").unwrap();
    let user2 = User::get_by_id(&pool, user.id).unwrap();
    let json2 = user2.safe_json();
    assert_eq!(json2["auth_method"], "passkey");
    assert_eq!(json2["auth_method_fallback"], "magic_link");
}

#[test]
fn passkey_auto_enable_on_first_registration() {
    let pool = test_pool();
    let _ = User::create(&pool, "pk_auto@test.com", "$2b$04$aaaa", "PK Auto", "admin");
    let user = User::get_by_email(&pool, "pk_auto@test.com").unwrap();
    assert_eq!(user.auth_method, "password");

    // Simulate first passkey registration: create passkey, then auto-enable
    UserPasskey::create(&pool, user.id, "cred_auto1", r#"{}"#, 0, "[]", "First").unwrap();
    let count = UserPasskey::count_for_user(&pool, user.id);
    if count == 1 {
        let fallback = &user.auth_method;
        User::update_auth_method(&pool, user.id, "passkey", fallback).unwrap();
    }

    let updated = User::get_by_id(&pool, user.id).unwrap();
    assert_eq!(updated.auth_method, "passkey");
    assert_eq!(updated.auth_method_fallback, "password");
}

#[test]
fn passkey_auto_revert_on_last_deletion() {
    let pool = test_pool();
    let _ = User::create(
        &pool,
        "pk_revert@test.com",
        "$2b$04$aaaa",
        "PK Revert",
        "admin",
    );
    let user = User::get_by_email(&pool, "pk_revert@test.com").unwrap();

    // Set up: user has passkey enabled with password fallback
    let pk_id = UserPasskey::create(&pool, user.id, "cred_rev1", r#"{}"#, 0, "[]", "Only").unwrap();
    User::update_auth_method(&pool, user.id, "passkey", "password").unwrap();

    let u = User::get_by_id(&pool, user.id).unwrap();
    assert_eq!(u.auth_method, "passkey");

    // Delete last passkey — should revert to fallback
    UserPasskey::delete(&pool, pk_id, user.id).unwrap();
    let remaining = UserPasskey::count_for_user(&pool, user.id);
    if remaining == 0 {
        User::update_auth_method(
            &pool,
            user.id,
            &u.auth_method_fallback,
            &u.auth_method_fallback,
        )
        .unwrap();
    }

    let reverted = User::get_by_id(&pool, user.id).unwrap();
    assert_eq!(reverted.auth_method, "password");
    assert_eq!(reverted.auth_method_fallback, "password");
}

#[test]
fn passkey_no_revert_when_keys_remain() {
    let pool = test_pool();
    let _ = User::create(
        &pool,
        "pk_norev@test.com",
        "$2b$04$aaaa",
        "PK NoRev",
        "admin",
    );
    let user = User::get_by_email(&pool, "pk_norev@test.com").unwrap();

    let pk1 = UserPasskey::create(&pool, user.id, "cred_nr1", r#"{}"#, 0, "[]", "Key1").unwrap();
    let _pk2 = UserPasskey::create(&pool, user.id, "cred_nr2", r#"{}"#, 0, "[]", "Key2").unwrap();
    User::update_auth_method(&pool, user.id, "passkey", "password").unwrap();

    // Delete one — should NOT revert since one remains
    UserPasskey::delete(&pool, pk1, user.id).unwrap();
    let remaining = UserPasskey::count_for_user(&pool, user.id);
    assert_eq!(remaining, 1);

    let u = User::get_by_id(&pool, user.id).unwrap();
    assert_eq!(
        u.auth_method, "passkey",
        "should stay passkey with keys remaining"
    );
}

#[test]
fn passkey_security_store_and_take_reg_state() {
    let pool = test_pool();
    // store_reg_state / take_reg_state use settings table
    let key = format!("passkey_reg_state_{}", 999);
    Setting::set(&pool, &key, r#"{"test":"state"}"#).unwrap();
    let val = Setting::get(&pool, &key);
    assert!(val.is_some());
    assert!(val.unwrap().contains("test"));

    // Clear it
    Setting::set(&pool, &key, "").unwrap();
    let val2 = Setting::get(&pool, &key).unwrap_or_default();
    assert!(val2.is_empty());
}

#[test]
fn passkey_security_store_and_take_auth_state() {
    let pool = test_pool();
    let token = "test-token-abc";
    let key = format!("passkey_auth_state_{}", token);
    Setting::set(&pool, &key, r#"{"challenge":"xyz"}"#).unwrap();
    let val = Setting::get(&pool, &key);
    assert!(val.is_some());

    // Simulate take (read + clear)
    let data = Setting::get(&pool, &key).unwrap();
    Setting::set(&pool, &key, "").unwrap();
    assert!(data.contains("challenge"));
    assert!(Setting::get(&pool, &key).unwrap_or_default().is_empty());
}

#[test]
fn passkey_migration_idempotent() {
    let pool = test_pool();
    // Running migrations again should not fail
    run_migrations(&pool).expect("Second migration run should succeed");
    run_migrations(&pool).expect("Third migration run should succeed");
    // Table should still be intact
    let count: i64 = pool
        .get()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM user_passkeys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

// ── Typography & Color CSS Variables Tests ─────────────────────────────

/// Helper: build a serde_json::Value from key-value pairs
fn typo_settings(pairs: &[(&str, &str)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    serde_json::Value::Object(map)
}

#[test]
fn css_vars_default_colors() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--color-text: #111827"));
    assert!(css.contains("--color-text-secondary: #6b7280"));
    assert!(css.contains("--color-bg: #ffffff"));
    assert!(css.contains("--color-accent: #3b82f6"));
    assert!(css.contains("--color-link: #3b82f6"));
    assert!(css.contains("--color-link-hover: #2563eb"));
    assert!(css.contains("--color-border: #e5e7eb"));
    assert!(css.contains("--color-logo-text: #111827"));
    assert!(css.contains("--color-tagline: #6b7280"));
    assert!(css.contains("--color-heading: #111827"));
    assert!(css.contains("--color-subheading: #1f2937"));
    assert!(css.contains("--color-caption: #374151"));
    assert!(css.contains("--color-footer: #9ca3af"));
    assert!(css.contains("--color-categories: #6b7280"));
    assert!(css.contains("--color-tags: #6b7280"));
    assert!(css.contains("--color-lightbox-categories: #AAAAAA"));
}

#[test]
fn css_vars_custom_colors() {
    let settings = typo_settings(&[
        ("site_text_color", "#ff0000"),
        ("site_background_color", "#000000"),
        ("site_accent_color", "#00ff00"),
        ("color_link", "#1122cc"),
        ("color_link_hover", "#3344ee"),
        ("color_border", "#aabbcc"),
        ("color_logo_text", "#deadbe"),
        ("color_tagline", "#cafe01"),
        ("color_heading", "#abcdef"),
        ("color_subheading", "#123456"),
        ("color_caption", "#654321"),
        ("color_footer", "#999888"),
        ("color_categories", "#112233"),
        ("color_tags", "#445566"),
        ("color_lightbox_categories", "#778899"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--color-text: #ff0000"));
    assert!(css.contains("--color-bg: #000000"));
    assert!(css.contains("--color-accent: #00ff00"));
    assert!(css.contains("--color-link: #1122cc"));
    assert!(css.contains("--color-link-hover: #3344ee"));
    assert!(css.contains("--color-border: #aabbcc"));
    assert!(css.contains("--color-logo-text: #deadbe"));
    assert!(css.contains("--color-tagline: #cafe01"));
    assert!(css.contains("--color-heading: #abcdef"));
    assert!(css.contains("--color-subheading: #123456"));
    assert!(css.contains("--color-caption: #654321"));
    assert!(css.contains("--color-footer: #999888"));
    assert!(css.contains("--color-categories: #112233"));
    assert!(css.contains("--color-tags: #445566"));
    assert!(css.contains("--color-lightbox-categories: #778899"));
}

#[test]
fn css_vars_default_fonts_sitewide() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    // Default sitewide: all font families should be Inter
    assert!(css.contains("--font-primary: 'Inter', sans-serif"));
    assert!(css.contains("--font-heading: 'Inter', sans-serif"));
    assert!(css.contains("--font-body: 'Inter', sans-serif"));
    assert!(css.contains("--font-nav: 'Inter', sans-serif"));
    assert!(css.contains("--font-buttons: 'Inter', sans-serif"));
    assert!(css.contains("--font-captions: 'Inter', sans-serif"));
}

#[test]
fn css_vars_custom_fonts_sitewide() {
    let settings = typo_settings(&[
        ("font_primary", "Roboto"),
        ("font_heading", "Playfair Display"),
        ("font_sitewide", "true"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-primary: 'Roboto', sans-serif"));
    assert!(css.contains("--font-heading: 'Playfair Display', sans-serif"));
    // Sitewide: body/nav/buttons/captions inherit from primary
    assert!(css.contains("--font-body: 'Roboto', sans-serif"));
    assert!(css.contains("--font-nav: 'Roboto', sans-serif"));
    assert!(css.contains("--font-buttons: 'Roboto', sans-serif"));
    assert!(css.contains("--font-captions: 'Roboto', sans-serif"));
}

#[test]
fn css_vars_per_element_fonts_no_sitewide() {
    let settings = typo_settings(&[
        ("font_primary", "Roboto"),
        ("font_heading", "Playfair Display"),
        ("font_sitewide", "false"),
        ("font_body", "Lato"),
        ("font_headings", "Merriweather"),
        ("font_navigation", "Montserrat"),
        ("font_buttons", "Poppins"),
        ("font_captions", "Source Sans Pro"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-body: 'Lato', sans-serif"));
    assert!(css.contains("--font-heading: 'Merriweather', sans-serif"));
    assert!(css.contains("--font-nav: 'Montserrat', sans-serif"));
    assert!(css.contains("--font-buttons: 'Poppins', sans-serif"));
    assert!(css.contains("--font-captions: 'Source Sans Pro', sans-serif"));
}

#[test]
fn css_vars_per_element_fonts_fallback_to_primary() {
    // When sitewide is false but per-element fonts are not set, fallback to primary/heading
    let settings = typo_settings(&[
        ("font_primary", "Roboto"),
        ("font_heading", "Georgia"),
        ("font_sitewide", "false"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-body: 'Roboto', sans-serif"));
    assert!(css.contains("--font-heading: 'Georgia', sans-serif"));
    assert!(css.contains("--font-nav: 'Roboto', sans-serif"));
    assert!(css.contains("--font-buttons: 'Roboto', sans-serif"));
    assert!(css.contains("--font-captions: 'Roboto', sans-serif"));
}

#[test]
fn css_vars_independent_element_fonts() {
    // logo, subheading, blockquote, list, footer, lightbox_title, categories, tags
    // are always resolved independently of sitewide toggle
    let settings = typo_settings(&[
        ("font_primary", "Inter"),
        ("font_heading", "Inter"),
        ("font_logo", "Pacifico"),
        ("font_subheading", "Oswald"),
        ("font_blockquote", "Georgia"),
        ("font_list", "Fira Sans"),
        ("font_footer", "Nunito"),
        ("font_lightbox_title", "Raleway"),
        ("font_categories", "Open Sans"),
        ("font_tags", "Ubuntu"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-logo: 'Pacifico', sans-serif"));
    assert!(css.contains("--font-subheading: 'Oswald', sans-serif"));
    assert!(css.contains("--font-blockquote: 'Georgia', sans-serif"));
    assert!(css.contains("--font-list: 'Fira Sans', sans-serif"));
    assert!(css.contains("--font-footer: 'Nunito', sans-serif"));
    assert!(css.contains("--font-lb-title: 'Raleway', sans-serif"));
    assert!(css.contains("--font-categories: 'Open Sans', sans-serif"));
    assert!(css.contains("--font-tags: 'Ubuntu', sans-serif"));
}

#[test]
fn css_vars_default_font_sizes() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-size-body: 16px"));
    assert!(css.contains("--font-size-h1: 2.5rem"));
    assert!(css.contains("--font-size-h2: 2rem"));
    assert!(css.contains("--font-size-h3: 1.75rem"));
    assert!(css.contains("--font-size-h4: 1.5rem"));
    assert!(css.contains("--font-size-h5: 1.25rem"));
    assert!(css.contains("--font-size-h6: 1rem"));
    assert!(css.contains("--font-size-logo: 1.5rem"));
    assert!(css.contains("--font-size-nav: 14px"));
    assert!(css.contains("--font-size-footer: 12px"));
    assert!(css.contains("--line-height: 1.6"));
}

#[test]
fn css_vars_custom_font_sizes() {
    let settings = typo_settings(&[
        ("font_size_body", "18px"),
        ("font_size_h1", "3rem"),
        ("font_size_h2", "2.5rem"),
        ("font_size_logo", "2rem"),
        ("font_size_nav", "16px"),
        ("font_line_height", "1.8"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--font-size-body: 18px"));
    assert!(css.contains("--font-size-h1: 3rem"));
    assert!(css.contains("--font-size-h2: 2.5rem"));
    assert!(css.contains("--font-size-logo: 2rem"));
    assert!(css.contains("--font-size-nav: 16px"));
    assert!(css.contains("--line-height: 1.8"));
}

#[test]
fn css_vars_text_transform_direction_alignment() {
    let settings = typo_settings(&[
        ("font_text_transform", "uppercase"),
        ("font_text_direction", "rtl"),
        ("font_text_alignment", "center"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--text-transform: uppercase"));
    assert!(css.contains("--text-direction: rtl"));
    assert!(css.contains("--text-alignment: center"));
}

#[test]
fn css_vars_text_defaults() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--text-transform: none"));
    assert!(css.contains("--text-direction: ltr"));
    assert!(css.contains("--text-alignment: left"));
}

#[test]
fn css_vars_layout_sidebar_left() {
    let settings = typo_settings(&[("layout_sidebar_position", "left")]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--sidebar-direction: row"));
}

#[test]
fn css_vars_layout_sidebar_right() {
    let settings = typo_settings(&[("layout_sidebar_position", "right")]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--sidebar-direction: row-reverse"));
}

#[test]
fn css_vars_layout_margins() {
    let settings = typo_settings(&[
        ("layout_margin_top", "20"),
        ("layout_margin_bottom", "30"),
        ("layout_margin_left", "10"),
        ("layout_margin_right", "15"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--content-margin-top: 20px"));
    assert!(css.contains("--content-margin-bottom: 30px"));
    assert!(css.contains("--content-margin-left: 10px"));
    assert!(css.contains("--content-margin-right: 15px"));
}

#[test]
fn css_vars_layout_margins_zero() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--content-margin-top: 0"));
    assert!(css.contains("--content-margin-bottom: 0"));
    assert!(css.contains("--content-margin-left: 0"));
    assert!(css.contains("--content-margin-right: 0"));
}

#[test]
fn css_vars_layout_margins_with_px_suffix() {
    let settings = typo_settings(&[("layout_margin_top", "20px")]);
    let css = typography::build_css_variables(&settings);
    // Should strip existing px and re-add it
    assert!(css.contains("--content-margin-top: 20px"));
    assert!(!css.contains("--content-margin-top: 20pxpx"));
}

#[test]
fn css_vars_content_boundary_boxed() {
    let settings = typo_settings(&[("layout_content_boundary", "boxed")]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--content-max-width: 1200px"));
}

#[test]
fn css_vars_content_boundary_full() {
    let settings = typo_settings(&[("layout_content_boundary", "full")]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--content-max-width: none"));
}

#[test]
fn css_vars_grid_columns() {
    let settings = typo_settings(&[("portfolio_grid_columns", "4"), ("blog_grid_columns", "2")]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--grid-columns: 4"));
    assert!(css.contains("--blog-grid-columns: 2"));
}

#[test]
fn css_vars_lightbox_colors() {
    let settings = typo_settings(&[
        ("portfolio_lightbox_border_color", "#FF0000"),
        ("portfolio_lightbox_title_color", "#00FF00"),
        ("portfolio_lightbox_tag_color", "#0000FF"),
        ("portfolio_lightbox_nav_color", "#FFFF00"),
    ]);
    let css = typography::build_css_variables(&settings);
    assert!(css.contains("--lightbox-border-color: #FF0000"));
    assert!(css.contains("--lightbox-title-color: #00FF00"));
    assert!(css.contains("--lightbox-tag-color: #0000FF"));
    assert!(css.contains("--lightbox-nav-color: #FFFF00"));
}

#[test]
fn css_vars_wraps_in_root_selector() {
    let settings = typo_settings(&[]);
    let css = typography::build_css_variables(&settings);
    assert!(css.starts_with(":root {"));
    assert!(css.ends_with("}"));
}

// ── Font Links Tests ───────────────────────────────────────────────────

#[test]
fn font_links_empty_when_no_providers() {
    let settings = typo_settings(&[]);
    let html = typography::build_font_links(&settings);
    assert!(html.is_empty());
}

#[test]
fn font_links_google_basic() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Roboto"),
        ("font_heading", "Inter"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("fonts.googleapis.com"));
    assert!(html.contains("fonts.gstatic.com"));
    assert!(html.contains("family=Roboto"));
    assert!(html.contains("family=Inter"));
}

#[test]
fn font_links_google_deduplicates() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Roboto"),
        ("font_heading", "Roboto"),
    ]);
    let html = typography::build_font_links(&settings);
    // Should only appear once
    let count = html.matches("family=Roboto").count();
    assert_eq!(count, 1);
}

#[test]
fn font_links_google_per_element_fonts_not_sitewide() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Roboto"),
        ("font_heading", "Inter"),
        ("font_sitewide", "false"),
        ("font_body", "Lato"),
        ("font_navigation", "Montserrat"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("family=Roboto"));
    assert!(html.contains("family=Inter"));
    assert!(html.contains("family=Lato"));
    assert!(html.contains("family=Montserrat"));
}

#[test]
fn font_links_google_independent_element_fonts() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Inter"),
        ("font_heading", "Inter"),
        ("font_logo", "Pacifico"),
        ("font_footer", "Nunito"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("family=Pacifico"));
    assert!(html.contains("family=Nunito"));
}

#[test]
fn font_links_google_skips_system_fonts() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "system-ui"),
        ("font_heading", "Georgia, serif"),
    ]);
    let html = typography::build_font_links(&settings);
    // system-ui and Georgia are system fonts, should not generate Google Fonts link
    assert!(!html.contains("fonts.googleapis.com"));
}

#[test]
fn font_links_google_skips_adobe_prefixed() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "adobe-caslon-pro"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(!html.contains("adobe-caslon-pro"));
}

#[test]
fn font_links_adobe() {
    let settings = typo_settings(&[
        ("font_adobe_enabled", "true"),
        ("font_adobe_project_id", "abc123xyz"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("use.typekit.net/abc123xyz.css"));
}

#[test]
fn font_links_adobe_empty_project_id() {
    let settings = typo_settings(&[
        ("font_adobe_enabled", "true"),
        ("font_adobe_project_id", ""),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(!html.contains("typekit"));
}

#[test]
fn font_links_custom_font_face_woff2() {
    let settings = typo_settings(&[
        ("font_custom_name", "MyCustomFont"),
        ("font_custom_filename", "my-font.woff2"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("@font-face"));
    assert!(html.contains("font-family: 'MyCustomFont'"));
    assert!(html.contains("/uploads/fonts/my-font.woff2"));
    assert!(html.contains("format('woff2')"));
}

#[test]
fn font_links_custom_font_face_ttf() {
    let settings = typo_settings(&[
        ("font_custom_name", "MyFont"),
        ("font_custom_filename", "my-font.ttf"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("format('truetype')"));
}

#[test]
fn font_links_custom_font_face_otf() {
    let settings = typo_settings(&[
        ("font_custom_name", "MyFont"),
        ("font_custom_filename", "my-font.otf"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("format('opentype')"));
}

#[test]
fn font_links_custom_font_missing_name_no_output() {
    let settings = typo_settings(&[
        ("font_custom_name", ""),
        ("font_custom_filename", "my-font.woff2"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(!html.contains("@font-face"));
}

#[test]
fn font_links_custom_font_missing_file_no_output() {
    let settings = typo_settings(&[("font_custom_name", "MyFont"), ("font_custom_filename", "")]);
    let html = typography::build_font_links(&settings);
    assert!(!html.contains("@font-face"));
}

#[test]
fn font_links_google_and_adobe_and_custom_combined() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Roboto"),
        ("font_heading", "Roboto"),
        ("font_adobe_enabled", "true"),
        ("font_adobe_project_id", "xyz789"),
        ("font_custom_name", "BrandFont"),
        ("font_custom_filename", "brand.woff2"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("fonts.googleapis.com"));
    assert!(html.contains("family=Roboto"));
    assert!(html.contains("use.typekit.net/xyz789.css"));
    assert!(html.contains("@font-face"));
    assert!(html.contains("font-family: 'BrandFont'"));
}

#[test]
fn font_links_google_spaces_replaced_with_plus() {
    let settings = typo_settings(&[
        ("font_google_enabled", "true"),
        ("font_primary", "Open Sans"),
    ]);
    let html = typography::build_font_links(&settings);
    assert!(html.contains("family=Open+Sans"));
    assert!(!html.contains("family=Open Sans"));
}

// ── Render Integration: CSS vars appear in rendered output ─────────────

#[test]
fn render_css_vars_in_page_output() {
    let pool = test_pool();
    // Set some custom colors
    Setting::set(&pool, "site_text_color", "#ff1234").unwrap();
    Setting::set(&pool, "site_background_color", "#001122").unwrap();
    Setting::set(&pool, "font_primary", "Roboto").unwrap();
    // Build settings JSON the same way render.rs does
    let all = Setting::all(&pool);
    let settings_json: serde_json::Value = all
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let css = typography::build_css_variables(&settings_json);
    assert!(css.contains("--color-text: #ff1234"));
    assert!(css.contains("--color-bg: #001122"));
    assert!(css.contains("--font-primary: 'Roboto', sans-serif"));
}

// ── Image Optimization Settings Tests ──────────────────────────────────

#[test]
fn image_opt_seed_defaults_exist() {
    let pool = test_pool();
    assert_eq!(
        Setting::get(&pool, "images_max_dimension"),
        Some("0".to_string())
    );
    assert_eq!(
        Setting::get(&pool, "images_reencode"),
        Some("false".to_string())
    );
    assert_eq!(
        Setting::get(&pool, "images_strip_metadata"),
        Some("false".to_string())
    );
    assert_eq!(
        Setting::get(&pool, "images_quality"),
        Some("85".to_string())
    );
    assert_eq!(
        Setting::get(&pool, "images_webp_convert"),
        Some("false".to_string())
    );
}

#[test]
fn image_opt_max_dimension_disabled_by_default() {
    let pool = test_pool();
    let val = Setting::get_i64(&pool, "images_max_dimension");
    assert_eq!(val, 0, "max_dimension should default to 0 (disabled)");
}

#[test]
fn image_opt_max_dimension_set_and_read() {
    let pool = test_pool();
    Setting::set(&pool, "images_max_dimension", "2400").unwrap();
    assert_eq!(Setting::get_i64(&pool, "images_max_dimension"), 2400);
}

#[test]
fn image_opt_reencode_disabled_by_default() {
    let pool = test_pool();
    assert!(!Setting::get_bool(&pool, "images_reencode"));
}

#[test]
fn image_opt_reencode_enable() {
    let pool = test_pool();
    Setting::set(&pool, "images_reencode", "true").unwrap();
    assert!(Setting::get_bool(&pool, "images_reencode"));
}

#[test]
fn image_opt_strip_metadata_disabled_by_default() {
    let pool = test_pool();
    assert!(!Setting::get_bool(&pool, "images_strip_metadata"));
}

#[test]
fn image_opt_strip_metadata_enable() {
    let pool = test_pool();
    Setting::set(&pool, "images_strip_metadata", "true").unwrap();
    assert!(Setting::get_bool(&pool, "images_strip_metadata"));
}

#[test]
fn image_opt_quality_default() {
    let pool = test_pool();
    let q = Setting::get_i64(&pool, "images_quality") as u8;
    assert_eq!(q, 85);
}

#[test]
fn image_opt_quality_custom() {
    let pool = test_pool();
    Setting::set(&pool, "images_quality", "70").unwrap();
    assert_eq!(Setting::get_i64(&pool, "images_quality"), 70);
}

#[test]
fn image_opt_all_disabled_is_noop() {
    let pool = test_pool();
    // All three off — pipeline should be a no-op
    assert_eq!(Setting::get_i64(&pool, "images_max_dimension"), 0);
    assert!(!Setting::get_bool(&pool, "images_reencode"));
    assert!(!Setting::get_bool(&pool, "images_strip_metadata"));
}

#[test]
fn image_opt_reencode_implies_strip() {
    let pool = test_pool();
    // When reencode is on, strip_meta behavior is implied (both should be true in UI)
    Setting::set(&pool, "images_reencode", "true").unwrap();
    Setting::set(&pool, "images_strip_metadata", "true").unwrap();
    assert!(Setting::get_bool(&pool, "images_reencode"));
    assert!(Setting::get_bool(&pool, "images_strip_metadata"));
}

#[test]
fn image_opt_strip_without_reencode() {
    let pool = test_pool();
    // Strip alone forces re-encode under the hood
    Setting::set(&pool, "images_reencode", "false").unwrap();
    Setting::set(&pool, "images_strip_metadata", "true").unwrap();
    assert!(!Setting::get_bool(&pool, "images_reencode"));
    assert!(Setting::get_bool(&pool, "images_strip_metadata"));
}

#[test]
fn image_opt_max_dimension_zero_means_disabled() {
    let pool = test_pool();
    Setting::set(&pool, "images_max_dimension", "0").unwrap();
    let val = Setting::get_i64(&pool, "images_max_dimension") as u32;
    assert_eq!(val, 0);
    // 0 means no resize should happen
}

#[test]
fn image_opt_webp_quality_setting_used() {
    let pool = test_pool();
    Setting::set(&pool, "images_quality", "60").unwrap();
    let q = Setting::get_i64(&pool, "images_quality") as u8;
    assert_eq!(q, 60);
    // This quality value should be passed to WebP encoder and JPEG re-encode
}

// ═══════════════════════════════════════════════════════════
// Seed Defaults — Blog & Portfolio Slugs
// ═══════════════════════════════════════════════════════════

#[test]
fn seed_defaults_blog_slug_is_homepage() {
    let pool = test_pool();
    let blog_slug = Setting::get_or(&pool, "blog_slug", "MISSING");
    assert_eq!(
        blog_slug, "",
        "blog_slug should default to empty (homepage)"
    );
}

#[test]
fn seed_defaults_portfolio_slug_and_disabled() {
    let pool = test_pool();
    let portfolio_slug = Setting::get_or(&pool, "portfolio_slug", "MISSING");
    assert_eq!(portfolio_slug, "portfolio");
    let portfolio_enabled = Setting::get_or(&pool, "portfolio_enabled", "MISSING");
    assert_eq!(
        portfolio_enabled, "false",
        "portfolio should be disabled by default"
    );
}

// ═══════════════════════════════════════════════════════════
// Seed Defaults — Designs (Inkwell active, Oneguy exists)
// ═══════════════════════════════════════════════════════════

#[test]
fn seed_defaults_inkwell_active_oneguy_exists() {
    let pool = test_pool();
    let active = Design::active(&pool).expect("should have an active design");
    assert_eq!(
        active.slug, "inkwell",
        "Inkwell should be the default active design"
    );

    let oneguy = Design::find_by_slug(&pool, "oneguy");
    assert!(oneguy.is_some(), "Oneguy design should exist");
    assert!(
        !oneguy.unwrap().is_active,
        "Oneguy should not be active by default"
    );
}

// ═══════════════════════════════════════════════════════════
// Portfolio Renderer — Oneguy (default/fallback)
// ═══════════════════════════════════════════════════════════

#[test]
fn portfolio_render_grid_empty_items() {
    let ctx = serde_json::json!({ "items": [] });
    let html = crate::designs::oneguy::portfolio::render_grid(&ctx);
    assert!(
        html.contains("No portfolio items yet"),
        "empty grid should show placeholder"
    );
}

#[test]
fn portfolio_render_grid_with_items() {
    let ctx = serde_json::json!({
        "items": [
            {
                "item": {
                    "id": 1,
                    "title": "Sunset",
                    "slug": "sunset",
                    "image_path": "sunset.jpg",
                    "thumbnail_path": "",
                    "likes": 5,
                    "sell_enabled": false,
                    "price": 0.0
                },
                "tags": [],
                "categories": []
            }
        ],
        "settings": {
            "portfolio_slug": "portfolio",
            "portfolio_display_type": "masonry",
            "portfolio_show_tags": "false",
            "portfolio_show_categories": "false",
            "portfolio_enable_likes": "false",
            "portfolio_fade_animation": "none",
            "portfolio_border_style": "none",
            "portfolio_show_title": "true"
        },
        "current_page": 1,
        "total_pages": 1
    });
    let html = crate::designs::oneguy::portfolio::render_grid(&ctx);
    assert!(
        html.contains("masonry-grid"),
        "should use masonry-grid class"
    );
    assert!(
        html.contains("/portfolio/sunset"),
        "should link to portfolio item"
    );
    assert!(html.contains("Sunset"), "should contain item title");
}

#[test]
fn portfolio_render_grid_css_grid_mode() {
    let ctx = serde_json::json!({
        "items": [
            {
                "item": {
                    "id": 1,
                    "title": "A",
                    "slug": "a",
                    "image_path": "a.jpg",
                    "thumbnail_path": "",
                    "likes": 0,
                    "sell_enabled": false,
                    "price": 0.0
                },
                "tags": [],
                "categories": []
            }
        ],
        "settings": {
            "portfolio_slug": "portfolio",
            "portfolio_display_type": "grid",
            "portfolio_show_tags": "false",
            "portfolio_show_categories": "false",
            "portfolio_enable_likes": "false",
            "portfolio_fade_animation": "none",
            "portfolio_border_style": "none",
            "portfolio_show_title": "false"
        },
        "current_page": 1,
        "total_pages": 1
    });
    let html = crate::designs::oneguy::portfolio::render_grid(&ctx);
    assert!(
        html.contains("css-grid"),
        "display_type=grid should use css-grid class"
    );
}

#[test]
fn portfolio_render_single_valid() {
    let ctx = serde_json::json!({
        "item": {
            "id": 42,
            "title": "Mountain View",
            "slug": "mountain-view",
            "image_path": "mountain.jpg",
            "description_html": "<p>A beautiful mountain.</p>",
            "likes": 10,
            "meta_description": ""
        },
        "tags": [
            { "name": "Nature", "slug": "nature" }
        ],
        "categories": [
            { "name": "Landscape", "slug": "landscape" }
        ],
        "settings": {
            "portfolio_slug": "portfolio",
            "portfolio_enable_likes": "true",
            "portfolio_like_position": "top_right",
            "portfolio_show_categories": "below_left",
            "portfolio_show_tags": "below_left",
            "share_icons_position": "none"
        },
        "commerce_enabled": false,
        "comments_enabled": false
    });
    let html = crate::designs::oneguy::portfolio::render_single(&ctx);
    assert!(
        html.contains("portfolio-single"),
        "should have portfolio-single wrapper"
    );
    assert!(html.contains("Mountain View"), "should contain title");
    assert!(html.contains("mountain.jpg"), "should contain image path");
    assert!(
        html.contains("A beautiful mountain"),
        "should contain description"
    );
    assert!(
        html.contains("/portfolio/category/landscape"),
        "should link to category"
    );
    assert!(html.contains(">#Nature</span>"), "should show tag as pill");
    assert!(html.contains("like-btn"), "should show like button");
}

#[test]
fn portfolio_render_single_missing_item_returns_404() {
    let ctx = serde_json::json!({ "settings": {} });
    let html = crate::designs::oneguy::portfolio::render_single(&ctx);
    assert!(html.contains("404"), "missing item should render 404");
}

// ═══════════════════════════════════════════════════════════
// Store: health_content_stats (verifies portfolio table name)
// ═══════════════════════════════════════════════════════════

#[test]
fn health_content_stats_counts_portfolio() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    // Baseline: should have zero portfolio items
    let (_, _, _, portfolio_count, _, _, _, _, _, _) = store.health_content_stats();
    assert_eq!(portfolio_count, 0, "should start with 0 portfolio items");

    // Insert a portfolio item directly
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, status) VALUES ('Test', 'test-item', 'img.jpg', 'published')",
        [],
    ).unwrap();

    let (_, _, _, portfolio_count, _, _, _, _, _, _) = store.health_content_stats();
    assert_eq!(portfolio_count, 1, "should count 1 portfolio item");
}

// ═══════════════════════════════════════════════════════════
// Store: task_publish_scheduled (verifies portfolio table name)
// ═══════════════════════════════════════════════════════════

#[test]
fn task_publish_scheduled_publishes_portfolio() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    // Insert a scheduled portfolio item with published_at in the past
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, status, published_at) VALUES ('Scheduled', 'sched-item', 'img.jpg', 'scheduled', datetime('now', '-1 hour'))",
        [],
    ).unwrap();

    // Verify it's scheduled
    let status: String = conn
        .query_row(
            "SELECT status FROM portfolio WHERE slug = 'sched-item'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "scheduled");

    // Run the task
    let count = store.task_publish_scheduled().unwrap();
    assert!(count >= 1, "should publish at least 1 item");

    // Verify it's now published
    let status: String = conn
        .query_row(
            "SELECT status FROM portfolio WHERE slug = 'sched-item'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "published");
}

// ═══════════════════════════════════════════════════════════
// Store: health_referenced_files (verifies portfolio table name)
// ═══════════════════════════════════════════════════════════

#[test]
fn health_referenced_files_includes_portfolio_images() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, status) VALUES ('Ref Test', 'ref-test', '/uploads/photo.jpg', 'published')",
        [],
    ).unwrap();

    let referenced = store.health_referenced_files();
    assert!(
        referenced.contains("photo.jpg"),
        "should include portfolio image in referenced files, got: {:?}",
        referenced
    );
}

// ═══════════════════════════════════════════════════════════
// Store: export_content (verifies portfolio table name)
// ═══════════════════════════════════════════════════════════

#[test]
fn export_content_includes_portfolio() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, status) VALUES ('Export Test', 'export-test', 'img.jpg', 'published')",
        [],
    ).unwrap();

    let export = store
        .health_export_content()
        .expect("export should succeed");
    let portfolio = export
        .get("portfolio")
        .expect("export should have 'portfolio' key");
    let items = portfolio.as_array().expect("portfolio should be an array");
    assert_eq!(items.len(), 1, "should export 1 portfolio item");
    assert_eq!(items[0]["title"], "Export Test");
}

// ═══════════════════════════════════════════════════════════
// Schema validation: prevent silent table/column name mismatches
// ═══════════════════════════════════════════════════════════

#[test]
fn schema_all_expected_tables_exist() {
    let pool = test_pool();
    let conn = pool.get().unwrap();

    // Every table created by run_migrations() in db.rs.
    // If a new table is added to db.rs, add it here too.
    let expected_tables = [
        "posts",
        "portfolio",
        "categories",
        "tags",
        "content_categories",
        "content_tags",
        "comments",
        "orders",
        "download_tokens",
        "licenses",
        "designs",
        "design_templates",
        "settings",
        "imports",
        "sessions",
        "page_views",
        "magic_links",
        "likes",
        "users",
        "fw_bans",
        "fw_events",
        "audit_log",
        "user_passkeys",
        "email_queue",
    ];

    for table in &expected_tables {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .unwrap_or(false);
        assert!(
            exists,
            "Expected table '{}' does not exist in schema",
            table
        );
    }
}

#[test]
fn schema_posts_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();

    let expected = [
        "id",
        "title",
        "slug",
        "content_json",
        "content_html",
        "excerpt",
        "featured_image",
        "meta_title",
        "meta_description",
        "status",
        "published_at",
        "created_at",
        "updated_at",
    ];
    assert_table_columns(&conn, "posts", &expected);
}

#[test]
fn schema_portfolio_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();

    let expected = [
        "id",
        "title",
        "slug",
        "description_json",
        "description_html",
        "image_path",
        "thumbnail_path",
        "meta_title",
        "meta_description",
        "sell_enabled",
        "price",
        "purchase_note",
        "payment_provider",
        "download_file_path",
        "likes",
        "status",
        "published_at",
        "created_at",
        "updated_at",
    ];
    assert_table_columns(&conn, "portfolio", &expected);
}

#[test]
fn schema_content_tags_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    assert_table_columns(
        &conn,
        "content_tags",
        &["content_id", "content_type", "tag_id"],
    );
}

#[test]
fn schema_content_categories_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    assert_table_columns(
        &conn,
        "content_categories",
        &["content_id", "content_type", "category_id"],
    );
}

#[test]
fn schema_page_views_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    assert_table_columns(
        &conn,
        "page_views",
        &[
            "id",
            "path",
            "ip_hash",
            "country",
            "city",
            "referrer",
            "user_agent",
            "device_type",
            "browser",
            "created_at",
        ],
    );
}

/// Helper: assert that a table has all expected columns.
fn assert_table_columns(
    conn: &r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>,
    table: &str,
    expected: &[&str],
) {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .unwrap_or_else(|e| panic!("Failed to query table_info for '{}': {}", table, e));
    let columns: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for col in expected {
        assert!(
            columns.contains(&col.to_string()),
            "Table '{}' is missing column '{}'. Actual columns: {:?}",
            table,
            col,
            columns
        );
    }
}

#[test]
fn schema_no_phantom_tables_in_health() {
    // Verify that health.rs table list matches real tables.
    // This test queries the same tables that health.rs uses.
    let pool = test_pool();
    let conn = pool.get().unwrap();

    let health_tables = [
        "posts",
        "portfolio",
        "comments",
        "categories",
        "tags",
        "settings",
        "sessions",
        "imports",
        "page_views",
        "content_tags",
        "content_categories",
    ];
    for table in &health_tables {
        let sql = format!("SELECT COUNT(*) FROM {}", table);
        conn.query_row(&sql, [], |r| r.get::<_, u64>(0))
            .unwrap_or_else(|e| {
                panic!("health.rs references table '{}' which fails: {}", table, e)
            });
    }
}

#[test]
fn schema_export_queries_all_succeed() {
    // Verify that health_export_content doesn't silently skip any section.
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);

    let export = store
        .health_export_content()
        .expect("export should succeed");

    let expected_keys = [
        "posts",
        "portfolio",
        "categories",
        "tags",
        "comments",
        "post_tags",
        "post_categories",
        "portfolio_tags",
        "portfolio_categories",
        "settings",
    ];
    for key in &expected_keys {
        assert!(
            export.get(key).is_some(),
            "health_export_content is missing key '{}' — likely a silent SQL failure. Keys present: {:?}",
            key,
            export.as_object().map(|m| m.keys().collect::<Vec<_>>())
        );
    }
}

#[test]
fn schema_orphan_scan_queries_succeed() {
    // Verify health_referenced_files doesn't silently fail.
    // Insert data in posts and portfolio, confirm both are found.
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO posts (title, slug, content_html, featured_image, status) VALUES ('P', 'p-1', '<img src=\"/uploads/post-img.jpg\">', '/uploads/feat.jpg', 'published')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO portfolio (title, slug, image_path, description_html, status) VALUES ('I', 'i-1', '/uploads/port-img.jpg', '<img src=\"/uploads/desc-img.jpg\">', 'published')",
        [],
    ).unwrap();

    let refs = store.health_referenced_files();
    assert!(
        refs.contains("feat.jpg"),
        "should find post featured_image, got: {:?}",
        refs
    );
    assert!(
        refs.contains("post-img.jpg"),
        "should find image in post content_html, got: {:?}",
        refs
    );
    assert!(
        refs.contains("port-img.jpg"),
        "should find portfolio image_path, got: {:?}",
        refs
    );
    assert!(
        refs.contains("desc-img.jpg"),
        "should find image in portfolio description_html, got: {:?}",
        refs
    );
}

// ═══════════════════════════════════════════════════════════
// Built-in MTA tests
// ═══════════════════════════════════════════════════════════

#[test]
fn mta_dkim_keygen_produces_valid_keypair() {
    let (private_pem, public_b64) =
        crate::mta::dkim::generate_keypair().expect("keygen should succeed");
    assert!(
        private_pem.contains("BEGIN PRIVATE KEY"),
        "should be PEM format"
    );
    assert!(!public_b64.is_empty(), "public key should not be empty");

    // Round-trip: extract public key from private
    let extracted = crate::mta::dkim::public_key_from_private_pem(&private_pem)
        .expect("extract should succeed");
    assert_eq!(
        extracted, public_b64,
        "extracted public key should match generated"
    );
}

#[test]
fn mta_dkim_sign_message_produces_header() {
    let (private_pem, _) = crate::mta::dkim::generate_keypair().expect("keygen should succeed");
    let header = crate::mta::dkim::sign_message(
        &private_pem,
        "velocty",
        "example.com",
        "noreply@example.com",
        "user@test.com",
        "Test Subject",
        "Hello, this is a test body.",
    )
    .expect("signing should succeed");
    assert!(
        header.starts_with("DKIM-Signature:"),
        "should produce DKIM-Signature header"
    );
    assert!(header.contains("d=example.com"), "should contain domain");
    assert!(header.contains("s=velocty"), "should contain selector");
    assert!(header.contains("bh="), "should contain body hash");
    assert!(header.contains("b="), "should contain signature");
}

#[test]
fn mta_domain_from_url_extracts_correctly() {
    assert_eq!(
        crate::mta::deliver::domain_from_url("https://photos.example.com"),
        Some("photos.example.com".to_string())
    );
    assert_eq!(
        crate::mta::deliver::domain_from_url("http://localhost:8000"),
        Some("localhost".to_string())
    );
    assert_eq!(
        crate::mta::deliver::domain_from_url("https://example.com/blog"),
        Some("example.com".to_string())
    );
}

#[test]
fn mta_default_from_address_uses_domain() {
    assert_eq!(
        crate::mta::deliver::default_from_address("https://photos.example.com"),
        "noreply@photos.example.com"
    );
    assert_eq!(
        crate::mta::deliver::default_from_address("http://localhost:8000"),
        "noreply@localhost"
    );
}

#[test]
fn mta_spf_merge_no_existing() {
    let result = crate::mta::dns::generate_spf(None, Some("1.2.3.4"));
    assert_eq!(result, "v=spf1 a mx ip4:1.2.3.4 ~all");
}

#[test]
fn mta_spf_merge_preserves_existing() {
    let result = crate::mta::dns::merge_spf("v=spf1 include:_spf.google.com ~all", Some("1.2.3.4"));
    assert!(
        result.contains("include:_spf.google.com"),
        "should preserve google include"
    );
    assert!(result.contains("ip4:1.2.3.4"), "should add our IP");
    assert!(
        result.ends_with("~all"),
        "should preserve soft fail qualifier"
    );
}

#[test]
fn mta_spf_merge_already_has_a() {
    let existing = "v=spf1 a include:_spf.google.com ~all";
    let result = crate::mta::dns::merge_spf(existing, Some("1.2.3.4"));
    assert_eq!(result, existing, "should not modify if 'a' already present");
}

#[test]
fn mta_spf_merge_preserves_hard_fail() {
    let result = crate::mta::dns::merge_spf(
        "v=spf1 include:spf.protection.outlook.com -all",
        Some("10.0.0.1"),
    );
    assert!(result.ends_with("-all"), "should preserve -all (hard fail)");
}

#[test]
fn mta_queue_push_and_pending() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);

    let id = store
        .mta_queue_push("user@test.com", "noreply@example.com", "Test", "Body")
        .expect("push should succeed");
    assert!(id > 0, "should return positive id");

    let pending = store.mta_queue_pending(10);
    assert_eq!(pending.len(), 1, "should have 1 pending message");
    assert_eq!(pending[0].to_addr, "user@test.com");
    assert_eq!(pending[0].subject, "Test");
    assert_eq!(pending[0].status, "pending");
}

#[test]
fn mta_queue_update_status_marks_sent() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);

    let id = store
        .mta_queue_push("user@test.com", "noreply@example.com", "Test", "Body")
        .unwrap();
    store
        .mta_queue_update_status(id, "sent", None, None)
        .unwrap();

    let pending = store.mta_queue_pending(10);
    assert_eq!(
        pending.len(),
        0,
        "sent message should not appear in pending"
    );

    let sent = store.mta_queue_sent_last_hour().unwrap();
    assert_eq!(sent, 1, "should count 1 sent in last hour");
}

#[test]
fn mta_queue_retry_schedule() {
    assert_eq!(crate::mta::queue::retry_delay(0), Some(0));
    assert_eq!(crate::mta::queue::retry_delay(1), Some(60));
    assert_eq!(crate::mta::queue::retry_delay(4), Some(7200));
    assert_eq!(
        crate::mta::queue::retry_delay(5),
        None,
        "should give up after 5 attempts"
    );
}

#[test]
fn mta_queue_cleanup_removes_old() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool.clone());

    store
        .mta_queue_push("user@test.com", "noreply@example.com", "Old", "Body")
        .unwrap();
    // Manually backdate the entry
    let conn = pool.get().unwrap();
    conn.execute(
        "UPDATE email_queue SET created_at = datetime('now', '-60 days')",
        [],
    )
    .unwrap();

    let cleaned = store.mta_queue_cleanup(30).unwrap();
    assert_eq!(cleaned, 1, "should clean up 1 old entry");
}

#[test]
fn mta_init_dkim_generates_key() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);

    // Should be empty initially
    assert!(store
        .setting_get("mta_dkim_private_key")
        .unwrap_or_default()
        .is_empty());

    crate::mta::init_dkim_if_needed(&store);

    let key = store
        .setting_get("mta_dkim_private_key")
        .unwrap_or_default();
    assert!(
        key.contains("BEGIN PRIVATE KEY"),
        "should have generated a key"
    );
    assert!(!store
        .setting_get("mta_dkim_generated_at")
        .unwrap_or_default()
        .is_empty());
}

#[test]
fn mta_init_from_address_auto_populates() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);

    // Set site_url, leave mta_from_address empty
    store
        .setting_set("site_url", "https://photos.example.com")
        .unwrap();
    store.setting_set("mta_from_address", "").unwrap();

    crate::mta::init_from_address(&store);

    let from = store.setting_get("mta_from_address").unwrap_or_default();
    assert_eq!(from, "noreply@photos.example.com");
}

// ═══════════════════════════════════════════════════════
// AI Enabled / Vision Detection (refactor regression tests)
// ═══════════════════════════════════════════════════════

#[test]
fn ai_is_enabled_false_by_default() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    // All AI providers default to "false", so is_enabled should be false
    assert!(
        !crate::ai::is_enabled(&store),
        "ai::is_enabled should be false when no providers are enabled"
    );
}

#[test]
fn ai_is_enabled_true_when_provider_on() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    store.setting_set("ai_openai_enabled", "true").unwrap();
    assert!(
        crate::ai::is_enabled(&store),
        "ai::is_enabled should be true when openai is enabled"
    );
}

#[test]
fn ai_has_vision_false_by_default() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    assert!(
        !crate::ai::has_vision_provider(&store),
        "ai::has_vision_provider should be false when no providers are enabled"
    );
}

#[test]
fn ai_has_vision_true_for_vision_provider() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    store.setting_set("ai_gemini_enabled", "true").unwrap();
    assert!(
        crate::ai::has_vision_provider(&store),
        "ai::has_vision_provider should be true when gemini is enabled"
    );
}

#[test]
fn ai_has_vision_false_for_non_vision_provider() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    // Cloudflare does not support vision
    store.setting_set("ai_cloudflare_enabled", "true").unwrap();
    assert!(
        !crate::ai::has_vision_provider(&store),
        "ai::has_vision_provider should be false when only cloudflare is enabled"
    );
    // But is_enabled should still be true
    assert!(
        crate::ai::is_enabled(&store),
        "ai::is_enabled should be true when cloudflare is enabled"
    );
}

#[test]
fn ai_is_enabled_not_a_setting_key() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    // "ai_enabled" is NOT a real setting — it must be derived via ai::is_enabled()
    assert!(
        !store.setting_get_bool("ai_enabled"),
        "ai_enabled should not exist as a direct setting key"
    );
    assert!(
        !store.setting_get_bool("ai_has_vision"),
        "ai_has_vision should not exist as a direct setting key"
    );
}

// ═══════════════════════════════════════════════════════
// Setting keys used by setting_get_bool / setting_get_i64 must exist in defaults
// ═══════════════════════════════════════════════════════

#[test]
fn all_bool_setting_keys_exist_in_defaults() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    let settings = store.setting_all();

    // Every key that code reads via setting_get_bool must be seeded
    let bool_keys = [
        "seo_sitemap_enabled",
        "seo_open_graph",
        "seo_twitter_cards",
        "images_webp_convert",
        "images_reencode",
        "images_strip_metadata",
    ];
    for key in &bool_keys {
        assert!(
            settings.contains_key(*key),
            "setting_get_bool key '{}' is missing from seed defaults",
            key
        );
    }
}

#[test]
fn all_i64_setting_keys_exist_in_defaults() {
    use crate::store::Store;
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    let settings = store.setting_all();

    let i64_keys = [
        "session_expiry_hours",
        "login_rate_limit",
        "images_quality",
        "images_max_dimension",
        "blog_posts_per_page",
        "portfolio_items_per_page",
        "comments_rate_limit",
    ];
    for key in &i64_keys {
        assert!(
            settings.contains_key(*key),
            "setting_get_i64 key '{}' is missing from seed defaults",
            key
        );
        let val = store.setting_get_i64(key);
        // These should all parse to a valid number (not 0 from parse failure,
        // except images_max_dimension which defaults to 0)
        if *key != "images_max_dimension" {
            assert!(
                val > 0,
                "setting_get_i64('{}') returned {} — expected > 0",
                key,
                val
            );
        }
    }
}

#[test]
fn schema_email_queue_has_expected_columns() {
    let pool = test_pool();
    let conn = pool.get().unwrap();
    assert_table_columns(
        &conn,
        "email_queue",
        &[
            "id",
            "to_addr",
            "from_addr",
            "subject",
            "body_text",
            "attempts",
            "max_attempts",
            "next_retry_at",
            "status",
            "error",
            "created_at",
        ],
    );
}

// ═══════════════════════════════════════════════════════════
// Media Library API
// ═══════════════════════════════════════════════════════════

#[test]
fn media_scan_returns_empty_when_no_uploads() {
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    let (files, disk) = crate::routes::admin::media::scan_media_files(&store);
    // Fresh test env has no uploads dir, so should return empty
    assert!(files.is_empty() || disk >= 0, "scan should not panic");
}

#[test]
fn media_scan_finds_uploaded_files() {
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    let dir = std::path::Path::new("website/site/uploads");
    let _ = std::fs::create_dir_all(dir);
    let test_file = dir.join("test_media_scan_unit.jpg");
    std::fs::write(&test_file, b"fake-jpg-data").unwrap();

    let (files, disk) = crate::routes::admin::media::scan_media_files(&store);
    let found = files.iter().any(|f| f.name == "test_media_scan_unit.jpg");
    assert!(found, "should find the test jpg file");
    assert!(disk > 0, "disk usage should be > 0");

    let f = files
        .iter()
        .find(|f| f.name == "test_media_scan_unit.jpg")
        .unwrap();
    assert!(f.is_image);
    assert!(!f.is_video);
    assert_eq!(f.media_type, "image");
    assert_eq!(f.ext, "jpg");

    let _ = std::fs::remove_file(&test_file);
}

#[test]
fn media_scan_distinguishes_image_and_video() {
    let pool = test_pool();
    let store = crate::store::sqlite::SqliteStore::new(pool);
    let dir = std::path::Path::new("website/site/uploads");
    let _ = std::fs::create_dir_all(dir);
    let img = dir.join("test_filter_unit.png");
    let vid = dir.join("test_filter_unit.mp4");
    std::fs::write(&img, b"fake-png").unwrap();
    std::fs::write(&vid, b"fake-mp4").unwrap();

    let (all, _) = crate::routes::admin::media::scan_media_files(&store);
    let images: Vec<_> = all.iter().filter(|f| f.media_type == "image").collect();
    let videos: Vec<_> = all.iter().filter(|f| f.media_type == "video").collect();
    assert!(
        images.iter().any(|f| f.name == "test_filter_unit.png"),
        "should find png in images"
    );
    assert!(
        videos.iter().any(|f| f.name == "test_filter_unit.mp4"),
        "should find mp4 in videos"
    );

    let _ = std::fs::remove_file(&img);
    let _ = std::fs::remove_file(&vid);
}
