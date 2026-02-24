# Velocty — Architecture & Specification

**CMS almost at the speed of light.**

This document expands on `README-CMS.md` with detailed architecture decisions, settings structure, authentication, the design/render system, AI integration, and import pipeline.

---

## Tech Stack

| Layer | Technology |
|---|---|
| **Backend** | Rust + Rocket |
| **Database** | SQLite (via rusqlite) or MongoDB (user choice at setup) |
| **Templates (admin)** | Tera (Rocket's built-in template engine) |
| **Page layout builder** | GrapesJS (admin only, ~200KB) — Phase 3 |
| **Content editor** | TinyMCE 7 (self-hosted, admin only, ~3.8MB) |
| **AI** | Ollama (local) / OpenAI / Gemini / Cloudflare (pluggable) — Phase 4 |
| **Frontend (visitors)** | Pure HTML/CSS + minimal vanilla JS |
| **Auth** | Session-based + optional MFA (TOTP) |

---

## Authentication & Security

### Admin Login

- Multi-user system with roles (admin/editor/author/subscriber)
- Session-based auth with secure cookies (`SameSite=Strict`, `HttpOnly`, `Secure` derived from `site_url` + `site_environment` settings)
- Bcrypt password hashing
- Login rate limiting (max 5 attempts per 15 minutes per IP)
- Session expiry (configurable, default 24h)
- Auth guards: `AdminUser`, `EditorUser`, `AuthorUser`, `AuthenticatedUser`

### Multi-Factor Authentication (MFA)

- Optional TOTP-based MFA (Google Authenticator, Authy, etc.)
- Enable/disable from Admin → Settings → Security
- Setup flow:
  1. Admin enables MFA in settings
  2. Server generates TOTP secret + QR code
  3. Admin scans QR with authenticator app
  4. Admin enters 6-digit code to confirm setup
  5. Recovery codes generated (one-time use, stored hashed)
- On login: password → then TOTP code prompt (if MFA enabled)
- Recovery codes for lockout scenarios

### Security Headers

All responses include:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Content-Security-Policy` (strict, admin pages allow GrapesJS/Editor.js)
- `Referrer-Policy: strict-origin-when-cross-origin`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains` (when `site_url` starts with `https://`)

### Security Hardening

- **Constant-time comparison** — all secret comparisons (deploy keys, webhook HMAC signatures, image proxy tokens) use a consolidated SHA-256 hash-then-compare function in `src/security/mod.rs`, preventing both timing and length-leak side-channels
- **OnceLock** — `static mut` in health.rs replaced with `std::sync::OnceLock` to eliminate undefined behavior from data races
- **Download path validation** — commerce download redirects reject absolute URLs, protocol-relative paths (`//`), and `..` traversal
- **Media delete hardening** — null byte rejection + `canonicalize()` verification ensures file is under uploads directory (defense-in-depth against symlinks)
- **Template XSS prevention** — all `json_encode() | safe` usages in Tera templates chain `| replace(from="</", to="<\\/")` to prevent `</script>` breakout injection
- **Error sanitization** — payment provider HTTP errors are logged server-side; clients receive generic "Payment provider request failed" messages
- **Rate limiting** — likes (30/5min/IP), purchase lookups (10/15min/IP) in addition to login and comment rate limits
- **Content-Disposition** — HTML/XHTML files served from uploads forced to `attachment` to prevent inline script execution

---

## Database Abstraction (Store Trait)

All database operations go through a backend-agnostic `Store` trait, enabling seamless switching between SQLite and MongoDB.

### Architecture

```
velocty.toml → backend = "sqlite" | "mongodb"
                    ↓
              create_store()
              ╱            ╲
     SqliteStore          MongoStore
     (wraps DbPool)       (wraps mongodb::Database)
              ╲            ╱
          Arc<dyn Store>  ← managed by Rocket
                ↓
    All routes, fairings, auth guards,
    background tasks, setup wizard
```

### Key Files

| File | Purpose |
|---|---|
| `src/store/mod.rs` | `Store` trait definition (~100 methods) + default implementations + unit tests |
| `src/store/sqlite.rs` | `SqliteStore` — wraps `DbPool` (r2d2), delegates to model methods. Also provides `impl Store for DbPool` bridge |
| `src/store/mongo.rs` | `MongoStore` — fully implemented (~3000 lines), uses mongodb v2 crate with sync API |
| `src/db.rs` | SQLite pool initialization, migrations, `seed_defaults()`, shared `default_settings()` |

### Store Trait Methods

The `Store` trait covers all database operations:

| Category | Methods |
|---|---|
| **Lifecycle** | `run_migrations`, `seed_defaults`, `db_backend` |
| **Settings** | `setting_get`, `setting_set`, `setting_set_many`, `setting_get_group`, `setting_all`, `setting_get_or`, `setting_get_i64` |
| **Users** | `user_create`, `user_get_by_id`, `user_get_by_email`, `user_list`, `user_count`, `user_update_*`, `user_delete` |
| **Sessions** | `session_create_full`, `session_get_user`, `session_validate`, `session_delete`, `session_cleanup_expired` |
| **Posts** | Full CRUD + listing, filtering, counting, slug lookup |
| **Portfolio** | Full CRUD + listing, filtering, counting, slug lookup, likes |
| **Categories** | CRUD + type filtering, nav-visible listing, content associations |
| **Tags** | CRUD + content associations, unused cleanup |
| **Comments** | CRUD + moderation (approve/spam/delete), threaded replies |
| **Designs** | CRUD + activation, slug lookup, templates |
| **Analytics** | Record page views, query stats, cleanup |
| **Audit** | Log actions, list entries |
| **Firewall** | Bans CRUD, events logging, IP lookup, stats |
| **Commerce** | Orders, download tokens, licenses — full CRUD |
| **Passkeys** | WebAuthn credential storage, registration state |
| **Imports** | Import history tracking |
| **Search** | Full-text search indexing |
| **Raw SQL** | `raw_execute`, `raw_query_i64` (SQLite-only, returns error on MongoDB) |
| **Background Tasks** | Session cleanup, analytics pruning, orphan detection |

### MongoStore Implementation Details

- **Auto-increment IDs** via `_counters` collection with `next_id()` — maintains SQLite-compatible integer IDs
- **Junction tables** as separate collections: `content_categories`, `content_tags`
- **Aggregation pipelines** for revenue totals, top IPs, event counts
- **Date parsing** handles RFC3339 and common datetime formats
- **Duration parsing** for firewall ban durations (e.g. "24h", "7d", "30m")
- **Indexes** created in `run_migrations()` for all collections
- **Seed defaults** at full parity with SQLite (settings, designs with shell HTML/CSS, legal content backfill, design sync)

### How Routes Use the Store

All route handlers, fairings, and auth guards receive `&State<Arc<dyn Store>>`:

```rust
#[get("/dashboard")]
fn dashboard(store: &State<Arc<dyn Store>>, _admin: EditorUser) -> Template {
    let s: &dyn Store = &**store;
    let posts = s.post_list_all();
    // ...
}
```

Auth guards resolve sessions via the Store:

```rust
async fn resolve_session_user(request: &Request<'_>) -> Option<User> {
    let store = request.guard::<&State<Arc<dyn Store>>>().await.succeeded()?;
    let session_id = cookies.get_private("velocty_session")?.value().to_string();
    store.session_get_user(&session_id)
}
```

### Where DbPool Still Exists

`DbPool` is intentionally retained in the SQLite implementation layer:

| Location | Purpose |
|---|---|
| `src/models/*.rs` | SQLite model methods (called by `SqliteStore`) |
| `src/store/sqlite.rs` | `SqliteStore` wraps `DbPool` |
| `src/db.rs` | Pool initialization + migrations |
| `src/health.rs` | SQLite PRAGMA queries (guarded by `Option<&DbPool>`) |
| `src/site.rs` | Legacy `SitePoolManager` for SQLite health tools |
| `src/main.rs` | Conditional `DbPool` creation only when `backend != "mongodb"` |
| `src/tests.rs` | Test harness (SQLite in-memory) |

**Zero** `State<DbPool>` references exist in any route handler or fairing.

---

## First-Run Setup Wizard

On first launch (when `setup_completed` is not set), the admin panel redirects to a 4-step setup wizard at `/<admin_slug>/setup`.

### Step 1: Database Backend

The user chooses between **SQLite** and **MongoDB**. Each option displays pros and cons.

**SQLite** (default):
- Zero setup, embedded, single-file database
- Fast reads, easy backup, low resources
- Risk: deleting the file loses all data, no replication

**MongoDB**:
- Replica sets, concurrent writes, cloud-ready (Atlas)
- Natural multi-site fit (one database per site)
- Requires a running MongoDB server

When MongoDB is selected, additional fields appear:
- **Connection URI** — `mongodb://` or `mongodb+srv://`
- **Database Name**
- **Requires authentication** checkbox, which reveals:
  - **Auth Mechanism** — SCRAM-SHA-256, SCRAM-SHA-1, X.509, LDAP, AWS IAM
  - **Username / Password** (hidden for X.509 and AWS)
  - **Auth Database** (default: "admin")
- **Test Connection** button — calls `POST /<admin_slug>/setup/test-mongo` which:
  1. Parses the URI to extract host:port
  2. TCP connects with 5-second timeout
  3. Sends a MongoDB OP_MSG `isMaster` wire protocol handshake
  4. Returns JSON `{ ok: bool, message: String }`

### Step 2: Site Name

### Step 3: Admin Account (email + password)

### Step 4: Terms & Privacy acceptance

### Output: `velocty.toml`

On submit, the wizard writes `velocty.toml` at the project root:

```toml
# SQLite
[database]
backend = "sqlite"
path = "website/site/db/velocty.db"

# MongoDB (with auth)
[database]
backend = "mongodb"
uri = "mongodb://localhost:27017"
name = "velocty"

[database.auth]
mechanism = "scram_sha256"
auth_db = "admin"
username = "myuser"
password = "mypass"
```

The backend choice is locked after first run and stored in both `velocty.toml` and the `db_backend` setting.

---

## Health Dashboard

The admin panel includes a **Health** page (`/<admin_slug>/health`) with two tabs: **Status** and **Tools**. The dashboard is backend-aware — it reads `velocty.toml` to detect SQLite vs MongoDB and adapts its display and available tools accordingly.

### Status Tab

| Section | SQLite | MongoDB |
|---|---|---|
| **Disk** | Total/free/used, DB file size, uploads breakdown (images, video, other) with D3 donut chart | Same but no DB file size (remote) |
| **Database** | File size, WAL size, page count, fragmentation %, integrity check | Connection status (✓/✗), latency (ms), masked URI |
| **Resources** | Uptime, memory RSS, OS/arch, Velocty version | Same |
| **Filesystem** | Permission checks on `db/`, `uploads/`, `designs/`, `static/`, `templates/` | Same but skips `db/` directory |
| **Content** | Post/portfolio/comment/category/tag/session counts with D3 bar chart | Same |
| **Uploads** | File count, image/video/other size breakdown with D3 chart | Same |

### Filesystem Checks

Each checked directory shows:

| Column | Description |
|---|---|
| **Path** | Directory path |
| **Exists** | ✓ or ✗ |
| **Writable** | Write test (creates + removes temp file) |
| **Owner:Group** | Unix owner/group names (red if `root`) |
| **Perms** | Actual octal — green if correct, yellow if wrong, red if world-writable |
| **Expected** | Recommended octal (`750` for db, `755` for others) |
| **Status** | Overall ✓ or ✗ |

Warning rows appear below problem entries with actionable `chmod` commands.

Additional checks:
- **Running as root** — red banner if the process UID is 0
- **Process user** — displays the effective user running Velocty

### Tools Tab

| Tool | SQLite | MongoDB | Description |
|---|---|---|---|
| **Integrity Check** | ✓ | — | `PRAGMA integrity_check` |
| **Vacuum** | ✓ | — | `VACUUM` with old→new size and % reclaimed |
| **WAL Checkpoint** | ✓ | — | `PRAGMA wal_checkpoint(TRUNCATE)` |
| **Connection Ping** | — | ✓ | TCP + OP_MSG `isMaster` with latency |
| **Session Cleanup** | ✓ | ✓ | Delete expired sessions |
| **Orphan File Scan** | ✓ | ✓ | Find uploads not referenced by content |
| **Delete Orphan Files** | ✓ | ✓ | Permanently remove orphans |
| **Unused Tags Cleanup** | ✓ | ✓ | Delete tags with no associations |
| **Analytics Pruning** | ✓ | ✓ | Delete events older than N days |
| **Export Database** | ✓ | — | Copy `.db` file to downloads |
| **Export Content** | ✓ | ✓ | JSON export of all content |

### Multi-Site Tools

In multi-site mode, the Super Admin health page (`/super/health`) includes a **Maintenance Tools** section with a site selector dropdown. Each tool operates on the selected site's database and uploads directory.

| Route | Description |
|---|---|
| `POST /super/health/tool/<site_id>/vacuum` | Vacuum the selected site's DB |
| `POST /super/health/tool/<site_id>/wal-checkpoint` | WAL checkpoint for the selected site |
| `POST /super/health/tool/<site_id>/integrity-check` | Integrity check on the selected site's DB |
| `POST /super/health/tool/<site_id>/session-cleanup` | Clean expired sessions for the selected site |
| `POST /super/health/tool/<site_id>/orphan-scan` | Scan for orphan files in the selected site's uploads |
| `POST /super/health/tool/<site_id>/orphan-delete` | Delete orphan files from the selected site's uploads |
| `POST /super/health/tool/<site_id>/unused-tags` | Clean unused tags for the selected site |
| `POST /super/health/tool/<site_id>/export-content` | Export content JSON for the selected site |

The tool routes resolve the site ID → slug via the registry, then use `SiteStoreManager` to get the site's `Arc<dyn Store>` (or `SitePoolManager` for SQLite-specific health tools). Orphan scan/delete use the per-site uploads path (`website/sites/<uuid>/uploads`).

Per-site admin tools (`/<admin_slug>/health`) work identically in both single-site and multi-site modes — they always operate on the current site's database.

### Implementation

- **Backend**: `src/health.rs` — `gather()` takes `Option<&DbPool>` + `&dyn Store`, branches to `gather_db_sqlite()` or `gather_db_mongo()`
- **Routes**: `src/routes/admin/health.rs` — `GET /health` + `POST /health/<tool>` endpoints (single-site)
- **Routes**: `src/routes/super_admin/health.rs` — `POST /health/tool/<site_id>/<tool>` endpoints (multi-site)
- **Template**: `website/templates/admin/health.html.tera` — Tera conditionals on `report.database.backend`
- **Template**: `website/templates/super/health.html.tera` — System health + site selector tools
- **MongoDB ping**: Raw TCP + OP_MSG wire protocol (same approach as setup test-mongo)

---

## Cookie Consent & Legal Pages

### Cookie Consent Banner

Configurable GDPR-compliant cookie consent banner, disabled by default. When enabled:

- **3 styles**: `minimal` (bottom/top bar), `modal` (centered overlay), `corner` (bottom-left card)
- **3 themes**: `auto` (dark), `dark`, `light`
- **3 actions**: Accept All, Necessary Only, Reject All (optional)
- Sets `velocty_consent` cookie (365 days) with value `all`, `necessary`, or `none`
- Analytics `<script>` tags are output as `type="text/plain" data-consent="analytics"` — they don't execute until the visitor accepts
- On acceptance, the banner JS activates gated scripts by cloning them with the correct `type`

### Privacy Policy & Terms of Use

- Pre-filled with industry-standard Markdown templates at seed time
- Editable from **Settings › Frontend** (sub-tabs: General, Privacy Policy, Terms of Use)
- Rendered at `/privacy` and `/terms` using `pulldown-cmark` (Markdown → HTML)
- Pages use the same site shell (sidebar, fonts, CSS) with clean legal typography
- Return 404 when disabled
- Enabling cookie consent auto-enables the privacy policy page

### Settings

| Key | Default | Description |
|---|---|---|
| `cookie_consent_enabled` | `false` | Show cookie consent banner |
| `cookie_consent_style` | `minimal` | `minimal`, `modal`, `corner` |
| `cookie_consent_position` | `bottom` | `bottom`, `top` |
| `cookie_consent_policy_url` | `/privacy` | Link in "Learn more" |
| `cookie_consent_show_reject` | `true` | Show "Reject All" button |
| `cookie_consent_theme` | `auto` | `auto`, `dark`, `light` |
| `privacy_policy_enabled` | `false` | Enable `/privacy` page |
| `privacy_policy_content` | *(template)* | Markdown content |
| `terms_of_use_enabled` | `false` | Enable `/terms` page |
| `terms_of_use_content` | *(template)* | Markdown content |

### Implementation

- **Banner**: `src/render.rs` — `build_cookie_consent_banner()` generates inline HTML/CSS/JS
- **Analytics gating**: `src/render.rs` — `build_analytics_scripts()` uses `type="text/plain"` when consent enabled
- **Routes**: `src/routes/public.rs` — `GET /privacy`, `GET /terms`
- **Rendering**: `src/render.rs` — `render_legal_page()` with `pulldown-cmark` Markdown → HTML
- **Settings UI**: `website/templates/admin/settings/design.html.tera` — 3 sub-tabs

---

## Design System

### Concept

No themes. No theme API. No child themes. Instead:

- **Designs** are saved HTML + CSS layouts created in a Wix-like drag-and-drop builder (GrapesJS)
- User creates multiple designs, previews them, and **activates one** as the live site template
- A design is a **set of page templates** — one per page type

### Page Template Types (per design)

| Template | Used for |
|---|---|
| `homepage` | Site landing page |
| `blog_list` | Blog archive / listing page |
| `blog_single` | Individual blog post |
| `portfolio_grid` | Portfolio archive / gallery page |
| `portfolio_single` | Individual portfolio item |
| `page` | Generic static pages (about, contact, etc.) |
| `404` | Not found page |

Each template is a separate HTML + CSS document within the design. GrapesJS edits one template at a time, but they're grouped and managed as a single design.

### Content Placeholders

Designs use **placeholder components** that GrapesJS renders as draggable blocks with sample data. At serve time, Rust replaces them with real content.

#### Global Placeholders (available in all templates)

| Placeholder | Renders as |
|---|---|
| `{{site_title}}` | Site name from settings |
| `{{site_tagline}}` | Tagline from settings |
| `{{site_logo}}` | Logo image |
| `{{navigation}}` | Auto-generated nav menu |
| `{{footer}}` | Footer content |
| `{{social_links}}` | Social media icons |
| `{{current_year}}` | Current year (for copyright) |

#### Blog Placeholders

| Placeholder | Renders as |
|---|---|
| `{{blog_list}}` | Paginated list of posts (respects blog settings: grid/masonry/list) |
| `{{post_title}}` | Post title |
| `{{post_content}}` | Post body (Editor.js JSON → HTML) |
| `{{post_date}}` | Publish date (formatted per settings) |
| `{{post_author}}` | Author name |
| `{{post_excerpt}}` | Post excerpt |
| `{{post_featured_image}}` | Featured image |
| `{{post_categories}}` | Category links |
| `{{post_tags}}` | Tag links |
| `{{post_comments}}` | Comments section |
| `{{post_navigation}}` | Previous / Next post links |

#### Portfolio Placeholders

| Placeholder | Renders as |
|---|---|
| `{{portfolio_grid}}` | Grid of portfolio items (respects portfolio settings) |
| `{{portfolio_title}}` | Item title |
| `{{portfolio_image}}` | Full image |
| `{{portfolio_description}}` | Description (Editor.js JSON → HTML) |
| `{{portfolio_categories}}` | Category links |
| `{{portfolio_tags}}` | Tag links |
| `{{portfolio_likes}}` | Heart/like button + count |
| `{{portfolio_buy_button}}` | PayPal buy / download section (Phase 2) |
| `{{portfolio_meta}}` | Date, categories, share links |

#### SEO Placeholders (auto-injected into `<head>`)

These are **not** draggable — they're automatically injected:
- Meta title, meta description
- Open Graph tags
- Twitter Card tags
- JSON-LD structured data
- Canonical URL

### Storage

```
designs/
├── design-001/
│   ├── meta.toml              # name, author, created_at
│   ├── homepage.html          # GrapesJS HTML
│   ├── homepage.css           # GrapesJS CSS
│   ├── blog_list.html
│   ├── blog_list.css
│   ├── blog_single.html
│   ├── blog_single.css
│   ├── portfolio_grid.html
│   ├── portfolio_grid.css
│   ├── portfolio_single.html
│   ├── portfolio_single.css
│   ├── page.html
│   ├── page.css
│   ├── 404.html
│   ├── 404.css
│   └── thumbnail.png          # auto-generated preview
├── design-002/
│   └── ...
```

Also stored in the `designs` table in SQLite for metadata and active flag. The HTML/CSS files are the source of truth for layout content.

### Render Pipeline

```
Visitor requests /blog/my-post
  → Rocket route matches blog_single
  → Fetch active design's blog_single.html + blog_single.css
  → Fetch post from DB (content_html, title, date, etc.)
  → Replace placeholders with real data
  → Inject SEO meta into <head>
  → Serve pure HTML/CSS response
  → ~microsecond response time
```

### Default Design (Phase 1)

Before GrapesJS exists (Phase 3), a **hardcoded default design** ships with the binary:
- Clean, minimalist layout inspired by your current Minimalio/Oneguy setup
- Responsive, mobile-first
- All 7 template types included
- When Phase 3 lands, this becomes "Default" in the design manager — user can modify or replace it

---

## Settings

All settings stored in `settings` table as key-value pairs (`key TEXT PRIMARY KEY, value TEXT`).

### General

| Key | Description | Default |
|---|---|---|
| `site_name` | Site name | "Velocty" |
| `site_tagline` | Site tagline | "" |
| `site_logo` | Path to logo image | "" |
| `site_favicon` | Path to favicon | "" |
| `site_url` | Public site URL | "http://localhost:8000" |
| `timezone` | Timezone | "UTC" |
| `date_format` | Date display format | "%B %d, %Y" |
| `admin_email` | Admin email address | "" |

### Security

| Key | Description | Default |
|---|---|---|
| `mfa_enabled` | TOTP multi-factor auth | "false" |
| `mfa_secret` | Encrypted TOTP secret | "" |
| `mfa_recovery_codes` | Hashed recovery codes (JSON array) | "[]" |
| `session_expiry_hours` | Session lifetime | "24" |
| `login_rate_limit` | Max login attempts per 15 min | "5" |

### Blog

| Key | Description | Default |
|---|---|---|
| `blog_posts_per_page` | Posts per page | "10" |
| `blog_display_type` | grid / masonry / list | "grid" |
| `blog_list_style` | compact / classic / editorial (when list) | "compact" |
| `blog_excerpt_words` | Excerpt word count | "40" |
| `blog_show_author` | Show author on posts | "true" |
| `blog_show_date` | Show date on posts | "true" |
| `blog_show_reading_time` | Show estimated reading time | "true" |
| `blog_default_status` | Default post status | "draft" |
| `blog_featured_image_required` | Require featured image | "false" |

### Portfolio

| Key | Description | Default |
|---|---|---|
| `portfolio_items_per_page` | Items per page | "12" |
| `portfolio_grid_columns` | Grid columns (2/3/4) | "3" |
| `portfolio_enable_likes` | Enable heart/like | "true" |
| `portfolio_heart_position` | image-bottom-right / image-bottom-left / after-meta | "image-bottom-right" |
| `portfolio_image_protection` | Disable right-click on images | "false" |
| `portfolio_featured_image_scale` | Image size scaling | "original" |
| `portfolio_fade_animation` | Fade-in on scroll | "true" |
| `portfolio_show_categories` | Show categories on archive | "true" |
| `portfolio_show_tags` | Show tags on archive | "true" |
| `portfolio_click_mode` | lightbox / single_page | "lightbox" |
| `portfolio_lightbox_border_color` | Border color for lightbox frame | "#D4A017" |
| `portfolio_lightbox_show_title` | Show title in lightbox | "true" |
| `portfolio_lightbox_show_tags` | Show tags in lightbox | "true" |
| `portfolio_lightbox_show_likes` | Show heart in lightbox | "true" |
| `portfolio_lightbox_nav` | Show prev/next arrows | "true" |
| `portfolio_lightbox_keyboard` | Keyboard nav (Esc, arrows) | "true" |

#### Portfolio Click Behavior — Two Modes

**Mode 1: Lightbox** (`portfolio_click_mode = "lightbox"`)
- Click image → overlay opens on same page (no navigation)
- Dark semi-transparent backdrop dims the grid behind
- Image displayed large, centered, with a styled border (color configurable)
- Below the image: title, categories/tags, heart/like button
- Prev/next arrows to browse items without closing
- Close: X button, click backdrop, or Esc key
- Keyboard: Esc to close, arrow keys for prev/next
- No comments in lightbox mode
- If sell enabled (Phase 2): show price + "Buy" button
- Vanilla JS, no library dependency

**Mode 2: Single Page** (`portfolio_click_mode = "single_page"`)
- Click image → navigates to `/portfolio/slug` (full page load)
- Uses the `portfolio_single` design template
- Shows: full image, title, description, categories, tags, date, heart/like
- Comments section (if enabled)
- PayPal buy section (Phase 2, if sell enabled)
- Full SEO: unique URL, meta tags, JSON-LD, OG tags
- Previous/Next navigation links

**Important:** Even in lightbox mode, `/portfolio/slug` single pages are always generated server-side for:
- SEO — search engines index individual items
- Direct links — shareable URLs
- Fallback — if JS fails, the `<a href="/portfolio/slug">` link still works

The lightbox intercepts clicks with JS; the underlying `<a>` always points to the single page.

### Comments

| Key | Description | Default |
|---|---|---|
| `comments_enabled` | Global enable/disable | "true" |
| `comments_on_blog` | Enable on blog posts | "true" |
| `comments_on_portfolio` | Enable on portfolio items | "false" |
| `comments_moderation` | auto-approve / manual / disabled | "manual" |
| `comments_honeypot` | Honeypot spam protection | "true" |
| `comments_rate_limit` | Max comments per IP per hour | "5" |
| `comments_require_name` | Require name field | "true" |
| `comments_require_email` | Require email field | "true" |

### Fonts & Typography

| Key | Description | Default |
|---|---|---|
| `font_primary` | Primary body font | "Inter" |
| `font_heading` | Heading font | "Inter" |
| `font_source` | google / local / custom | "google" |
| `font_size_body` | Body font size | "16px" |
| `font_size_h1` | H1 size | "2.5rem" |
| `font_size_h2` | H2 size | "2rem" |
| `font_size_h3` | H3 size | "1.75rem" |
| `font_size_h4` | H4 size | "1.5rem" |
| `font_size_h5` | H5 size | "1.25rem" |
| `font_size_h6` | H6 size | "1rem" |
| `font_text_transform` | uppercase / lowercase / capitalize / none | "none" |

### Media — Images

| Key | Description | Default |
|---|---|---|
| `images_storage_path` | Upload directory | "website/site/uploads/" |
| `images_max_upload_mb` | Max upload size in MB | "10" |
| `images_thumb_small` | Small thumbnail dimensions | "150x150" |
| `images_thumb_medium` | Medium thumbnail dimensions | "300x300" |
| `images_thumb_large` | Large thumbnail dimensions | "1024x1024" |
| `images_quality` | JPEG/WebP quality (1-100) | "85" |
| `images_webp_convert` | Auto-convert to WebP | "true" |
| `images_allowed_types` | Allowed image extensions | "jpg,jpeg,png,gif,webp,svg,tiff,heic" |

### Media — Video

| Key | Description | Default |
|---|---|---|
| `video_upload_enabled` | Enable video uploads | "false" |
| `video_max_upload_mb` | Max video upload size in MB | "100" |
| `video_allowed_types` | Allowed video extensions | "mp4,webm,mov,avi,mkv" |
| `video_max_duration` | Max duration in seconds (0 = no limit) | "0" |
| `video_generate_thumbnail` | Auto-generate video thumbnail | "true" |

### Media — Organization

| Key | Description | Default |
|---|---|---|
| `media_organization` | Upload folder structure | "flat" |

Allowed values for `media_organization`:

| Value | Structure | Example |
|---|---|---|
| `flat` | All files in one folder | `photo.jpg` |
| `year` | `<year>/` | `2026/photo.jpg` |
| `year_month` | `<year>/<month>/` | `2026/02/photo.jpg` |
| `category_year` | `<category>/<year>/` | `landscapes/2026/photo.jpg` |
| `category_year_month` | `<category>/<year>/<month>/` | `landscapes/2026/02/photo.jpg` |
| `category` | `<category>/` | `landscapes/photo.jpg` |

### SEO

| Key | Description | Default |
|---|---|---|
| `seo_title_template` | Title template | "{{title}} — {{site_name}}" |
| `seo_default_description` | Fallback meta description | "" |
| `seo_sitemap_enabled` | Generate sitemap.xml | "true" |
| `seo_structured_data` | JSON-LD enabled | "true" |
| `seo_open_graph` | Open Graph tags enabled | "true" |
| `seo_twitter_cards` | Twitter Card tags enabled | "true" |
| `seo_canonical_base` | Canonical URL base | "" (uses site_url) |
| `seo_robots_txt` | Custom robots.txt content | "User-agent: *\nAllow: /" |

### SEO — Webmaster Tools

| Key | Description | Default |
|---|---|---|
| `seo_google_verification` | Google Search Console verification code | "" |
| `seo_bing_verification` | Bing Webmaster Tools verification code | "" |
| `seo_yandex_verification` | Yandex Webmaster verification code | "" |
| `seo_pinterest_verification` | Pinterest domain verification code | "" |
| `seo_baidu_verification` | Baidu Webmaster verification code | "" |

### SEO — Analytics Providers

| Key | Description | Default |
|---|---|---|
| `seo_ga_enabled` | Enable Google Analytics (GA4) | "false" |
| `seo_ga_measurement_id` | GA4 Measurement ID (G-XXXXXXXXXX) | "" |
| `seo_plausible_enabled` | Enable Plausible Analytics | "false" |
| `seo_plausible_domain` | Plausible tracked domain | "" |
| `seo_plausible_host` | Plausible instance URL | "https://plausible.io" |
| `seo_fathom_enabled` | Enable Fathom Analytics | "false" |
| `seo_fathom_site_id` | Fathom Site ID | "" |
| `seo_matomo_enabled` | Enable Matomo Analytics | "false" |
| `seo_matomo_url` | Matomo instance URL | "" |
| `seo_matomo_site_id` | Matomo Site ID | "1" |
| `seo_cloudflare_analytics_enabled` | Enable Cloudflare Web Analytics | "false" |
| `seo_cloudflare_analytics_token` | Cloudflare beacon token | "" |
| `seo_clicky_enabled` | Enable Clicky Analytics | "false" |
| `seo_clicky_site_id` | Clicky Site ID | "" |
| `seo_umami_enabled` | Enable Umami Analytics | "false" |
| `seo_umami_website_id` | Umami Website ID | "" |
| `seo_umami_host` | Umami instance URL | "https://analytics.umami.is" |

### Frontend

| Key | Description | Default |
|---|---|---|
| `design_active_id` | Active design ID | "default" |
| `design_back_to_top` | Back-to-top button | "true" |
| `social_links` | Social media links (JSON) | "[]" |
| `social_brand_colors` | Use brand colors for icons | "true" |

### Commerce

#### Global Commerce Settings

| Key | Description | Default |
|---|---|---|
| `commerce_currency` | Currency code | "USD" |
| `downloads_max_per_purchase` | Max downloads per token | "3" |
| `downloads_expiry_hours` | Link expiry in hours | "48" |
| `downloads_license_template` | License text template | (default license text) |

#### Provider Enable Toggles

| Key | Provider | Required Fields |
|---|---|---|
| `commerce_paypal_enabled` | PayPal | `paypal_client_id`, `paypal_secret` |
| `commerce_stripe_enabled` | Stripe | `stripe_publishable_key`, `stripe_secret_key` |
| `commerce_razorpay_enabled` | Razorpay | `razorpay_key_id`, `razorpay_key_secret` |
| `commerce_mollie_enabled` | Mollie | `mollie_api_key` |
| `commerce_square_enabled` | Square | `square_application_id`, `square_access_token`, `square_location_id` |
| `commerce_2checkout_enabled` | 2Checkout | `twocheckout_merchant_code`, `twocheckout_secret_key` |
| `commerce_payoneer_enabled` | Payoneer | `payoneer_program_id`, `payoneer_client_id`, `payoneer_client_secret` |

#### Provider-Specific Keys

| Provider | Keys |
|---|---|
| **PayPal** | `paypal_client_id`, `paypal_secret`, `paypal_mode` (sandbox/live) |
| **Stripe** | `stripe_publishable_key`, `stripe_secret_key`, `stripe_webhook_secret` |
| **Razorpay** | `razorpay_key_id`, `razorpay_key_secret` |
| **Mollie** | `mollie_api_key` |
| **Square** | `square_application_id`, `square_access_token`, `square_location_id`, `square_webhook_signature_key` |
| **2Checkout** | `twocheckout_merchant_code`, `twocheckout_secret_key`, `twocheckout_secret_word` |
| **Payoneer** | `payoneer_program_id`, `payoneer_client_id`, `payoneer_client_secret` |

#### Webhook Security

| Provider | Method |
|---|---|
| Stripe | HMAC-SHA256 of payload with `stripe_webhook_secret` |
| Square | HMAC-SHA256 of URL+body with `square_webhook_signature_key` |
| 2Checkout | MD5 hash of sale_id + vendor_id + invoice_id + `twocheckout_secret_word` |
| Razorpay | HMAC-SHA256 client-side verification |
| Mollie | API fetch-back (server fetches payment status from Mollie API) |
| PayPal | Client-side JS SDK capture (no webhook needed) |
| Payoneer | Webhook with provider verification |

### AI (Phase 4)

| Key | Description | Default |
|---|---|---|
| `ai_provider` | ollama / openai / anthropic | "ollama" |
| `ai_endpoint` | API endpoint URL | "http://localhost:11434" |
| `ai_api_key` | API key (encrypted at rest) | "" |
| `ai_model` | Model name | "llama3:8b" |
| `ai_suggest_meta` | Auto-suggest meta title/desc | "true" |
| `ai_suggest_tags` | Auto-suggest tags | "true" |
| `ai_suggest_categories` | Auto-suggest categories | "false" |
| `ai_suggest_alt_text` | Auto-suggest image alt text | "true" |
| `ai_suggest_slug` | Auto-suggest slug | "true" |
| `ai_theme_generation` | Enable theme generation | "true" |
| `ai_post_generation` | Enable post generation | "true" |
| `ai_temperature` | LLM temperature (0.0–1.0) | "0.7" |

---

## Import System

Modular import pipeline — each source is a separate Rust module that produces a common intermediate format, then inserts into Velocty's DB.

### Supported Sources

| Source | Format | Phase |
|---|---|---|
| **WordPress** | WXR XML export (`Tools → Export`) | Phase 1 |
| **Tumblr** | Tumblr API JSON or export file | Phase 2+ |
| **Markdown** | Folder of `.md` files with YAML frontmatter (Hugo/Jekyll) | Phase 2+ |
| **CSV** | Generic CSV with configurable column mapping | Phase 2+ |

### WordPress Importer (Phase 1)

Parses WP XML export and imports:
- Posts → `posts` table (HTML content converted to Editor.js JSON in Phase 3, stored as HTML until then)
- Portfolio items → `portfolio` table (if `portfolio` post type exists in export)
- Categories → `categories` table
- Tags → `tags` table
- Comments → `comments` table
- Featured images → downloaded and stored locally
- Category/tag assignments → junction tables

### Import Flow

```
Admin → Import → Select Source → Upload File / Configure
  → Preview (show what will be imported, counts, conflicts)
  → Confirm → Import runs in background
  → Progress bar + log
  → Summary (imported X posts, Y portfolio items, Z comments, N skipped)
```

### Import History

| Column | Description |
|---|---|
| `id` | Import ID |
| `source` | wordpress / tumblr / markdown / csv |
| `filename` | Original file name |
| `imported_at` | Timestamp |
| `posts_count` | Posts imported |
| `portfolio_count` | Portfolio items imported |
| `comments_count` | Comments imported |
| `skipped_count` | Items skipped (duplicates, errors) |
| `log` | Detailed import log (JSON) |

---

## AI Integration (Phase 4)

### Pluggable LLM Connector

```rust
trait LlmProvider {
    async fn complete(&self, prompt: &str, options: &LlmOptions) -> Result<String>;
    async fn stream(&self, prompt: &str, options: &LlmOptions) -> Result<Stream<String>>;
}

// Implementations:
// - OllamaProvider (local, http://localhost:11434)
// - OpenAiProvider (cloud, api.openai.com)
// - AnthropicProvider (cloud, api.anthropic.com)
```

### AI Features

| Feature | Trigger | What it does |
|---|---|---|
| **SEO Suggest** | "AI Suggest" button on post/portfolio editor | Generates meta title (≤60 chars), meta description (≤160 chars), slug |
| **Tag Suggest** | "AI Suggest" button | Extracts keywords → suggests tags |
| **Category Suggest** | "AI Suggest" button | Matches content to existing categories |
| **Alt Text** | On image upload | Describes image for accessibility |
| **Post Generation** | "Write with AI" on new post | User describes topic → AI generates full draft as Editor.js blocks |
| **Inline Expand** | Select text → "Expand" | AI elaborates on selected paragraph |
| **Inline Rewrite** | Select text → "Rewrite" | AI rewrites in different tone/style |
| **Inline Summarise** | Select text → "Summarise" | AI generates TL;DR or excerpt |
| **Inline Continue** | Cursor at end → "Continue" | AI continues writing from current position |
| **Theme Generation** | "Generate Design" in design manager | User describes layout → AI generates GrapesJS HTML/CSS |

---

## Database Schema

See `README-CMS.md` for full schema. Additional tables for this architecture:

```sql
-- Orders (provider-agnostic)
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    portfolio_id INTEGER NOT NULL,
    buyer_email TEXT DEFAULT '',
    buyer_name TEXT DEFAULT '',
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    provider TEXT NOT NULL,          -- paypal, stripe, razorpay, mollie, square, 2checkout, payoneer
    provider_order_id TEXT DEFAULT '',
    status TEXT DEFAULT 'pending',   -- pending, completed, refunded
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (portfolio_id) REFERENCES portfolio(id)
);

-- Download tokens (per order)
CREATE TABLE download_tokens (
    id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL,
    token TEXT UNIQUE NOT NULL,
    downloads_used INTEGER DEFAULT 0,
    max_downloads INTEGER DEFAULT 3,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (order_id) REFERENCES orders(id)
);

-- License keys (per order)
CREATE TABLE licenses (
    id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL,
    license_key TEXT UNIQUE NOT NULL, -- XXXX-XXXX-XXXX-XXXX
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (order_id) REFERENCES orders(id)
);

-- Import history
CREATE TABLE imports (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,            -- wordpress, tumblr, markdown, csv
    filename TEXT,
    posts_count INTEGER DEFAULT 0,
    portfolio_count INTEGER DEFAULT 0,
    comments_count INTEGER DEFAULT 0,
    skipped_count INTEGER DEFAULT 0,
    log TEXT,                        -- JSON log
    imported_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Admin sessions
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,             -- session token
    created_at DATETIME NOT NULL,
    expires_at DATETIME NOT NULL,
    ip_address TEXT,
    user_agent TEXT
);

-- Design templates (extends designs table from README)
-- Each design has multiple templates (one per page type)
CREATE TABLE design_templates (
    id INTEGER PRIMARY KEY,
    design_id INTEGER NOT NULL,
    template_type TEXT NOT NULL,      -- homepage, blog_list, blog_single, portfolio_grid, portfolio_single, page, 404
    layout_html TEXT NOT NULL,
    style_css TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (design_id) REFERENCES designs(id),
    UNIQUE(design_id, template_type)
);

-- Built-in analytics (no third-party tracking)
CREATE TABLE page_views (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    ip_hash TEXT NOT NULL,              -- SHA-256 hashed for privacy
    country TEXT,                       -- from GeoLite2 offline DB
    city TEXT,
    referrer TEXT,
    user_agent TEXT,
    device_type TEXT,                   -- desktop / mobile / tablet
    browser TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_views_path ON page_views(path);
CREATE INDEX idx_views_date ON page_views(created_at);
CREATE INDEX idx_views_country ON page_views(country);
CREATE INDEX idx_views_referrer ON page_views(referrer);
```

---

## Slug Validation

The admin panel validates all user-configurable slugs (`admin_slug`, `blog_slug`, `portfolio_slug`) to prevent conflicts with hardcoded system routes.

### Reserved Slugs

The following top-level paths are reserved and cannot be used as any slug:

| Reserved | Reason |
|---|---|
| `static` | Static file server |
| `uploads` | Upload file server |
| `api` | Public JSON API (likes, comments, filtering) |
| `super` | Super admin panel (multi-site) |
| `download` | Commerce download pages |
| `feed` | RSS feed |
| `sitemap.xml` | XML sitemap |
| `robots.txt` | Robots file |
| `privacy` | Privacy policy page |
| `terms` | Terms of use page |
| `archives` | Blog archives |
| `login` | Auth sub-route |
| `logout` | Auth sub-route |
| `setup` | First-run wizard |
| `mfa` | MFA verification |
| `magic-link` | Magic link auth |
| `forgot-password` | Password reset |
| `reset-password` | Password reset |

### Cross-Slug Validation

- All three slugs (`admin_slug`, `blog_slug`, `portfolio_slug`) must be unique from each other when non-empty
- `blog_slug` and `portfolio_slug` may be empty (`""`) — this mounts the module at `/` as the homepage
- `admin_slug` is always required (cannot be empty)
- Both `blog_slug` and `portfolio_slug` cannot be empty at the same time

### Implementation

- **Validation**: `src/routes/admin/settings.rs` — `settings_save()` checks on save for `security`, `blog`, and `portfolio` sections
- Reserved list: `RESERVED_SLUGS` constant + `is_reserved()` helper inside `settings_save()`

---

## Project Structure (Phase 1)

```
velocty/
├── Cargo.toml
├── README-CMS.md
├── docs/
│   ├── Architecture.md
│   ├── DESIGN.md
│   ├── MULTI-SITE.md
│   ├── PROGRESS.md
│   ├── README-CMS.md
│   └── firewall-spec.md
├── src/
│   ├── main.rs                      # Rocket launch, Store init, route mounting
│   ├── db.rs                        # SQLite connection pool, migrations, shared default_settings()
│   ├── store/                       # Backend-agnostic database abstraction
│   │   ├── mod.rs                   # Store trait (~100 methods) + unit tests
│   │   ├── sqlite.rs                # SqliteStore impl (wraps DbPool) + DbPool bridge
│   │   └── mongo.rs                 # MongoStore impl (fully implemented, ~3000 lines)
│   ├── health.rs                    # Health dashboard data gathering + tools
│   ├── render.rs                    # Design + content merge, placeholder replacement
│   ├── rss.rs                       # RSS feed generation
│   ├── analytics.rs                 # Page view logging middleware, GeoLite2 lookup
│   ├── ai/                          # AI provider integrations
│   ├── security/
│   │   ├── auth.rs                  # Login, sessions, guards (AdminUser, EditorUser, AuthorUser)
│   │   ├── mfa.rs                   # TOTP MFA helpers
│   │   └── password_reset.rs        # Password reset tokens + email
│   ├── models/                      # SQLite model implementations (used by SqliteStore)
│   │   ├── mod.rs
│   │   ├── post.rs                  # Post struct, CRUD
│   │   ├── portfolio.rs             # Portfolio struct, CRUD
│   │   ├── category.rs              # Category struct, CRUD
│   │   ├── tag.rs                   # Tag struct, CRUD
│   │   ├── comment.rs               # Comment struct, CRUD
│   │   ├── design.rs                # Design struct, CRUD
│   │   ├── settings.rs              # Settings get/set helpers + SettingsCache
│   │   ├── import.rs                # Import history
│   │   ├── analytics.rs             # Page views, stats queries
│   │   ├── audit.rs                 # Audit log entries
│   │   ├── firewall.rs              # Firewall events + bans
│   │   ├── order.rs                 # Orders, download tokens, licenses
│   │   └── user.rs                  # Multi-user model
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── public.rs                # Visitor-facing routes (blog, portfolio, RSS, sitemap)
│   │   ├── api.rs                   # Public JSON API (likes, comments, filtering)
│   │   ├── ai.rs                    # AI suggestion endpoints
│   │   ├── admin/                   # Admin panel routes
│   │   │   ├── mod.rs               # Shared helpers (admin_base, save_upload), routes(), api_routes()
│   │   │   ├── dashboard.rs         # Dashboard
│   │   │   ├── posts.rs             # Posts CRUD
│   │   │   ├── portfolio.rs         # Portfolio CRUD
│   │   │   ├── comments.rs          # Comments moderation
│   │   │   ├── categories.rs        # Categories + tags CRUD
│   │   │   ├── media.rs             # Media library, image/font uploads
│   │   │   ├── settings.rs          # Settings page + save (with slug validation)
│   │   │   ├── designs.rs           # Design manager
│   │   │   ├── import.rs            # WordPress + Velocty import
│   │   │   ├── health.rs            # Health dashboard + tools
│   │   │   ├── users.rs             # User management + MFA setup
│   │   │   ├── firewall.rs          # Firewall dashboard + audit log + ban/unban
│   │   │   ├── sales.rs             # Sales dashboard + orders
│   │   │   └── api.rs               # Admin JSON API (stats, SEO check, theme)
│   │   ├── security/                # Auth routes
│   │   │   └── auth/
│   │   │       ├── login.rs          # Login page + submit
│   │   │       ├── logout.rs         # Logout + catch-all redirect
│   │   │       ├── setup.rs          # First-run setup wizard
│   │   │       ├── mfa.rs            # MFA challenge page
│   │   │       ├── magic_link.rs     # Magic link auth
│   │   │       └── password_reset.rs # Forgot/reset password
│   │   ├── commerce/                # Payment provider routes
│   │   │   ├── mod.rs               # Shared helpers, order pipeline, download routes
│   │   │   ├── paypal.rs
│   │   │   ├── stripe.rs
│   │   │   ├── razorpay.rs
│   │   │   ├── mollie.rs
│   │   │   ├── square.rs
│   │   │   ├── twocheckout.rs
│   │   │   └── payoneer.rs
│   │   └── super_admin/             # Super admin (multi-site, feature-gated)
│   │       ├── mod.rs               # routes()
│   │       ├── auth.rs              # Setup, login, logout + auth guard
│   │       ├── dashboard.rs         # Dashboard + settings
│   │       ├── sites.rs             # Site CRUD
│   │       └── health.rs            # Per-site health tools
│   └── import/
│       ├── mod.rs
│       └── wordpress.rs             # WP XML parser
├── website/
│   ├── site/                        # Site-specific data
│   │   ├── db/velocty.db            # SQLite database (created at runtime)
│   │   ├── uploads/                 # User uploads (images, files)
│   │   └── designs/                 # Saved GrapesJS designs (Phase 3)
│   ├── static/                      # Static assets for admin
│   │   ├── css/
│   │   │   └── admin.css
│   │   └── js/
│   │       ├── admin.js
│   │       └── tinymce/             # Self-hosted TinyMCE 7
│   └── templates/                   # Tera templates (admin panel)
│       ├── admin/
│       │   ├── base.html.tera
│       │   ├── login.html.tera
│       │   ├── dashboard.html.tera
│       │   ├── posts/
│       │   ├── portfolio/
│       │   ├── comments/
│       │   ├── settings/
│       │   ├── sales/
│       │   └── import/
│       └── super/                   # Super admin templates (multi-site)
```

---

## Build Phases (Updated)

### Phase 1 — Core ✅
- Rocket project scaffold + SQLite schema (all tables)
- Auth: admin login with bcrypt + sessions + optional MFA (TOTP)
- Blog: posts CRUD (TinyMCE rich text), comments with honeypot + rate limiting, RSS feed
- Portfolio: upload with auto-thumbnails, categories, tags, heart/like (IP-based)
- SEO: meta fields, sitemap.xml, JSON-LD, OG/Twitter tags, canonical URLs
- Admin panel: Tera server-rendered forms, dashboard, dark/light themes
- Settings: general, blog, portfolio, comments, fonts, images, SEO, security, frontend
- Default hardcoded design (clean minimalist layout)
- WordPress XML importer with drag-and-drop UI
- Cookie consent banner (GDPR-compliant, 3 styles, analytics gating)
- Privacy Policy & Terms of Use pages (TinyMCE, `/privacy`, `/terms`)
- RSS feed with configurable post count
- Health dashboard with maintenance tools
- 11 email providers + magic link auth
- Login captcha (reCAPTCHA, Turnstile, hCaptcha) + anti-spam (Akismet, CleanTalk, OOPSpam)
- Multi-site support (optional feature flag)

### Phase 2 — Commerce & Auth ✅
- MFA flow: enable TOTP via QR code, verify codes, download recovery codes, disable MFA, login challenge page
- DB schema: orders, download_tokens, licenses tables + portfolio purchase_note, payment_provider, download_file_path columns
- Order/DownloadToken/License models with full CRUD and query helpers
- 7 payment providers: PayPal (JS SDK), Stripe (Checkout + HMAC-SHA256 webhook), Razorpay (JS modal + HMAC verify), Mollie (redirect + API webhook), Square (redirect + HMAC-SHA256 webhook), 2Checkout (redirect + MD5 IPN), Payoneer (redirect + webhook)
- Per-item payment provider selection (dropdown if >1 enabled, auto-assign if 1)
- Portfolio editor: sell_enabled toggle, price input (auto-format), purchase_note, payment provider dropdown, download file URL input
- Commerce settings UI: all 7 providers with validation + downloads + license config
- Sales sidebar menu (conditionally visible) with Dashboard + Orders tabs
- Sales Dashboard: total/30d/7d revenue, order counts by status, recent orders table
- Orders page: filterable by status, paginated
- Public portfolio: single buy button per item, email capture, purchase lookup
- Download page: token-based with license key display, download count tracking, expiry enforcement
- Optional download file path per item (falls back to featured image)
- Buyer email notifications via Gmail SMTP or custom SMTP
- Zero `.unwrap()` calls in all commerce routes — safe error handling throughout

### Phase 3 — Editors & Design Builder
- Editor.js integration for blog/portfolio content
- GrapesJS integration for page layout design
- Custom GrapesJS components for content placeholders
- Design management: create, edit, duplicate, delete, activate, preview
- Migrate default design into GrapesJS-editable format

### Phase 4 — AI
- Pluggable LLM connector (Ollama / OpenAI / Anthropic)
- Content suggestions: meta, tags, categories, alt text, slug
- Blog post generation from description
- Inline assist: expand, rewrite, summarise, continue
- Theme/design generation → GrapesJS
- AI settings in admin
