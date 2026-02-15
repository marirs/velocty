# Velocty — Refactoring & Implementation Progress

Last updated: 2026-02-15

---

## Completed

### AI Module Refactor ✅
- Refactored monolithic `src/ai.rs` into `src/ai/` module directory
- Submodules: `mod.rs` (dispatch + types), `prompts.rs`, `ollama.rs`, `openai.rs`, `gemini.rs`, `groq.rs`, `cloudflare.rs`
- Refactored monolithic `src/routes/ai.rs` into `src/routes/ai/` with `mod.rs`, `suggest.rs`, `generate.rs`, `status.rs`
- Zero behavior changes — same API, same failover chain

### Email Module Refactor + Implementation ✅
- Refactored monolithic `src/email.rs` into `src/email/` module directory
- Implemented all 11 email providers:
  - **Gmail** (SMTP), **Custom SMTP** — already existed, moved into submodules
  - **Resend** — REST API
  - **Amazon SES** — SigV4 signed requests (custom `aws_urlencode`, no external crate)
  - **Postmark** — REST API
  - **Brevo** (Sendinblue) — REST API
  - **SendPulse** — OAuth2 token exchange + REST API
  - **Mailgun** — REST API with EU/US region support
  - **Moosend** — REST API
  - **Mandrill** (Mailchimp Transactional) — REST API with rejection detection
  - **SparkPost** — REST API with EU/US region support
- Failover chain dispatch in `mod.rs` with `email_failover_enabled` + `email_failover_chain` settings

### Security Module — Captcha Providers ✅
- Created `src/security/` module with 3 captcha providers:
  - **reCAPTCHA** (`recaptcha.rs`) — v2 checkbox + v3 invisible, score threshold
  - **Cloudflare Turnstile** (`turnstile.rs`) — siteverify API
  - **hCaptcha** (`hcaptcha.rs`) — siteverify API
- Dispatch in `security/mod.rs`: `verify_captcha()`, `verify_login_captcha()`, `login_captcha_info()`, `active_captcha()`

### Security Module — Spam Detection Providers ✅
- 3 spam detection providers:
  - **Akismet** (`akismet.rs`) — comment-check API
  - **CleanTalk** (`cleantalk.rs`) — check_message API
  - **OOPSpam** (`oopspam.rs`) — v1 spamdetection API with score threshold
- Dispatch in `security/mod.rs`: `check_spam()`, `has_spam_provider()`

### Auth Refactor (auth.rs → security/auth.rs + security/mfa.rs) ✅
- Moved session CRUD, password hash/verify, IP hashing, `AdminUser` guard, rate limit check → `security/auth.rs`
- Moved TOTP secret gen, QR code, verify code, recovery codes, pending cookies → `security/mfa.rs`
- Updated all imports across codebase: `admin.rs`, `admin_api.rs`, `api.rs`, `ai/*.rs`, `site.rs`
- Removed old `src/auth.rs`

### Magic Link Authentication — New Feature ✅
- Created `security/magic_link.rs`: token generation (UUID, 15min expiry, single-use), email sending via configured provider, token verification, cleanup
- Added `magic_links` table to DB schema (`db.rs`)
- Created `magic_link.html.tera` template with success/error states
- Login page auto-redirects to magic link page when `login_method == "magic_link"`
- Magic link verify route creates session (with MFA challenge if enabled)

### Routes Refactor (routes/auth.rs → routes/security/auth/) ✅
- Split monolithic `routes/auth.rs` into:
  - `routes/security/mod.rs` — `NoCacheTemplate`, aggregated `routes()`
  - `routes/security/auth/mod.rs` — auth sub-routes
  - `routes/security/auth/login.rs` — login page + submit
  - `routes/security/auth/mfa.rs` — MFA challenge page + submit
  - `routes/security/auth/magic_link.rs` — magic link request + verify
  - `routes/security/auth/setup.rs` — first-run wizard + MongoDB test
  - `routes/security/auth/logout.rs` — logout + catch-all redirect
- Removed old `src/routes/auth.rs`

### Captcha Wired Into Login ✅
- `login_captcha_enabled` + `login_captcha_provider` settings respected
- `verify_login_captcha()` in `security/mod.rs` uses the specific provider
- `inject_captcha_context()` passes provider/site_key/version to templates
- `login.html.tera` renders captcha widget (reCAPTCHA v2/v3, Turnstile, hCaptcha) + JS token extraction
- `magic_link.html.tera` same captcha support
- `captcha_token` field added to `LoginForm` and `MagicLinkForm`

### Captcha + Spam Wired Into Comments ✅
- `captcha_token` + `ip` fields added to `CommentSubmit` in `routes/api.rs`
- Server-side captcha verification before comment creation (auto-detect provider)
- Server-side spam check via all enabled spam providers before comment creation
- Comment form in `render.rs` now has:
  - Full JS submit handler (fetch to `/api/comment`)
  - Captcha widget injection (reCAPTCHA v2/v3, Turnstile, hCaptcha)
  - Token extraction per provider

### README.md Updated ✅
- Directory structure updated to reflect new modular layout

---

## Remaining / Future Work

### Phase 3 — Design Builder
- GrapesJS integration for drag-and-drop page layout
- Design management (create, edit, duplicate, delete, activate, preview)
- Custom components for content placeholders

### Multi-Site Improvements
- Fix crash when `--features multi-site` binary runs without setup (duplicated managed state)
- Wizard-based first-run for multi-site migration/setup

### SEO Module Refactor ✅
- Refactored monolithic `src/seo.rs` into `src/seo/` module directory:
  - `seo/mod.rs` — re-exports, shared `html_escape`/`json_escape`
  - `seo/meta.rs` — `build_meta()` (title, description, canonical, OG, Twitter Cards)
  - `seo/jsonld.rs` — `build_post_jsonld()`, `build_portfolio_jsonld()` (uses dynamic blog/portfolio slugs)
  - `seo/sitemap.rs` — `generate_sitemap()` + `generate_robots()` (uses dynamic slugs)
  - `seo/analytics.rs` — `build_analytics_scripts()` (moved from render.rs) — 7 providers: GA4, Plausible, Fathom, Matomo, Cloudflare, Clicky, Umami
  - `seo/webmaster.rs` — `build_webmaster_meta()` (moved from render.rs) — Google, Bing, Yandex, Pinterest, Baidu
- **Bug fix:** JSON-LD structured data now actually injected into blog single + portfolio single pages (was defined but never called)
- **Bug fix:** `/sitemap.xml` now returns 404 when `seo_sitemap_enabled` is `false` (was served unconditionally)
- **Bug fix:** Sitemap URLs now use dynamic `blog_slug` and `portfolio_slug` settings instead of hardcoded `/blog/` and `/portfolio/`
- **Bug fix:** `robots.txt` only includes Sitemap line when sitemap is enabled

### Typography Fixes ✅
- Rewrote `build_css_variables()` in `render.rs` to emit all typography CSS variables:
  - `--font-body`, `--font-heading`, `--font-nav`, `--font-buttons`, `--font-captions` (per-element fonts)
  - `--font-size-h1` through `--font-size-h6` (configurable heading sizes)
  - `--text-transform` (none, uppercase, lowercase, capitalize)
- New `build_font_links()` function for conditional font loading:
  - **Google Fonts**: only loads when `font_google_enabled=true`, collects all unique families (primary, heading, per-element) into a single `<link>` tag
  - **Adobe Fonts**: emits `<link rel="stylesheet" href="https://use.typekit.net/{project_id}.css">` when enabled
  - **Custom fonts**: emits `@font-face` declaration pointing to `/uploads/fonts/{filename}`
- Updated `DEFAULT_CSS` to use CSS variables for:
  - `body` uses `var(--font-body)` + `var(--text-transform)`
  - `h1-h6` use `var(--font-heading)` + `var(--font-size-h1)` through `var(--font-size-h6)`
  - `.cat-link`, `.archives-link` use `var(--font-nav)`
  - `.comment-form button`, `.pagination` use `var(--font-buttons)`
  - `.footer-text`, `.item-tags` use `var(--font-captions)`
- Added H4, H5, H6 size fields to typography settings template
- **New API**: `POST /{admin_slug}/upload/font` — accepts font file (.woff2/.woff/.ttf/.otf) + font name, saves to `uploads/fonts/`, stores `font_custom_name` and `font_custom_filename` in settings
- Wired "Add Font" button in typography template to use the upload API via `fetch`

### Comments Fixes ✅
- **Bug fix:** `comments_enabled` global setting now checked in both the API (`comment_submit`) and render — form only shown when enabled
- **Bug fix:** `comments_on_blog` now checked in `blog_single` route — comments not loaded or rendered when disabled
- **Bug fix:** `comments_on_portfolio` now checked with global `comments_enabled` in `portfolio_single` route
- **Bug fix:** Portfolio single pages now render comments and comment form (was completely missing)
- **Bug fix:** `comments_require_name` / `comments_require_email` now enforced server-side in the API and dynamically set `required` attribute in the HTML form
- **Bug fix:** Missing checkbox keys (`comments_honeypot`, `comments_require_name`, `comments_require_email`) added to admin settings save handler
- **Removed dead code:** `Comment::rate_limit_check` method (queried non-existent `page_views` table; actual rate limiting uses `RateLimiter`)
- **New feature:** Threaded replies — `parent_id` column added to comments table via migration
  - Comments display nested with indentation (up to 3 levels deep)
  - "Reply" button on each comment sets `parent_id` in the form
  - Reply indicator with cancel button
  - `parent_id` sent in API payload and stored in DB
- **Refactored:** Comment rendering extracted into reusable `build_comments_section()` + `render_comment()` functions shared by blog and portfolio
- WordPress import updated with `parent_id: None` for compatibility

### Portfolio Settings Audit ✅
- **Bug fix:** 5 missing checkbox keys in `settings_save` — `portfolio_show_categories`, `portfolio_show_tags`, `portfolio_lightbox_show_title`, `portfolio_lightbox_show_tags`, `portfolio_lightbox_nav`, `portfolio_lightbox_keyboard` now properly reset to false when unchecked
- **Bug fix:** `portfolio_click_mode` now wired — sets `data-click-mode` on `<body>`, lightbox JS reads it and skips when mode is "single"
- **Bug fix:** `portfolio_display_type` now wired — grid uses `css-grid` class for "grid" mode, `masonry-grid` for "masonry" (CSS column-count)
- **Bug fix:** `portfolio_grid_columns` was already wired via CSS var `--grid-columns` ✓
- **Bug fix:** `portfolio_pagination_type` now wired — supports "classic" (numbered), "load_more" (AJAX button), "infinite" (IntersectionObserver scroll)
- **Bug fix:** `portfolio_lightbox_border_color` was already wired via CSS var ✓
- **Bug fix:** `portfolio_lightbox_title_color` now wired via CSS var `--lightbox-title-color` (was hardcoded `#fff`)
- **Bug fix:** `portfolio_lightbox_tag_color` now wired via CSS var `--lightbox-tag-color` + tags shown in lightbox
- **Bug fix:** `portfolio_lightbox_nav_color` now wired via CSS var `--lightbox-nav-color` (was hardcoded `rgba(255,255,255,0.5)`)
- **Bug fix:** `portfolio_lightbox_show_title` now wired — conditionally renders title element in lightbox overlay
- **Bug fix:** `portfolio_lightbox_show_tags` now wired — conditionally renders tags element in lightbox overlay
- **Bug fix:** `portfolio_lightbox_nav` now wired — conditionally renders prev/next buttons in lightbox
- **Bug fix:** `portfolio_lightbox_keyboard` now wired — conditionally enables arrow key / Escape navigation
- **Bug fix:** `portfolio_enable_likes` now wired — like button hidden on single page + lightbox when disabled
- **Bug fix:** `portfolio_show_categories` now wired — hides sidebar category nav + single page categories when disabled
- **Bug fix:** `portfolio_show_tags` now wired — hides tags below grid images when disabled
- **Bug fix:** `portfolio_fade_animation` now wired — adds `fade-in` CSS class + IntersectionObserver JS for scroll reveal
- **Bug fix:** `portfolio_image_protection` was already wired ✓
- **Refactored:** Extracted `build_pagination()` helper for reuse
- **Refactored:** Portfolio grid links now use dynamic `portfolio_slug` setting instead of hardcoded "/portfolio/"
- **Refactored:** Cleaned up redundant `settings` clones in `render_portfolio_single`

### Journal Settings Audit ✅
- **Bug fix:** `blog_display_type` now wired — supports "grid" (CSS grid), "masonry" (column-count), "list" (default compact or editorial)
- **Bug fix:** `blog_grid_columns` now wired via CSS var `--blog-grid-columns` (separate from portfolio's `--grid-columns`)
- **Bug fix:** `blog_list_style` now wired — "editorial" mode uses larger images, bigger headings, more spacing
- **Bug fix:** `blog_excerpt_words` now wired — `truncate_words()` helper truncates excerpts to configured word count
- **Bug fix:** `blog_pagination_type` now wired — supports "classic", "load_more", "infinite" (reuses same JS from portfolio)
- **Bug fix:** `blog_show_author` now wired — conditionally shows author name in list + single views
- **Bug fix:** `blog_show_date` now wired — conditionally shows date in list + single views
- **Bug fix:** `blog_show_reading_time` now wired — shows "X min read" based on word_count / 200 wpm
- **Bug fix:** Blog list links now use dynamic `blog_slug` setting instead of hardcoded "/blog/"
- **Refactored:** Blog meta (author · date · reading time) extracted into reusable `.blog-meta` div
- **Added:** `truncate_words()` helper function for excerpt truncation

### Slug Validation Fixes ✅
- **Bug fix:** Slug validation now checks if module is *enabled* before validating — disabled module's empty slug no longer triggers "both empty" error
- **Bug fix:** Added duplicate slug check — both enabled modules cannot share the same slug
- **Proactive:** When re-enabling a module with empty slug, auto-fills default slug ("journal"/"blog" for Journal, "portfolio"/"gallery" for Portfolio) to prevent future conflicts
- **Proactive:** Fallback slug avoids collision with the other module's current slug

### Potential Enhancements
- Wire captcha into comment form via design templates (currently only in default `render.rs`)
- Magic link token cleanup cron/scheduled task
- Session cleanup scheduled task
- Comment notification emails (using email module)
- Password reset via email flow
