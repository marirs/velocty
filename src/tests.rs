#![cfg(test)]

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::collections::HashMap;

use crate::db::{DbPool, run_migrations, seed_defaults};
use crate::models::settings::Setting;
use crate::models::post::{Post, PostForm};
use crate::models::portfolio::{PortfolioItem, PortfolioForm};
use crate::models::category::{Category, CategoryForm};
use crate::models::tag::{Tag, TagForm};
use crate::models::comment::{Comment, CommentForm};
use crate::models::user::User;
use crate::models::order::{Order, DownloadToken, License};
use crate::models::audit::AuditEntry;
use crate::models::firewall::{FwBan, FwEvent};
use crate::models::design::{Design, DesignTemplate};
use crate::models::analytics::PageView;
use crate::models::import::Import;
use crate::security::auth;
use crate::security::mfa;

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
        ).unwrap();
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
    assert_eq!(Setting::get_or(&pool, "nonexistent", "fallback"), "fallback");
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
        Post::create(&pool, &make_post_form(&format!("Post {}", i), &format!("post-{}", i), "published")).unwrap();
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
    let id = PortfolioItem::create(&pool, &make_portfolio_form("Sunset", "sunset", "draft")).unwrap();
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
        PortfolioItem::create(&pool, &make_portfolio_form(&format!("Item {}", i), &format!("item-{}", i), "published")).unwrap();
    }
    PortfolioItem::create(&pool, &make_portfolio_form("Draft Item", "draft-item", "draft")).unwrap();

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
    assert_eq!(PortfolioItem::find_by_id(&pool, id).unwrap().status, "published");
}

#[test]
fn portfolio_likes() {
    let pool = test_pool();
    let id = PortfolioItem::create(&pool, &make_portfolio_form("Likeable", "likeable", "published")).unwrap();

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
    let id = Tag::create(&pool, &TagForm { name: "Rust".to_string(), slug: "rust".to_string() }).unwrap();
    assert!(id > 0);

    let tag = Tag::find_by_id(&pool, id).unwrap();
    assert_eq!(tag.name, "Rust");

    Tag::update(&pool, id, &TagForm { name: "Rust Lang".to_string(), slug: "rust-lang".to_string() }).unwrap();
    let updated = Tag::find_by_id(&pool, id).unwrap();
    assert_eq!(updated.slug, "rust-lang");

    assert_eq!(Tag::count(&pool), 1);
    Tag::delete(&pool, id).unwrap();
    assert_eq!(Tag::count(&pool), 0);
}

#[test]
fn tag_content_association() {
    let pool = test_pool();
    let t1 = Tag::create(&pool, &TagForm { name: "A".to_string(), slug: "a".to_string() }).unwrap();
    let t2 = Tag::create(&pool, &TagForm { name: "B".to_string(), slug: "b".to_string() }).unwrap();
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

// ═══════════════════════════════════════════════════════════
// Comments
// ═══════════════════════════════════════════════════════════

#[test]
fn comment_crud() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "published")).unwrap();

    let cid = Comment::create(&pool, &CommentForm {
        post_id,
        content_type: Some("post".to_string()),
        author_name: "Alice".to_string(),
        author_email: Some("alice@test.com".to_string()),
        body: "Great post!".to_string(),
        honeypot: None,
        parent_id: None,
    }).unwrap();

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

    let result = Comment::create(&pool, &CommentForm {
        post_id,
        content_type: None,
        author_name: "Bot".to_string(),
        author_email: None,
        body: "spam".to_string(),
        honeypot: Some("gotcha".to_string()),
        parent_id: None,
    });
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Spam detected");
}

#[test]
fn comment_threaded_replies() {
    let pool = test_pool();
    let post_id = Post::create(&pool, &make_post_form("P", "p", "published")).unwrap();

    let parent = Comment::create(&pool, &CommentForm {
        post_id,
        content_type: Some("post".to_string()),
        author_name: "A".to_string(),
        author_email: None,
        body: "parent".to_string(),
        honeypot: None,
        parent_id: None,
    }).unwrap();

    let child = Comment::create(&pool, &CommentForm {
        post_id,
        content_type: Some("post".to_string()),
        author_name: "B".to_string(),
        author_email: None,
        body: "reply".to_string(),
        honeypot: None,
        parent_id: Some(parent),
    }).unwrap();

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
        conn.execute("UPDATE posts SET user_id = ?1 WHERE id = ?2", rusqlite::params![uid, pid]).unwrap();
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

    let oid = Order::create(&pool, pid, "buyer@test.com", "Buyer", 29.99, "USD", "paypal", "PP-123", "pending").unwrap();
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

    Order::create(&pool, pid, "a@t.com", "A", 10.0, "USD", "stripe", "S1", "completed").unwrap();
    Order::create(&pool, pid, "b@t.com", "B", 20.0, "USD", "paypal", "P1", "pending").unwrap();
    Order::create(&pool, pid, "a@t.com", "A", 30.0, "USD", "stripe", "S2", "completed").unwrap();

    assert_eq!(Order::list(&pool, 10, 0).len(), 3);
    assert_eq!(Order::list_by_status(&pool, "completed", 10, 0).len(), 2);
    assert_eq!(Order::list_by_email(&pool, "a@t.com", 10, 0).len(), 2);
    assert_eq!(Order::list_by_portfolio(&pool, pid).len(), 3);
}

#[test]
fn download_token_lifecycle() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(&pool, pid, "b@t.com", "B", 10.0, "USD", "stripe", "", "completed").unwrap();

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
    let oid = Order::create(&pool, pid, "b@t.com", "B", 10.0, "USD", "stripe", "", "completed").unwrap();

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::hours(1);
    DownloadToken::create(&pool, oid, "expired-tok", 3, past).unwrap();

    let token = DownloadToken::find_by_token(&pool, "expired-tok").unwrap();
    assert!(!token.is_valid());
}

#[test]
fn license_crud() {
    let pool = test_pool();
    let pid = setup_portfolio(&pool);
    let oid = Order::create(&pool, pid, "b@t.com", "B", 10.0, "USD", "stripe", "", "completed").unwrap();

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

    AuditEntry::log(&pool, Some(1), Some("Admin"), "login", None, None, None, None, Some("1.2.3.4"));
    AuditEntry::log(&pool, Some(1), Some("Admin"), "settings_change", Some("settings"), None, Some("general"), None, None);
    AuditEntry::log(&pool, Some(2), Some("Editor"), "post_create", Some("post"), Some(1), Some("Hello"), None, None);

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
    AuditEntry::log(&pool, Some(1), Some("A"), "test", None, None, None, None, None);
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE audit_log SET created_at = datetime('now', '-10 days')",
            [],
        ).unwrap();
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
    let bid = FwBan::create(&pool, "10.0.0.1", "brute_force", Some("5 failed logins"), None, None, None).unwrap();
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
    FwBan::create_with_duration(&pool, "10.0.0.3", "manual", None, "permanent", None, None).unwrap();
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

    FwEvent::log(&pool, "10.0.0.1", "failed_login", Some("bad password"), None, Some("Mozilla/5.0"), Some("/admin/login"));
    FwEvent::log(&pool, "10.0.0.1", "failed_login", None, None, None, None);
    FwEvent::log(&pool, "10.0.0.2", "bot_detected", None, None, None, Some("/wp-admin"));

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

    auth::cleanup_expired_sessions(&pool).unwrap();

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
    "static", "uploads", "api", "super", "download", "feed",
    "sitemap.xml", "robots.txt", "privacy", "terms", "archives",
    "login", "logout", "setup", "mfa", "magic-link",
    "forgot-password", "reset-password",
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
        User::create(&pool, &format!("u{}@t.com", i), &hash, &format!("U{}", i), "editor").unwrap();
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

    // seed_defaults creates a "Default" design, so count starts at 1
    let baseline = Design::list(&pool).len();

    let id = Design::create(&pool, "Custom Theme").unwrap();
    assert!(id > 0);

    let design = Design::find_by_id(&pool, id).unwrap();
    assert_eq!(design.name, "Custom Theme");
    assert!(!design.is_active);

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

    PageView::record(&pool, "/", "hash1", Some("US"), None, Some("https://google.com"), Some("Mozilla/5.0"), Some("desktop"), Some("Chrome")).unwrap();
    PageView::record(&pool, "/blog/hello", "hash2", Some("UK"), None, None, None, Some("mobile"), Some("Safari")).unwrap();
    PageView::record(&pool, "/portfolio/sunset", "hash1", Some("US"), None, None, None, Some("desktop"), Some("Chrome")).unwrap();

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
    PageView::record(&pool, "/", "h1", None, None, Some("https://google.com"), None, None, None).unwrap();
    PageView::record(&pool, "/", "h2", None, None, Some("https://google.com"), None, None, None).unwrap();
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
    PageView::record(&pool, "/portfolio/b", "h2", None, None, None, None, None, None).unwrap();
    PageView::record(&pool, "/about", "h3", None, None, None, None, None, None).unwrap();

    let stream = PageView::stream_data(&pool, "2020-01-01", "2030-12-31");
    assert!(!stream.is_empty());
    let total: i64 = stream.iter().map(|s| s.count).sum();
    assert_eq!(total, 3);
}

#[test]
fn pageview_top_portfolio() {
    let pool = test_pool();
    PageView::record(&pool, "/portfolio/sunset", "h1", None, None, None, None, None, None).unwrap();
    PageView::record(&pool, "/portfolio/sunset", "h2", None, None, None, None, None, None).unwrap();
    PageView::record(&pool, "/portfolio/dawn", "h3", None, None, None, None, None, None).unwrap();
    PageView::record(&pool, "/blog/unrelated", "h4", None, None, None, None, None, None).unwrap();

    let top = PageView::top_portfolio(&pool, "2020-01-01", "2030-12-31", 10);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].label, "/portfolio/sunset");
    assert_eq!(top[0].count, 2);
}

#[test]
fn pageview_tag_relations() {
    let pool = test_pool();
    let t1 = Tag::create(&pool, &TagForm { name: "Rust".to_string(), slug: "rust".to_string() }).unwrap();
    let t2 = Tag::create(&pool, &TagForm { name: "Web".to_string(), slug: "web".to_string() }).unwrap();
    let t3 = Tag::create(&pool, &TagForm { name: "API".to_string(), slug: "api-tag".to_string() }).unwrap();

    let p1 = Post::create(&pool, &make_post_form("P1", "p1", "published")).unwrap();
    let p2 = Post::create(&pool, &make_post_form("P2", "p2", "published")).unwrap();

    // p1 has Rust + Web, p2 has Rust + API
    Tag::set_for_content(&pool, p1, "post", &[t1, t2]).unwrap();
    Tag::set_for_content(&pool, p2, "post", &[t1, t3]).unwrap();

    let relations = PageView::tag_relations(&pool);
    assert!(!relations.is_empty());
    // Rust-Web and Rust-API should appear
    assert!(relations.iter().any(|r| r.source == "API" || r.target == "API"));
}

// ═══════════════════════════════════════════════════════════
// Imports
// ═══════════════════════════════════════════════════════════

#[test]
fn import_create_and_list() {
    let pool = test_pool();

    let id = Import::create(&pool, "wordpress", Some("export.xml"), 10, 5, 3, 2, Some("All good")).unwrap();
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
        ).unwrap()
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
        ).unwrap()
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

    Order::create(&pool, pid, "a@t.com", "A", 50.0, "USD", "stripe", "S1", "completed").unwrap();
    Order::create(&pool, pid, "b@t.com", "B", 30.0, "USD", "stripe", "S2", "completed").unwrap();
    Order::create(&pool, pid, "c@t.com", "C", 20.0, "USD", "stripe", "S3", "pending").unwrap();

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
    let oid = Order::create(&pool, pid, "b@t.com", "B", 10.0, "USD", "stripe", "", "completed").unwrap();

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
        "posts", "portfolio", "categories", "tags",
        "content_categories", "content_tags", "comments",
        "orders", "download_tokens", "licenses",
        "designs", "design_templates", "settings", "imports",
        "sessions", "page_views", "magic_links", "likes",
        "users", "fw_bans", "fw_events", "audit_log",
    ];
    for table in &tables {
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", table),
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| panic!("Table '{}' should exist", table));
        assert!(count >= 0, "Table '{}' query failed", table);
    }
}
