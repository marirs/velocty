# Velocty — Test Coverage Report

> **Last run:** 2026-02-20 | **Result:** 308 passed, 0 failed | **Duration:** ~2.8s  
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
| `models/passkey.rs` | 7 | 7 | **100%** |
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
| `image_proxy.rs` | 4 | 4 | **100%** |
| `svg_sanitizer.rs` | 1 | 1 | **100%** |
| `typography/mod.rs` | 2 | 2 | **100%** |
| `security/passkey.rs` | 4 | 2 | **50%** |
| **TOTAL** | **200** | **192** | **96%** |

### Not unit-testable (excluded from coverage)

| Module | Reason |
|--------|--------|
| `security/auth.rs` — request guards (`AuthenticatedUser`, `AdminUser`, `EditorUser`, `AuthorUser`) | Require Rocket `FromRequest` context |
| `security/mfa.rs` — cookie functions (`set_pending_cookie`, `take_pending_cookie`, `get_pending_cookie`) | Require Rocket `CookieJar` |
| `security/passkey.rs` — `build_webauthn`, `load_credentials` | Require valid site_url + real WebAuthn credentials |
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

### 34. Render: Commerce Settings (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 155 | `render_commerce_button_custom_color` | commerce_button_color=#FF0000 + Stripe | `background:#FF0000` overrides Stripe default | Present | ✅ Pass |
| 156 | `render_commerce_button_default_stripe_color` | No custom color + Stripe | `background:#635BFF` (Stripe default) | Present | ✅ Pass |
| 157 | `render_commerce_button_custom_label` | commerce_button_label="Purchase Now" | Custom label shown; default absent | Correct | ✅ Pass |
| 158 | `render_commerce_button_radius_pill` | commerce_button_radius=pill | `border-radius:999px` | Present | ✅ Pass |
| 159 | `render_commerce_button_radius_square` | commerce_button_radius=square | `border-radius:0` | Present | ✅ Pass |
| 160 | `render_commerce_button_alignment_center` | commerce_button_alignment=center | `text-align:center` + `display:inline-block` | Both present | ✅ Pass |
| 161 | `render_commerce_paypal_sdk_style` | PayPal color=blue, shape=pill, label=buynow | All 3 values in `paypal.Buttons({style:...})` | Present | ✅ Pass |
| 162 | `render_commerce_position_below_image` | commerce_button_position=below_image | Commerce section between image and meta | Correct order | ✅ Pass |
| 163 | `render_commerce_position_below_description` | commerce_button_position=below_description | Commerce section after description | Correct order | ✅ Pass |
| 164 | `render_commerce_position_sidebar_right` | commerce_button_position=sidebar_right | Flex row layout with sidebar column | Both classes present | ✅ Pass |
| 165 | `render_commerce_price_badge_top_right` | commerce_show_price=true, position=top_right | `price-badge` with `top:8px;right:8px` + "USD 19.99" | Present | ✅ Pass |
| 166 | `render_commerce_price_badge_top_left` | commerce_price_position=top_left | `top:8px;left:8px` | Present | ✅ Pass |
| 167 | `render_commerce_price_badge_below_title` | commerce_price_position=below_title | `price-badge-below` class | Present | ✅ Pass |
| 168 | `render_commerce_price_badge_hidden` | commerce_show_price=false | No `price-badge` in output | Absent | ✅ Pass |
| 169 | `render_commerce_lightbox_buy_data_attrs` | lightbox_buy=true, position=sidebar, currency=EUR | All 3 data attributes present | Present | ✅ Pass |
| 170 | `render_commerce_lightbox_buy_disabled` | commerce_lightbox_buy=false | `data-lb-buy="false"` | Present | ✅ Pass |
| 171 | `render_commerce_grid_item_data_attrs` | Grid item with price=19.99, sell_enabled=true | `data-price` and `data-sell` attributes | Present | ✅ Pass |

### 35. License Default & Generation (`db.rs`, `routes/commerce/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 172 | `license_default_text_seeded` | Check `downloads_license_template` after seed_defaults | Contains all sections: GRANT, PERMITTED USES, RESTRICTIONS, ATTRIBUTION, WARRANTY, TERMINATION, Licensor/Licensee | All present | ✅ Pass |
| 173 | `license_default_not_personal_only` | Verify license allows commercial use | No "personal only" language; contains "Commercial use in a single end product" | Correct | ✅ Pass |
| 174 | `license_paypal_legacy_key_empty` | Check `paypal_license_text` is deprecated/empty | Empty string | Empty | ✅ Pass |
| 175 | `license_text_generation_header` | Build license .txt with header fields | Contains: License for, Purchased from, Transaction, Date, License Key, separator, body | All present | ✅ Pass |
| 176 | `license_text_generation_no_provider_order_id` | Empty provider_order_id falls back to ORD-{id} | `"ORD-42"` | `"ORD-42"` | ✅ Pass |

### 36. Image Proxy (`image_proxy.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 177 | `image_proxy_encode_decode_roundtrip` | Encode path → decode with same secret | Original path returned | Matched | ✅ Pass |
| 178 | `image_proxy_tampered_token_rejected` | Tamper with token bytes | `None` returned | `None` | ✅ Pass |
| 179 | `image_proxy_wrong_secret_rejected` | Decode with different secret | `None` returned | `None` | ✅ Pass |
| 180 | `image_proxy_rewrite_upload_urls` | Rewrite `/uploads/` URLs in HTML | All replaced with `/img/<token>` | Matched | ✅ Pass |
| 181 | `image_proxy_preserves_non_upload_urls` | Non-upload URLs unchanged | No rewriting | Matched | ✅ Pass |
| 182 | `image_proxy_mime_detection` | Detect MIME from file extension | Correct MIME types | Matched | ✅ Pass |
| 183 | `image_proxy_render_rewrites_urls` | Render pipeline rewrites upload URLs | `/img/` tokens in output | Present | ✅ Pass |
| 184 | `image_proxy_seed_generates_secret` | seed_defaults creates image_proxy_secret | Non-empty 64-char hex | Valid | ✅ Pass |
| 185 | `image_proxy_dual_key_fallback` | Decode with old secret during rotation | Path returned via fallback | Matched | ✅ Pass |
| 186 | `image_proxy_dual_key_expired_old_rejected` | Decode with expired old secret | `None` returned | `None` | ✅ Pass |
| 187 | `image_proxy_dual_key_no_old_secret` | Decode with empty old secret | Only current key works | Matched | ✅ Pass |

### 37. Seed Defaults (`db.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 188 | `seed_defaults_critical_settings_exist` | Verify critical settings seeded | All required keys present | Present | ✅ Pass |
| 189 | `seed_defaults_no_duplicate_keys` | Check no duplicate keys in defaults | All keys unique | Unique | ✅ Pass |
| 190 | `seed_defaults_setting_groups_present` | Verify setting groups (smtp_, security_, etc.) | All groups have entries | Present | ✅ Pass |
| 191 | `seed_defaults_privacy_policy_content_not_empty` | Privacy policy template seeded | Non-empty content | Non-empty | ✅ Pass |
| 192 | `seed_defaults_terms_of_use_content_not_empty` | Terms of use template seeded | Non-empty content | Non-empty | ✅ Pass |
| 193 | `seed_defaults_legal_content_backfill_migration` | Backfill migration for legal content | Content populated | Present | ✅ Pass |

### 38. Resolve Status (`models/post.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 194 | `resolve_status_published_past_date_stays_published` | Published + past date | Stays published | `"published"` | ✅ Pass |
| 195 | `resolve_status_published_future_date_becomes_scheduled` | Published + future date | Becomes scheduled | `"scheduled"` | ✅ Pass |
| 196 | `resolve_status_draft_future_date_stays_draft` | Draft + future date | Stays draft | `"draft"` | ✅ Pass |
| 197 | `resolve_status_published_empty_date_stays_published` | Published + empty date | Stays published | `"published"` | ✅ Pass |
| 198 | `resolve_status_published_invalid_date_stays_published` | Published + invalid date | Stays published | `"published"` | ✅ Pass |
| 199 | `resolve_status_published_no_date_stays_published` | Published + no date | Stays published | `"published"` | ✅ Pass |
| 200 | `resolve_status_scheduled_past_date_stays_scheduled` | Scheduled + past date | Stays scheduled | `"scheduled"` | ✅ Pass |
| 201 | `resolve_status_published_near_future_becomes_scheduled` | Published + near future | Becomes scheduled | `"scheduled"` | ✅ Pass |
| 202 | `resolve_status_date_with_seconds_format_stays_published` | Date with seconds format | Stays published | `"published"` | ✅ Pass |
| 203 | `resolve_status_utc_future_1h_becomes_scheduled` | UTC future +1h | Becomes scheduled | `"scheduled"` | ✅ Pass |
| 204 | `resolve_status_utc_future_1min_becomes_scheduled` | UTC future +1min | Becomes scheduled | `"scheduled"` | ✅ Pass |
| 205 | `resolve_status_utc_past_1h_stays_published` | UTC past -1h | Stays published | `"published"` | ✅ Pass |
| 206 | `resolve_status_utc_past_1min_stays_published` | UTC past -1min | Stays published | `"published"` | ✅ Pass |
| 207 | `resolve_status_draft_with_utc_future_stays_draft` | Draft + UTC future | Stays draft | `"draft"` | ✅ Pass |
| 208 | `resolve_status_scheduled_with_utc_past_stays_scheduled` | Scheduled + UTC past | Stays scheduled | `"scheduled"` | ✅ Pass |
| 209 | `resolve_status_handles_timezone_offset_string_gracefully` | Timezone offset string | Handled gracefully | No panic | ✅ Pass |
| 210 | `published_at_default_fallback_is_utc` | Default published_at | UTC format | Valid UTC | ✅ Pass |
| 211 | `utc_format_roundtrip_parseable` | UTC format roundtrip | Parseable string | Valid | ✅ Pass |

### 39. Settings Save (`models/settings.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 212 | `settings_save_portfolio_enable_persists` | Enable portfolio via save | `"true"` persisted | `"true"` | ✅ Pass |
| 213 | `settings_save_portfolio_disable_persists` | Disable portfolio via save | `"false"` persisted | `"false"` | ✅ Pass |
| 214 | `settings_save_journal_disable_persists` | Disable journal via save | `"false"` persisted | `"false"` | ✅ Pass |

### 40. Tag Helpers (`models/tag.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 215 | `tag_names_to_content_roundtrip_post` | Assign tags by name to post; read back | Tags match | Matched | ✅ Pass |
| 216 | `tag_names_to_content_roundtrip_portfolio` | Assign tags by name to portfolio; read back | Tags match | Matched | ✅ Pass |
| 217 | `tag_names_empty_clears_all` | Assign empty tag list | All tags removed | 0 tags | ✅ Pass |

### 41. Render: Footer Modes (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 218 | `render_footer_regular_no_class` | Footer mode=regular | No special class | Absent | ✅ Pass |
| 219 | `render_footer_always_visible_site_wide` | Footer mode=always_visible, scope=site_wide | `footer-always-visible` class | Present | ✅ Pass |
| 220 | `render_footer_fixed_reveal_site_wide` | Footer mode=fixed_reveal, scope=site_wide | `footer-fixed-reveal` class | Present | ✅ Pass |
| 221 | `render_footer_selected_pages_match` | Footer mode on selected page types | Class applied on matching page | Present | ✅ Pass |
| 222 | `render_footer_selected_pages_no_match` | Footer mode on non-matching page type | No class applied | Absent | ✅ Pass |

### 42. Render: Journal Navigation (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 223 | `render_journal_sidebar_under_link_has_toggle` | Journal under_link mode | Toggle with "open" class | Present | ✅ Pass |
| 224 | `render_journal_sidebar_under_link_custom_all_label` | Custom "All" label | Custom label rendered | Present | ✅ Pass |
| 225 | `render_journal_sidebar_under_link_all_hidden` | journal_show_all_categories=false | No "All" link | Absent | ✅ Pass |
| 226 | `render_journal_page_top_has_filter_bar` | Journal page_top mode | Horizontal filter bar | Present | ✅ Pass |
| 227 | `render_journal_page_top_align_right` | page_top + right alignment | `cats-right` class | Present | ✅ Pass |
| 228 | `render_journal_hidden_shows_plain_link` | Journal categories hidden | Plain nav-link for journal | Present | ✅ Pass |

### 43. Render: Portfolio Lightbox Defaults (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 229 | `render_portfolio_lightbox_defaults_center` | Default lightbox center alignment | `data-lb-center` attribute | Present | ✅ Pass |

### 44. Render: Upload Path Handling (`render.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 230 | `uploaded_path_with_value_is_used` | Portfolio item with uploaded_path | Path used for image | Present | ✅ Pass |
| 231 | `uploaded_path_empty_string_is_not_used` | Empty uploaded_path | Falls back to image_path | Correct | ✅ Pass |
| 232 | `uploaded_path_none_is_not_used` | None uploaded_path | Falls back to image_path | Correct | ✅ Pass |

### 45. SVG Sanitizer (`svg_sanitizer.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 233 | `test_clean_svg_passes_through` | Clean SVG with circle element | Preserved as-is | `<circle` present | ✅ Pass |
| 234 | `test_strips_script_element` | SVG with `<script>alert('xss')</script>` | Script removed, circle kept | No `<script>`, circle present | ✅ Pass |
| 235 | `test_strips_event_handlers` | Circle with `onload` and `onclick` attrs | Event handlers removed | No `onload`/`onclick` | ✅ Pass |
| 236 | `test_strips_javascript_href` | `<a href="javascript:alert(1)">` | href removed | No `javascript:` | ✅ Pass |
| 237 | `test_strips_foreignobject` | `<foreignObject>` with nested script | Entire element removed | No `foreignObject` | ✅ Pass |
| 238 | `test_strips_external_use` | `<use href="https://evil.com/...">` | External use removed | No `evil.com` | ✅ Pass |
| 239 | `test_allows_internal_use` | `<use href="#c">` (internal ref) | Preserved | `use` and `#c` present | ✅ Pass |
| 240 | `test_strips_data_uri_href` | `href="data:text/html,..."` | href removed | No `data:text/html` | ✅ Pass |
| 241 | `test_strips_style_expression` | `style="width:expression(alert(1))"` | Style attr removed | No `expression(` | ✅ Pass |
| 242 | `test_strips_comments` | IE conditional comment with script | Comment stripped | No `<!--` or `alert` | ✅ Pass |
| 243 | `test_strips_iframe_element` | `<iframe src="...">` inside SVG | Element removed | No `iframe` | ✅ Pass |
| 244 | `test_strips_embed_element` | `<embed src="...">` inside SVG | Element removed | No `embed` | ✅ Pass |

### 46. Passkey: DB Migration & Model (`models/passkey.rs`, `models/user.rs`, `security/passkey.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 245 | `passkey_table_exists` | Query user_passkeys table | Table queryable | Count=0 | ✅ Pass |
| 246 | `passkey_model_crud` | Create → list → get_by_credential_id → update_sign_count → delete | Full lifecycle | All assertions pass | ✅ Pass |
| 247 | `passkey_count_for_user` | Create 2 passkeys; count | count=2 | `2` | ✅ Pass |
| 248 | `passkey_delete_all_for_user` | Create 2 passkeys; delete_all; count | count=0 | `0` | ✅ Pass |
| 249 | `passkey_unique_credential_id` | Create 2 passkeys with same credential_id | Second returns Err | `Err` | ✅ Pass |
| 250 | `passkey_user_auth_method_fields` | Create user; check defaults; update auth_method | Default="password"; update works | Matched | ✅ Pass |
| 251 | `passkey_auto_enable_on_first_registration` | Set auth_method to passkey on first passkey | auth_method="passkey", fallback="password" | Matched | ✅ Pass |
| 252 | `passkey_auto_revert_on_last_deletion` | Delete last passkey; check revert | auth_method reverts to fallback | Matched | ✅ Pass |
| 253 | `passkey_no_revert_when_passkeys_remain` | Delete 1 of 2 passkeys | auth_method stays "passkey" | `"passkey"` | ✅ Pass |
| 254 | `passkey_safe_json_includes_auth_fields` | Call safe_json on user with passkey | auth_method and auth_method_fallback present | Present | ✅ Pass |
| 255 | `passkey_multiple_users_independent` | Create passkeys for 2 users | Each user's count independent | Correct counts | ✅ Pass |
| 256 | `passkey_from_row_defaults` | Create user without explicit auth_method | Defaults to "password" | `"password"` | ✅ Pass |
| 257 | `passkey_update_auth_method_roundtrip` | Set to passkey then back to magic_link | Both updates persist | Matched | ✅ Pass |
| 258 | `passkey_list_empty_for_new_user` | List passkeys for user with none | Empty vec | `len()=0` | ✅ Pass |
| 259 | `passkey_delete_wrong_user_fails` | Delete passkey with wrong user_id | Returns Err | `Err` | ✅ Pass |
| 260 | `passkey_store_and_take_reg_state` | Store reg state JSON; take it; verify cleared | JSON stored and cleared | Matched | ✅ Pass |
| 261 | `passkey_migration_idempotent` | Run migrations 3 times | No errors | All succeed | ✅ Pass |

### 47. Typography: CSS Variables — Colors (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 262 | `css_vars_default_colors` | Build CSS vars with empty settings | All 16 color defaults correct | Matched | ✅ Pass |
| 263 | `css_vars_custom_colors` | Build CSS vars with all 16 custom colors | All custom values in output | Matched | ✅ Pass |

### 48. Typography: CSS Variables — Fonts (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 264 | `css_vars_default_fonts_sitewide` | Default sitewide fonts | All families = Inter | Matched | ✅ Pass |
| 265 | `css_vars_custom_fonts_sitewide` | Custom primary/heading with sitewide=true | Body/nav/buttons inherit primary | Matched | ✅ Pass |
| 266 | `css_vars_per_element_fonts_no_sitewide` | Per-element fonts with sitewide=false | Each element uses its own font | Matched | ✅ Pass |
| 267 | `css_vars_per_element_fonts_fallback_to_primary` | sitewide=false, no per-element set | Falls back to primary/heading | Matched | ✅ Pass |
| 268 | `css_vars_independent_element_fonts` | logo, subheading, blockquote, list, footer, etc. | All 8 independent fonts correct | Matched | ✅ Pass |
| 269 | `css_vars_default_font_sizes` | Default sizes for body, h1-h6, logo, nav, footer | All 11 defaults correct | Matched | ✅ Pass |
| 270 | `css_vars_custom_font_sizes` | Custom sizes for body, h1, h2, logo, nav, line-height | All custom values in output | Matched | ✅ Pass |

### 49. Typography: CSS Variables — Text & Layout (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 271 | `css_vars_text_transform_direction_alignment` | Custom uppercase, rtl, center | All 3 values correct | Matched | ✅ Pass |
| 272 | `css_vars_text_defaults` | Default text-transform, direction, alignment | none, ltr, left | Matched | ✅ Pass |
| 273 | `css_vars_layout_sidebar_left` | Sidebar position=left | `--sidebar-direction: row` | Matched | ✅ Pass |
| 274 | `css_vars_layout_sidebar_right` | Sidebar position=right | `--sidebar-direction: row-reverse` | Matched | ✅ Pass |
| 275 | `css_vars_layout_margins` | Custom margins 20/30/10/15 | All 4 margins with px suffix | Matched | ✅ Pass |
| 276 | `css_vars_layout_margins_zero` | Default zero margins | All 4 = 0 (no px) | Matched | ✅ Pass |
| 277 | `css_vars_layout_margins_with_px_suffix` | Margin "20px" input | No double px suffix | `20px` not `20pxpx` | ✅ Pass |
| 278 | `css_vars_content_boundary_boxed` | Boundary=boxed | `--content-max-width: 1200px` | Matched | ✅ Pass |
| 279 | `css_vars_content_boundary_full` | Boundary=full | `--content-max-width: none` | Matched | ✅ Pass |

### 50. Typography: CSS Variables — Grid & Lightbox (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 280 | `css_vars_grid_columns` | Portfolio=4, blog=2 columns | Both grid vars correct | Matched | ✅ Pass |
| 281 | `css_vars_lightbox_colors` | Custom border/title/tag/nav colors | All 4 lightbox color vars correct | Matched | ✅ Pass |
| 282 | `css_vars_wraps_in_root_selector` | Build CSS vars | Starts with `:root {`, ends with `}` | Matched | ✅ Pass |

### 51. Typography: Font Links (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 283 | `font_links_empty_when_no_providers` | No font providers enabled | Empty string | `""` | ✅ Pass |
| 284 | `font_links_google_basic` | Google enabled + Roboto + Inter | Preconnect + both families in URL | Present | ✅ Pass |
| 285 | `font_links_google_deduplicates` | Same font for primary + heading | Family appears only once | `count=1` | ✅ Pass |
| 286 | `font_links_google_per_element_fonts_not_sitewide` | sitewide=false + per-element fonts | All unique families loaded | Present | ✅ Pass |
| 287 | `font_links_google_independent_element_fonts` | logo=Pacifico, footer=Nunito | Both loaded via Google Fonts | Present | ✅ Pass |
| 288 | `font_links_google_skips_system_fonts` | system-ui + Georgia | No Google Fonts link generated | Absent | ✅ Pass |
| 289 | `font_links_google_skips_adobe_prefixed` | adobe-caslon-pro | Not sent to Google Fonts | Absent | ✅ Pass |
| 290 | `font_links_adobe` | Adobe enabled + project ID | Typekit CSS link | Present | ✅ Pass |
| 291 | `font_links_adobe_empty_project_id` | Adobe enabled + empty ID | No Typekit link | Absent | ✅ Pass |
| 292 | `font_links_custom_font_face_woff2` | Custom font .woff2 | @font-face with woff2 format | Present | ✅ Pass |
| 293 | `font_links_custom_font_face_ttf` | Custom font .ttf | format('truetype') | Present | ✅ Pass |
| 294 | `font_links_custom_font_face_otf` | Custom font .otf | format('opentype') | Present | ✅ Pass |
| 295 | `font_links_custom_font_missing_name_no_output` | Empty font name | No @font-face | Absent | ✅ Pass |
| 296 | `font_links_custom_font_missing_file_no_output` | Empty filename | No @font-face | Absent | ✅ Pass |
| 297 | `font_links_google_and_adobe_and_custom_combined` | All 3 providers | Google + Adobe + @font-face all present | Present | ✅ Pass |
| 298 | `font_links_google_spaces_replaced_with_plus` | "Open Sans" | `family=Open+Sans` in URL | Correct encoding | ✅ Pass |

### 52. Typography: Render Integration (`typography/mod.rs`)

| # | Test | What it does | Expected | Got | Result |
|---|------|-------------|----------|-----|--------|
| 299 | `render_css_vars_in_page_output` | Set colors + font in DB; build CSS vars from Setting::all | Custom values in CSS output | Matched | ✅ Pass |

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
├── Render: Portfolio Lightbox & Features (8 tests)
├── Render: Commerce Settings (17 tests)
├── License Default & Generation (5 tests)
├── Image Proxy (11 tests)
├── Seed Defaults (6 tests)
├── Resolve Status (18 tests)
├── Settings Save (3 tests)
├── Tag Helpers (3 tests)
├── Render: Footer Modes (5 tests)
├── Render: Journal Navigation (6 tests)
├── Render: Portfolio Lightbox Defaults (1 test)
├── Render: Upload Path Handling (3 tests)
├── SVG Sanitizer (12 tests)
├── Passkey: DB & Model (17 tests)
├── Typography: CSS Variables — Colors (2 tests)
├── Typography: CSS Variables — Fonts (6 tests)
├── Typography: CSS Variables — Layout (9 tests)
├── Typography: CSS Variables — Lightbox & Grid (3 tests)
├── Typography: Font Links (14 tests)
└── Typography: Render Integration (1 test)
