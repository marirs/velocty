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

### Potential Enhancements
- Wire captcha into comment form via design templates (currently only in default `render.rs`)
- Magic link token cleanup cron/scheduled task
- Session cleanup scheduled task
- Comment notification emails (using email module)
- Password reset via email flow
