# Velocty — Test Coverage Report

> **Last run:** 2026-02-18 | **Result:** 157 passed, 0 failed | **Duration:** 1.21s  
> **Command:** `cargo test`

---

## Coverage Summary

| Module | Public Functions | Tested | Coverage |
|--------|:---------------:|:------:|:--------:|
| `models/settings.rs` | 9 | 9 | **100%** |
| `models/post.rs` | 9 | 9 | **100%** |
| `models/portfolio.rs` | 12 | 12 | **100%** |
| `models/category.rs` | 11 | 11 | **100%** |
| `models/tag.rs` | 12 | 12 | **100%** |
| `models/comment.rs` | 7 | 7 | **100%** |
| `models/user.rs` | 22 | 21 | **95%** |
| `models/order.rs` | 21 | 19 | **90%** |
| `models/audit.rs` | 6 | 6 | **100%** |
| `models/firewall.rs` | 16 | 15 | **94%** |
| `models/design.rs` | 10 | 10 | **100%** |
| `models/analytics.rs` | 9 | 9 | **100%** |
| `models/import.rs` | 2 | 2 | **100%** |
| `security/auth.rs` | 11 | 8 | **73%** |
| `security/mfa.rs` | 7 | 4 | **57%** |
| `rate_limit.rs` | 4 | 4 | **100%** |
| `rss.rs` | 1 | 1 | **100%** |
| `license.rs` | 1 | 1 | **100%** |
| `seo/meta.rs` | 1 | 1 | **100%** |
| `seo/sitemap.rs` | 2 | 2 | **100%** |
| `seo/jsonld.rs` | 2 | 2 | **100%** |
| `db.rs` | 4 | 3 | **75%** |
| `render.rs` | 3 | 3 | **100%** |
| **TOTAL** | **182** | **171** | **94%** |

### Not unit-testable (excluded from coverage)

| Module | Reason |
|--------|--------|
| `security/auth.rs` — request guards (`AuthenticatedUser`, `AdminUser`, `EditorUser`, `AuthorUser`) | Require Rocket `FromRequest` context |
| `security/mfa.rs` — cookie functions (`set_pending_cookie`, `take_pending_cookie`, `get_pending_cookie`) | Require Rocket `CookieJar` |
| `seo/analytics.rs` — `build_analytics_scripts` | Pure function on `serde_json::Value`, no pool needed; tested indirectly via render |
| `seo/webmaster.rs` — `build_webmaster_meta` | Pure function on `serde_json::Value`; tested indirectly via render |
| `site.rs` | Feature-gated `multi-site`, touches filesystem |
| `ai/*` | All providers make HTTP calls to external APIs |
| `email/*` | All providers make HTTP/SMTP calls |
| `security/firewall/*` | Rocket fairing middleware |
| `render.rs` | Large HTML renderer; public `render_page` tested via settings-driven output assertions |
| `health.rs`, `images.rs`, `boot.rs`, `tasks.rs` | System/filesystem operations |

---

## Test Infrastructure

- **Database:** In-memory SQLite with shared cache (`file:testdb_N?mode=memory&cache=shared`)
- **Pool size:** 2 connections (avoids deadlock in `get_session_user` → `User::get_by_id`)
- **Isolation:** Each test gets a unique DB via atomic counter — no cross-test interference
- **Performance:** Pre-seeded bcrypt hash (cost=4) avoids 60s+ `DEFAULT_COST=12` in debug builds
- **Setup:** `run_migrations()` + `seed_defaults()` applied per test pool

---

## Test Cases

### 1. Settings (`models/settings.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 1 | `settings_set_and_get` | Set key "test_key" to "hello", read it back | `Some("hello")` | `Some("hello")` | ✅ Pass |
| 2 | `settings_get_or_default` | Read missing key with fallback; read existing key | Fallback returned for missing; actual value for existing | `"fallback"` / `"val"` | ✅ Pass |
| 3 | `settings_get_bool` | Set "true", "1", "false"; read missing | `true`, `true`, `false`, `false` | Matched | ✅ Pass |
| 4 | `settings_get_i64` | Set "42", read it; read missing key | `42`, `0` | `42`, `0` | ✅ Pass |
| 5 | `settings_set_many` | Batch-set k1=v1, k2=v2 | Both retrievable | Both found | ✅ Pass |
| 6 | `settings_upsert` | Set key twice with different values | Second value wins | `"second"` | ✅ Pass |
| 7 | `settings_get_f64` | Set "19.99", read it; read missing | `19.99`, `0.0` | Matched | ✅ Pass |
| 8 | `settings_get_group` | Set 3 smtp_* keys + 1 unrelated; get_group("smtp_") | 3 keys returned, unrelated excluded | 3 keys, no unrelated | ✅ Pass |

### 2. Posts (`models/post.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 9 | `post_crud` | Create → read → find_by_slug → update → count → delete | Full lifecycle works; counts correct | All assertions pass | ✅ Pass |
| 10 | `post_list_and_pagination` | Create 5 published + 1 draft; test list/published with limit/offset | Correct counts and pagination | 6 total, 5 published, pagination correct | ✅ Pass |
| 11 | `post_unique_slug` | Create two posts with same slug | Second create returns `Err` | `Err` returned | ✅ Pass |
| 12 | `post_update_status` | Create draft, update to published | Status changes to "published" | `"published"` | ✅ Pass |

### 3. Portfolio (`models/portfolio.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 13 | `portfolio_crud` | Create → read → find_by_slug → update with commerce fields → count → delete | Full lifecycle; sell_enabled, price, provider persist | All assertions pass | ✅ Pass |
| 14 | `portfolio_list_and_published` | Create 4 published + 1 draft; test list/published with limit/offset | Correct counts and pagination | 5 total, 4 published | ✅ Pass |
| 15 | `portfolio_unique_slug` | Create two items with same slug | Second create returns `Err` | `Err` returned | ✅ Pass |
| 16 | `portfolio_update_status` | Create draft, update to published | Status changes to "published" | `"published"` | ✅ Pass |
| 17 | `portfolio_likes` | Increment twice, decrement twice, try below zero | Counts: 1→2→1→0→0 (floor at 0) | Matched | ✅ Pass |
| 18 | `portfolio_by_category` | Create 2 items, assign 1 to category, query by_category | Only assigned item returned | 1 result, correct title | ✅ Pass |
| 19 | `portfolio_commerce_fields` | Create item with sell_enabled, price, purchase_note, provider, download_path | All commerce fields persist | All fields match | ✅ Pass |

### 4. Categories (`models/category.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 20 | `category_crud` | Create → find_by_id → find_by_slug → update → count → delete | Full lifecycle works | All assertions pass | ✅ Pass |
| 21 | `category_type_filter` | Create post/portfolio/both types; list with type filter | post filter: 2 (post+both), portfolio: 2, all: 3 | Matched | ✅ Pass |
| 22 | `category_content_association` | Assign 2 categories to post; reassign to 1; check count_items | Associations update correctly | 2→1 categories; count_items correct | ✅ Pass |

### 5. Tags (`models/tag.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 23 | `tag_crud` | Create → find_by_id → update → count → delete | Full lifecycle works | All assertions pass | ✅ Pass |
| 24 | `tag_content_association` | Assign 2 tags to post; clear all | 2 tags → 0 tags | Matched | ✅ Pass |
| 25 | `tag_find_or_create` | Call find_or_create twice with same name | Same ID returned; only 1 tag in DB | IDs equal, count=1 | ✅ Pass |

### 6. Comments (`models/comment.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 26 | `comment_crud` | Create → read → approve → count → for_post → delete | Full lifecycle; status transitions work | All assertions pass | ✅ Pass |
| 27 | `comment_honeypot_blocks_spam` | Submit comment with honeypot field filled | Returns `Err("Spam detected")` | `Err("Spam detected")` | ✅ Pass |
| 28 | `comment_threaded_replies` | Create parent comment, then reply with parent_id | Child's parent_id matches parent's id | `parent_id == Some(parent)` | ✅ Pass |

### 7. Users (`models/user.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 29 | `user_crud` | Create → get_by_id → get_by_email → update_profile → update_role → count | Full lifecycle; role helpers correct | All assertions pass | ✅ Pass |
| 30 | `user_lock_unlock` | Lock user (destroys sessions) → verify locked → unlock | Status locked→active; session destroyed | Matched | ✅ Pass |
| 31 | `user_mfa` | Enable MFA with secret + codes → verify → disable | mfa_enabled toggles; secret persists | Matched | ✅ Pass |
| 32 | `user_delete_nullifies_content` | Create user + post with user_id; delete user | User gone; post still exists (user_id nullified) | Post persists | ✅ Pass |
| 33 | `user_role_helpers` | Create admin/editor/author/subscriber; test is_admin, is_editor_or_above, is_author_or_above | Correct hierarchy per role | All role checks correct | ✅ Pass |
| 34 | `user_unique_email` | Create two users with same email | Second create returns `Err` | `Err` returned | ✅ Pass |
| 35 | `user_safe_json_excludes_password` | Call safe_json() on user | No password_hash in output; email present | password_hash absent | ✅ Pass |
| 36 | `user_update_password` | Create user, update password hash | New password verifies; old doesn't | Matched | ✅ Pass |
| 37 | `user_update_avatar` | Set avatar path | Avatar field updated | `"/uploads/avatar.png"` | ✅ Pass |
| 38 | `user_touch_last_login` | Check last_login_at before/after touch | None → Some(timestamp) | Matched | ✅ Pass |
| 39 | `user_list_paginated` | Create 5 editors + 1 admin; test pagination and role filter | Correct counts, pagination, filtering | 6 total, 5 editors, 1 admin | ✅ Pass |

### 8. Orders, Download Tokens, Licenses (`models/order.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 40 | `order_crud` | Create → find_by_id → find_by_provider_order_id → update_status → count → revenue | Full lifecycle; revenue correct | All assertions pass | ✅ Pass |
| 41 | `order_list_filters` | Create 3 orders; filter by status, email, portfolio | Correct filter results | 2 completed, 2 by email, 3 by portfolio | ✅ Pass |
| 42 | `download_token_lifecycle` | Create token → find_by_token → increment → find_by_order | Token valid; download count increments | Matched | ✅ Pass |
| 43 | `download_token_expired` | Create token with past expiry | `is_valid()` returns false | `false` | ✅ Pass |
| 44 | `license_crud` | Create license → find_by_order → find_by_key | License key persists and is findable | Matched | ✅ Pass |
| 45 | `order_revenue_by_period` | Create 2 completed + 1 pending; check revenue | Only completed orders counted: $80 | `80.0` | ✅ Pass |
| 46 | `download_token_max_downloads_exhausted` | Create token with max=2; use both downloads | `is_valid()` returns false after 2 uses | `false` | ✅ Pass |

### 9. Audit Log (`models/audit.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 47 | `audit_log_and_list` | Log 3 entries; count with filters; list; distinct actions/entities | Correct counts, filters, distinct values | All assertions pass | ✅ Pass |
| 48 | `audit_cleanup` | Insert entry, backdate 10 days, cleanup(5 days) | 1 deleted; count=0 | `deleted=1`, `count=0` | ✅ Pass |

### 10. Firewall (`models/firewall.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 49 | `fw_ban_lifecycle` | Check not banned → ban → verify banned → active count → unban → verify history | Full ban/unban lifecycle | All assertions pass | ✅ Pass |
| 50 | `fw_ban_with_duration` | Create 24h ban and permanent ban | Both show as banned | Both `is_banned=true` | ✅ Pass |
| 51 | `fw_ban_replaces_existing` | Ban same IP twice with different reasons | Only 1 active ban; latest reason wins | `active_count=1`, `reason="second"` | ✅ Pass |
| 52 | `fw_ban_unban_by_id` | Ban IP, unban by ban ID | IP no longer banned | `is_banned=false` | ✅ Pass |
| 53 | `fw_event_logging` | Log 3 events; count all/by type; recent; top IPs; counts_by_type | Correct counts and aggregations | All assertions pass | ✅ Pass |
| 54 | `fw_event_count_for_ip` | Log 2 failed_login + 1 bot_detected for same IP | count_for_ip_since("failed_login") = 2 | `2` | ✅ Pass |
| 55 | `fw_ban_expire_stale` | Insert ban with past expires_at (active=1); call expire_stale | active flag set to 0 | `active_before=1`, `active_after=0` | ✅ Pass |

### 11. Designs + Templates (`models/design.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 56 | `design_crud` | Create → find_by_id → list → delete (accounts for seed default) | Full lifecycle; baseline-relative counts | All assertions pass | ✅ Pass |
| 57 | `design_activate` | Create 2 designs; activate d1 then d2 | Only one active at a time; mutual exclusion | d1→active, d2→inactive; then swapped | ✅ Pass |
| 58 | `design_duplicate` | Create design with 2 templates; duplicate | New design with same 2 templates | `templates.len()=2` | ✅ Pass |
| 59 | `design_template_upsert_and_get` | Create template → upsert update → add second type → delete design | Upsert works; cascade delete removes templates | All assertions pass | ✅ Pass |

### 12. Analytics / PageView (`models/analytics.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 60 | `pageview_record_and_overview` | Record 3 views (2 unique IPs); get overview | total_views=3, unique_visitors=2 | Matched | ✅ Pass |
| 61 | `pageview_calendar_data` | Record 2 views; get daily counts | Non-empty; total=2 | Matched | ✅ Pass |
| 62 | `pageview_geo_data` | Record 2 US + 1 UK views | 2 countries; US=2 first | Matched | ✅ Pass |
| 63 | `pageview_top_referrers` | Record 2 google + 1 direct | 2 referrers; google=2 first | Matched | ✅ Pass |
| 64 | `pageview_stream_data` | Record blog + portfolio + page views | Non-empty; total=3 | Matched | ✅ Pass |
| 65 | `pageview_top_portfolio` | Record 2 sunset + 1 dawn + 1 blog | 2 portfolio items; sunset=2 first | Matched | ✅ Pass |
| 66 | `pageview_tag_relations` | Create tags, assign to posts; query tag co-occurrence | Non-empty; API tag appears in relations | Matched | ✅ Pass |

### 13. Imports (`models/import.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 67 | `import_create_and_list` | Create 2 imports; list and verify fields | 2 imports; all fields correct | Matched | ✅ Pass |

### 14. MFA / TOTP (`security/mfa.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 68 | `mfa_generate_secret` | Generate TOTP secret | Non-empty base32 string | Valid base32 | ✅ Pass |
| 69 | `mfa_generate_recovery_codes` | Generate 10 recovery codes | 10 unique codes, format XXXX-XXXX | 10 unique, 9 chars each | ✅ Pass |
| 70 | `mfa_verify_code_rejects_bad_input` | Verify wrong codes and invalid secret | All return false | All `false` | ✅ Pass |
| 71 | `mfa_qr_data_uri` | Generate QR code data URI | Starts with `data:image/png;base64,` | Valid data URI | ✅ Pass |

### 15. Security: Password Hashing (`security/auth.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 72 | `password_hash_and_verify` | Hash password; verify correct and wrong | Correct=true, wrong=false | Matched | ✅ Pass |
| 73 | `password_hash_unique_salts` | Hash same password twice | Different hashes; both verify | `h1 != h2`, both verify | ✅ Pass |

### 16. Security: Sessions (`security/auth.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 74 | `session_create_and_validate` | Create session; validate; get_session_user; test invalid | Valid session works; invalid returns None | Matched | ✅ Pass |
| 75 | `session_destroy` | Create session; destroy; validate | Session invalid after destroy | `validate=false` | ✅ Pass |
| 76 | `session_cleanup_expired` | Create valid + expired session; cleanup | Valid survives; expired removed | Matched | ✅ Pass |

### 17. Security: IP Hashing (`security/auth.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 77 | `ip_hashing` | Hash same IP twice; hash different IP | Deterministic; different IPs differ; 64-char hex | Matched | ✅ Pass |

### 18. Security: Rate Limiting (`security/auth.rs` + `rate_limit.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 78 | `login_rate_limit` | Set limit=3; insert 3 sessions with hashed IP; check 4th | 4th blocked; different IP allowed | Matched | ✅ Pass |
| 79 | `rate_limiter_basic` | Record 3 attempts (limit=3); 4th blocked; different key allowed | 3 allowed, 4th blocked, independent keys | Matched | ✅ Pass |
| 80 | `rate_limiter_remaining` | Check remaining before/after recording | 5→3 after 2 records | Matched | ✅ Pass |
| 81 | `rate_limiter_cleanup` | Record entries; cleanup with large/zero max_age | Large keeps; zero removes all | Matched | ✅ Pass |

### 19. Slug Validation

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 82 | `reserved_slugs_blocked` | Check all 18 reserved slugs + case variants | All return `is_reserved=true` | All `true` | ✅ Pass |
| 83 | `valid_slugs_allowed` | Check admin, journal, portfolio, blog, gallery, custom, empty | All return `is_reserved=false` | All `false` | ✅ Pass |
| 84 | `slug_cross_validation` | Verify distinct slugs OK; duplicate slugs detected; empty allowed | Conflicts detected; empty valid | Matched | ✅ Pass |

### 20. RSS Feed (`rss.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 85 | `rss_feed_generation` | Set site settings; create published post; generate feed | Valid RSS 2.0 XML with post data | XML contains all expected elements | ✅ Pass |
| 86 | `rss_feed_empty` | Generate feed with no posts | Valid XML with channel but no items | No `<item>` tags | ✅ Pass |

### 21. License Text (`license.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 87 | `license_txt_generation` | Set display name + template; generate license | Contains all fields and template body | All fields present | ✅ Pass |

### 22. SEO: Meta Tags (`seo/meta.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 88 | `seo_build_meta_basic` | Build meta with title + description | Contains `<title>`, description, canonical | All present | ✅ Pass |
| 89 | `seo_build_meta_og_twitter` | Enable OG + Twitter; build meta | Contains og:title, og:description, twitter:card | All present | ✅ Pass |
| 90 | `seo_build_meta_no_og_twitter` | Disable OG + Twitter; build meta | No og:title or twitter:card | Absent | ✅ Pass |

### 23. SEO: Sitemap + Robots (`seo/sitemap.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 91 | `seo_sitemap_disabled` | Disable sitemap; generate | Returns `None` | `None` | ✅ Pass |
| 92 | `seo_sitemap_enabled` | Enable sitemap; create post + portfolio; generate | XML with homepage, blog, portfolio URLs | All URLs present | ✅ Pass |
| 93 | `seo_robots_txt` | Generate robots.txt without/with sitemap | Without: no Sitemap line; with: Sitemap URL present | Matched | ✅ Pass |

### 24. SEO: JSON-LD (`seo/jsonld.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 94 | `seo_jsonld_post` | Create post; generate JSON-LD | Contains BlogPosting, title, description, site URL | All present | ✅ Pass |
| 95 | `seo_jsonld_portfolio` | Create portfolio item; generate JSON-LD | Contains ImageObject, title, gallery slug | All present | ✅ Pass |

### 25. Database (`db.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 96 | `migrations_idempotent` | Run migrations 3 times | No errors on repeated runs | All succeed | ✅ Pass |
| 97 | `all_tables_exist` | Query COUNT(*) on all 22 tables | All queries succeed | All tables queryable | ✅ Pass |

### 26. Render: Category Filters (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 98 | `render_sidebar_under_link_has_toggle_open` | Sidebar + under_link categories | Toggle with "open" class | Present | ✅ Pass |
| 99 | `render_sidebar_page_top_has_horizontal_cats` | Sidebar + page_top categories | Horizontal category row at page top | Present | ✅ Pass |
| 100 | `render_sidebar_page_top_right_alignment` | page_top + right alignment | `text-align:right` on category row | Present | ✅ Pass |
| 101 | `render_topbar_submenu_no_duplicate_portfolio` | Topbar + submenu categories | No duplicate portfolio nav-link | No duplicate | ✅ Pass |
| 102 | `render_topbar_below_menu_has_category_row` | Topbar + below_menu categories | Category row below topbar | Present | ✅ Pass |
| 103 | `render_hidden_categories_no_output` | Categories set to hidden | No category HTML in output | Absent | ✅ Pass |

### 27. Render: Social Links (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 104 | `render_social_links_in_sidebar` | Social icons in sidebar position | Social links in sidebar HTML | Present | ✅ Pass |
| 105 | `render_social_brand_colors` | Brand colors enabled | `brand-color` class on social links | Present | ✅ Pass |
| 106 | `render_social_empty_when_no_urls` | No social URLs configured | No social links HTML | Absent | ✅ Pass |

### 28. Render: Share Buttons (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 107 | `render_share_label_prepended` | Share label configured | Label text before share buttons | Present | ✅ Pass |
| 108 | `render_share_buttons_rendered` | Share buttons enabled | Facebook/X/LinkedIn share links | Present | ✅ Pass |
| 109 | `render_share_disabled_no_output` | Share buttons disabled | No share HTML | Absent | ✅ Pass |

### 29. Render: Footer (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 110 | `render_footer_copyright_center` | Copyright center-aligned | Copyright in footer-center cell | Present | ✅ Pass |
| 111 | `render_footer_copyright_right` | Copyright right-aligned | Copyright in footer-right cell | Present | ✅ Pass |
| 112 | `render_footer_3_column_grid` | Copyright left + social right | 3-column footer grid | Present | ✅ Pass |

### 30. Render: Layout (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 113 | `render_sidebar_layout_has_site_wrapper` | Sidebar layout | `site-wrapper` class present | Present | ✅ Pass |
| 114 | `render_topbar_layout_has_topbar_shell` | Topbar layout | `topbar-layout` class present | Present | ✅ Pass |
| 115 | `render_topbar_nav_right_class` | Topbar + nav right | `topbar-nav-right` class | Present | ✅ Pass |
| 116 | `render_topbar_boxed_mode` | Topbar + boxed boundary | `layout-boxed` class | Present | ✅ Pass |
| 117 | `render_topbar_hides_custom_sidebar` | Topbar layout | No custom sidebar HTML | Absent | ✅ Pass |
| 118 | `render_sidebar_shows_custom_sidebar` | Sidebar + custom HTML | Custom sidebar content rendered | Present | ✅ Pass |
| 119 | `render_sidebar_right_class` | Sidebar right position | `sidebar-right` class | Present | ✅ Pass |

### 31. Render: Portfolio Item Display (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 120 | `render_portfolio_cats_false_no_visible_labels` | show_categories=false | No category labels in grid items | Absent | ✅ Pass |
| 121 | `render_portfolio_tags_false_no_visible_labels` | show_tags=false | No tag labels in grid items | Absent | ✅ Pass |
| 122 | `render_portfolio_cats_hover` | show_categories=hover | `item-meta-hover` class on categories | Present | ✅ Pass |
| 123 | `render_portfolio_tags_hover` | show_tags=hover | `item-meta-hover` class on tags | Present | ✅ Pass |
| 124 | `render_portfolio_cats_bottom_left` | show_categories=bottom_left | `item-meta-bottom_left` class | Present | ✅ Pass |
| 125 | `render_portfolio_cats_bottom_right` | show_categories=bottom_right | `item-meta-bottom_right` class | Present | ✅ Pass |
| 126 | `render_portfolio_cats_below_left` | show_categories=below_left | `item-meta-below_left` class | Present | ✅ Pass |
| 127 | `render_portfolio_cats_below_right` | show_categories=below_right | `item-meta-below_right` class | Present | ✅ Pass |
| 128 | `render_portfolio_tags_below_left` | show_tags=below_left | `item-meta-below_left` class on tags | Present | ✅ Pass |
| 129 | `render_portfolio_tags_below_right` | show_tags=below_right | `item-meta-below_right` class on tags | Present | ✅ Pass |
| 130 | `render_portfolio_legacy_true_normalizes_to_below_left` | show_categories=true (legacy) | Normalizes to `below_left` | Present | ✅ Pass |
| 131 | `render_portfolio_overlay_outside_link_tag` | Overlay categories | Overlay div outside `<a>` tag | Correct DOM order | ✅ Pass |
| 132 | `render_portfolio_mixed_cats_hover_tags_below` | cats=hover + tags=below_left | Both modes applied independently | Both present | ✅ Pass |

### 32. Render: Journal (blog_list) Settings (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 133 | `render_blog_list_grid_display` | blog_display_type=grid | `blog-grid` class in output | Present | ✅ Pass |
| 134 | `render_blog_list_masonry_display` | blog_display_type=masonry | `blog-masonry` class in output | Present | ✅ Pass |
| 135 | `render_blog_list_default_list_display` | blog_display_type=list | `blog-list` class, no grid/masonry | Correct | ✅ Pass |
| 136 | `render_blog_list_editorial_style` | list + blog_list_style=editorial | `blog-editorial` class | Present | ✅ Pass |
| 137 | `render_blog_list_show_author` | blog_show_author=true | `blog-author` + author name in output | Present | ✅ Pass |
| 138 | `render_blog_list_hide_author` | blog_show_author=false | No `blog-author` in output | Absent | ✅ Pass |
| 139 | `render_blog_list_show_date` | blog_show_date=true | `<time>` element in output | Present | ✅ Pass |
| 140 | `render_blog_list_hide_date` | blog_show_date=false | No `<time>` element | Absent | ✅ Pass |
| 141 | `render_blog_list_show_reading_time` | blog_show_reading_time=true | "min read" in output | Present | ✅ Pass |
| 142 | `render_blog_list_hide_reading_time` | blog_show_reading_time=false | No "min read" | Absent | ✅ Pass |
| 143 | `render_blog_list_excerpt_truncation` | blog_excerpt_words=5 | Excerpt truncated to 5 words | Truncated | ✅ Pass |
| 144 | `render_blog_list_pagination_load_more` | blog_pagination_type=load_more; 3 pages | `load-more-btn` element | Present | ✅ Pass |
| 145 | `render_blog_list_pagination_infinite` | blog_pagination_type=infinite; 3 pages | `infinite-sentinel` element | Present | ✅ Pass |
| 146 | `render_blog_list_pagination_classic` | blog_pagination_type=classic; 3 pages | Page number links, no sentinel | Correct | ✅ Pass |

### 33. Render: Portfolio Lightbox & Feature Settings (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 147 | `render_portfolio_lightbox_data_attrs` | All 4 lightbox toggles=false | All `data-lb-*="false"` attrs | All false | ✅ Pass |
| 148 | `render_portfolio_lightbox_defaults_true` | No lightbox settings (defaults) | All `data-lb-*="true"` attrs | All true | ✅ Pass |
| 149 | `render_portfolio_image_protection_enabled` | portfolio_image_protection=true | `contextmenu` JS injected | Present | ✅ Pass |
| 150 | `render_portfolio_image_protection_disabled` | portfolio_image_protection=false | No `contextmenu` JS | Absent | ✅ Pass |
| 151 | `render_portfolio_likes_data_attr` | portfolio_enable_likes=true | `data-show-likes="true"` | Present | ✅ Pass |
| 152 | `render_portfolio_likes_disabled` | portfolio_enable_likes=false | `data-show-likes="false"` | Present | ✅ Pass |
| 153 | `render_portfolio_pagination_data_attr` | portfolio_pagination_type=load_more | `data-pagination-type="load_more"` | Present | ✅ Pass |
| 154 | `render_portfolio_click_mode_data_attr` | portfolio_click_mode=detail | `data-click-mode="detail"` | Present | ✅ Pass |

---

## Running Tests

```bash
# Run all tests
cargo test

# Run a specific test
cargo test settings_set_and_get

# Run tests with output
cargo test -- --nocapture

# Run tests matching a pattern
cargo test seo_

# Run tests in a specific section
cargo test fw_ban
```

## Test Architecture

```
src/tests.rs
├── test_pool()          — shared-cache in-memory SQLite with migrations + seed
├── fast_hash()          — bcrypt cost=4 for test speed
├── make_post_form()     — PostForm builder helper
├── make_portfolio_form()— PortfolioForm builder helper
├── make_cat_form()      — CategoryForm builder helper
├── setup_portfolio()    — quick portfolio insert for order tests
│
├── Settings (8 tests)
├── Posts (4 tests)
├── Portfolio (7 tests)
├── Categories (3 tests)
├── Tags (3 tests)
├── Comments (3 tests)
├── Users (11 tests)
├── Orders + Tokens + Licenses (7 tests)
├── Audit Log (2 tests)
├── Firewall (7 tests)
├── Designs + Templates (4 tests)
├── Analytics / PageView (7 tests)
├── Imports (1 test)
├── MFA / TOTP (4 tests)
├── Password Hashing (2 tests)
├── Sessions (3 tests)
├── IP Hashing (1 test)
├── Rate Limiting (4 tests)
├── Slug Validation (3 tests)
├── RSS Feed (2 tests)
├── License Text (1 test)
├── SEO Meta (3 tests)
├── SEO Sitemap + Robots (3 tests)
├── SEO JSON-LD (2 tests)
├── DB Migrations (2 tests)
├── Render: Category Filters (6 tests)
├── Render: Social Links (3 tests)
├── Render: Share Buttons (3 tests)
├── Render: Footer (3 tests)
├── Render: Layout (7 tests)
├── Render: Portfolio Item Display (13 tests)
├── Render: Journal Settings (14 tests)
└── Render: Portfolio Lightbox & Features (8 tests)
```
