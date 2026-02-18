<p align="center">
  <img src="website/static/images/logo-transparent.png" alt="Velocty" width="200">
</p>
<p align="center"><strong>CMS almost at the speed of light.</strong></p>

<p align="center">
  <a href="https://github.com/marirs/velocty/actions/workflows/ci.yml"><img src="https://github.com/marirs/velocty/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/marirs/velocty/actions/workflows/release.yml"><img src="https://github.com/marirs/velocty/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust&logoColor=white" alt="Rust"></a>
</p>

<p align="center">
  A purpose-built, blazing-fast content management system written in Rust.<br>
  Focused on blogging and portfolio/photography — no bloat, no plugin ecosystem, just what you need.
</p>

---

## What is Velocty?

Velocty is a self-hosted CMS that ships as a **single binary** backed by **SQLite**. It's designed for photographers, artists, designers, and bloggers who want a fast, secure, and beautiful platform without the overhead of WordPress or similar systems.

It serves pure HTML/CSS to visitors with **microsecond response times**, while giving you a modern, polished admin panel to manage everything.

## Who is it for?

- **Photographers & visual artists** — portfolio grid with lightbox, categories, likes, and digital download sales
- **Bloggers & writers** — rich text editor, categories, tags, comments, RSS
- **Freelancers & creatives** — showcase work + sell digital downloads with built-in commerce
- **Privacy-conscious creators** — built-in analytics (no Google Analytics, no third-party scripts)
- **Self-hosters** — single binary, no PHP, no MySQL, no Docker required

## Who is it NOT for?

- Sites needing a plugin/extension ecosystem
- E-commerce stores with physical products, inventory, or shipping
- Sites requiring server-side rendering frameworks (React, Next.js, etc.)

---

## Screenshots

### Setup Wizard

Velocty guides you through a 4-step setup wizard on first run:

| Step 1 — Database | Step 2 — Your Site |
|---|---|
| ![Database Selection](docs/wizzard-1.png) | ![Site Name](docs/wizzard-2.png) |

| Step 3 — Admin Account | Step 4 — Terms & Privacy |
|---|---|
| ![Admin Account](docs/wizzard-3.png) | ![Terms & Privacy](docs/wizzard-4.png) |

### Admin Dashboard

| Analytics Dashboard | Sales Dashboard |
|---|---|
| ![Dashboard](docs/dashboard.png) | ![Sales Dashboard](docs/sales-dashboard.png) |

### Journal

| Journal List | New Post |
|---|---|
| ![Journal List](docs/journal-list.png) | ![New Post](docs/new-post.png) |

### Settings

| Site | Typography | Portfolio |
|---|---|---|
| ![Site Settings](docs/site-settings.png) | ![Typography Settings](docs/typography-settings.png) | ![Portfolio Settings](docs/portfolio-settings.png) |

| SEO | Security | Frontend |
|---|---|---|
| ![SEO Settings](docs/seo-settings.png) | ![Security Settings](docs/security-settings.png) | ![Frontend Settings](docs/frontend-settings.png) |

| Commerce | Email | AI |
|---|---|---|
| ![Commerce Settings](docs/commerce-settings.png) | ![Email Settings](docs/email%20settings.png) | ![AI Settings](docs/ai-settings.png) |

---

## Tech Stack

| Layer | Technology |
|---|---|
| **Language** | Rust |
| **Web Framework** | Rocket |
| **Database** | SQLite (via rusqlite + r2d2 connection pool) |
| **Templates (admin)** | Tera |
| **Rich Text Editor** | TinyMCE 7 (self-hosted, admin-only) |
| **Analytics Charts** | D3.js (admin-only) |
| **GeoIP** | MaxMind GeoLite2 (offline, privacy-preserving) |
| **Frontend (visitors)** | Pure HTML/CSS + minimal vanilla JS |
| **Auth** | Bcrypt + session cookies + optional TOTP MFA + Magic Link |
| **Background Tasks** | Tokio async runtime (session/token/analytics cleanup) |

### Why Rust?

| Metric | Velocty (Rust) | WordPress (PHP) |
|---|---|---|
| **Response time** | Microseconds | 200–500ms |
| **Memory usage** | ~10–20 MB | ~50–100 MB |
| **Deployment** | Single binary + SQLite file | PHP + MySQL + Apache/Nginx |
| **Attack surface** | Minimal (no plugins) | Huge (plugins, themes, XML-RPC) |
| **Cold start** | Instant | Seconds |
| **Dependencies at runtime** | Zero | PHP extensions, plugins |
| **Updates** | Replace one binary | Core + plugin + theme updates |

---

## Features

### Content

- **Journal (Blog)** — Rich text posts with TinyMCE, categories, tags, excerpts, featured images, publish date picker, inline category creation
- **Portfolio** — Image gallery with masonry grid, lightbox, categories, tags, likes, publish date picker, inline category creation
- **Browse by tag** — `/tag/<slug>` routes for both blog and portfolio with pagination
- **Browse by category** — `/category/<slug>` routes for both blog and portfolio with pagination
- **Archives** — `/archives` page with posts grouped by year/month, drill-down to `/archives/<year>/<month>`
- **Dynamic URL slugs** — Blog and portfolio base URLs are configurable (e.g. `/journal`, `/gallery`) from settings
- **Comments** — Built-in commenting with honeypot spam protection, rate limiting, moderation queue
- **RSS Feed** — Auto-generated RSS 2.0 feed with configurable post count (Settings › Site)
- **WordPress Import** — Import posts, portfolio items, categories, tags, and comments from WP XML export
- **Category management** — Create, edit, delete categories with type filter (post/portfolio/both)

### Portfolio & Photography

- **Masonry grid** with configurable columns (2/3/4)
- **Lightbox** with keyboard navigation, prev/next arrows, configurable border color
- **Single page mode** as alternative to lightbox
- **Heart/like** system (IP-based, no login required)
- **Image protection** — optional right-click disable
- **Fade-in animations** on scroll (IntersectionObserver)
- **Auto-thumbnails** — small, medium, large generated on upload
- **WebP conversion** — automatic for smaller file sizes

### Commerce (Digital Downloads)

- **7 payment providers** — PayPal (JS SDK), Stripe (Checkout), Razorpay (JS modal), Mollie, Square, 2Checkout, Payoneer (redirect-based)
- **Per-item provider selection** — seller chooses which payment processor to use for each portfolio item
- **Sandbox/Live modes** per provider (Stripe, Square, 2Checkout, Payoneer)
- **Webhook security** — Stripe (HMAC-SHA256), Square (HMAC-SHA256), 2Checkout (MD5), Razorpay (HMAC client verify), Mollie (API fetch-back)
- **Order pipeline** — `create_pending_order` → provider checkout → `finalize_order` (idempotent)
- **Secure token-based downloads** with configurable expiry and download limits
- **Optional download file** — seller can specify a separate download file per item; falls back to featured image
- **License key generation** — auto-generated `XXXX-XXXX-XXXX-XXXX` format per purchase
- **Purchase email** — async delivery via Gmail SMTP or custom SMTP with download link + license key
- **Purchase lookup** — returning buyers can check purchase status by email
- **Sales dashboard** — total/30d/7d revenue, order counts, recent orders
- **Orders page** — filterable by status (all/completed/pending/refunded), paginated
- **Price auto-format** — `25` → `25.00` in the portfolio editor

### SEO (Built-in, No Plugins)

- **Meta title & description** fields on every post and portfolio item
- **SEO Check button** — one-click 10-point analysis on each post/portfolio editor (meta title, description, slug quality, content length, image alt text, tags, heading structure) with A–F grade
- **Auto-generated sitemap.xml**
- **JSON-LD structured data** for blog posts and portfolio items
- **Open Graph & Twitter Card** meta tags
- **Canonical URLs**
- **Custom robots.txt**
- **Webmaster Tools** — verification codes for Google Search Console, Bing, Yandex, Pinterest, Baidu (auto-injected into `<head>`)
- **Third-party Analytics** — Google Analytics (GA4), Plausible, Fathom, Matomo, Cloudflare Web Analytics, Clicky, Umami — each with enable/disable toggle (scripts auto-injected into visitor pages)
- **All configurable** from Settings > SEO (tabbed: General, Webmaster Tools, plus per-provider analytics tabs)

### Analytics (Built-in, Privacy-First)

- **No third-party scripts** — all data stays in your SQLite database
- **GeoLite2 offline lookup** — country/city without sending data to external services
- **D3.js dashboard** with:
  - Visitor flow (Sankey diagram)
  - Content breakdown (Sunburst chart)
  - World map (Choropleth)
  - Activity stream
  - Calendar heatmap
  - Top portfolio items (Radial bar)
  - Top referrers (Horizontal bar)
  - Tag relationships (Force-directed graph)
- **Tracked per request:** path, hashed IP, country, referrer, user-agent, device type, browser

### Admin Panel

- **Dark & Light themes** — toggle from sidebar
- **Ultra-narrow icon sidebar** that expands on hover with labels
- **Responsive** — works on mobile (sidebar collapses to bottom tab bar)
- **Keyboard shortcuts** — Cmd+S to save from any form, `/` to focus settings search
- **Flash notifications** — success/error toasts on save
- **Settings search** — search across all settings with keyboard shortcut, grouped dropdown results, sub-tab navigation
- **Multi-user system** — roles (admin/editor/author/subscriber), user management UI, per-user MFA
- **Health Dashboard** — system health with disk usage, DB stats, filesystem permission checks (owner:group, recommended perms, world-writable detection), resource monitoring, and maintenance tools (vacuum, WAL checkpoint, orphan scan, session cleanup, export). Backend-aware: adapts for SQLite vs MongoDB
- **Cookie Consent Banner** — GDPR-compliant banner with 3 styles (minimal bar, modal, corner card), dark/light/auto theme, configurable position. Analytics scripts gated behind consent
- **Privacy Policy & Terms of Use** — pre-filled industry-standard templates, editable with TinyMCE from Settings › Frontend, rendered at `/privacy` and `/terms`
- **Import page** — drag-and-drop file upload with 3-column card layout for WordPress and other importers
- **Background tasks** — automatic session cleanup, magic link token cleanup, analytics data cleanup with configurable intervals (Settings › Tasks)

### Security

- **Bcrypt password hashing**
- **Session-based auth** with secure cookies (SameSite=Strict, HttpOnly)
- **Configurable admin URL slug** — change `/admin` to anything for security through obscurity
- **Authentication modes:**
  - **Email & Password** — traditional login
  - **Magic Link** — passwordless login via email (requires email provider)
- **Optional TOTP MFA** — per-user, Google Authenticator, Authy, etc. with recovery codes
- **Multi-user auth guards** — AdminUser, EditorUser, AuthorUser, AuthenticatedUser with role-based route gating
- **Login rate limiting** — in-memory IP-based enforcement, configurable attempts per 15 minutes
- **Comment rate limiting** — in-memory enforcement, configurable per 15-minute window
- **Login captcha** — reCAPTCHA v3, Cloudflare Turnstile, or hCaptcha
- **Anti-spam services** — Akismet, CleanTalk, OOPSpam
- **Firewall fairing** — bot detection, failed login tracking, auto-ban, XSS/SQLi/path traversal protection, rate limiting, geo-blocking, security headers
- **Session expiry** — configurable (default 24h)
- **Security headers** — X-Content-Type-Options, X-Frame-Options, CSP, Referrer-Policy

### Email

- **11 email providers** — Gmail/Google Workspace, Resend, Amazon SES, Postmark, Brevo, SendPulse, Mailgun, Moosend, Mandrill, SparkPost, Custom SMTP
- **Used for:** Magic Link login, purchase notifications, comment notifications

### Typography & Design

- **Google Fonts** integration with 1,500+ fonts
- **Adobe Fonts** support
- **Custom font upload**
- **Per-element font assignment** — body, headings, navigation, buttons, captions
- **Configurable sizes** for H1–H6 and body
- **Text transform** options

### Settings (16 sections)

| Section | What it controls |
|---|---|
| **Site** | Name, tagline, logo, favicon, URL, timezone, date format |
| **Journal** | Posts per page, display type, excerpt length, reading time |
| **Portfolio** | Grid columns, likes, lightbox, image protection, animations |
| **Comments** | Enable/disable, moderation mode, spam protection, rate limits |
| **Typography** | Fonts, sizes, sources, per-element assignment |
| **Media** | Image upload (max size, quality, WebP, thumbnails), video upload (types, size, duration), media organization (6 folder structures) |
| **SEO** | Title template, meta defaults, sitemap, structured data, robots.txt, webmaster verification, 7 analytics providers |
| **Security** | Admin slug, auth method, MFA, sessions, rate limits, captcha, anti-spam |
| **Frontend** | Active design, back-to-top button |
| **Social** | Social media links with brand color icons |
| **Email** | 11 provider configurations |
| **Commerce** | 7 payment providers, currency, download limits, license template |
| **AI** | Provider chain, model selection, failover |
| **Tasks** | Background task intervals (session cleanup, magic link cleanup, analytics cleanup) |

---

## Quick Start

### Prerequisites

- Rust toolchain (1.75+)
- (Optional) [MaxMind GeoLite2-City.mmdb](https://dev.maxmind.com/geoip/geolite2-free-geolocation-data) for analytics geo-lookup

### Build & Run

```bash
git clone https://github.com/marirs/velocty.git
cd velocty
cargo build --release
./target/release/velocty
```

Open `http://localhost:8000/admin/setup` — the setup wizard walks you through:
1. **Database** — choose SQLite (default) or MongoDB (with connection test & auth config)
2. **Site name**
3. **Admin account**
4. **Terms acceptance**

Your choice is saved to `velocty.toml` and cannot be changed after setup.

### Multi-Site Mode

To serve multiple independent sites from a single binary:

```bash
cargo build --release --features multi-site
./target/release/velocty
```

Open `http://localhost:8000/super/setup` to create the super admin account, then add sites from the dashboard. See [MULTI-SITE.md](docs/MULTI-SITE.md) for full architecture details.

### Configuration

All configuration is done through the admin panel. Settings are stored in the database and take effect immediately (except admin slug, which requires a restart).

`velocty.toml` is generated during first-run setup and stores the database backend choice:

```toml
# SQLite (default)
[database]
backend = "sqlite"
path = "website/site/db/velocty.db"

# MongoDB (alternative)
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

### Directory Structure

```
velocty/
├── Cargo.toml
├── README.md
├── Rocket.toml                  # Rocket config (port, template dir)
├── docs/                        # Documentation & design specs
│   ├── Architecture.md
│   ├── DESIGN.md
│   ├── MULTI-SITE.md            # Multi-site/multi-tenancy architecture
│   └── README-CMS.md
├── src/
│   ├── main.rs                  # Rocket launch, DB init, route mounting
│   ├── db.rs                    # SQLite pool, migrations, seed defaults
│   ├── analytics.rs             # Page view logging fairing, GeoIP
│   ├── render.rs                # Design + content merge (with captcha widget injection)
│   ├── seo.rs                   # Meta tags, JSON-LD, sitemap
│   ├── rss.rs                   # RSS/Atom feed generation
│   ├── images.rs                # Upload, thumbnails, WebP conversion
│   ├── license.rs               # Purchase license.txt generation
│   ├── rate_limit.rs            # In-memory rate limiter (login, comments)
│   ├── tasks.rs                 # Background tasks fairing (session/token/analytics cleanup)
│   ├── site.rs                  # Multi-site: SiteContext, SitePoolManager, SiteResolver (feature-gated)
│   ├── ai/                      # AI provider integrations
│   │   ├── mod.rs               # Provider dispatch, failover chain, types
│   │   ├── prompts.rs           # Prompt builders for all AI features
│   │   ├── ollama.rs            # Ollama provider
│   │   ├── openai.rs            # OpenAI provider
│   │   ├── gemini.rs            # Google Gemini provider
│   │   ├── groq.rs              # Groq provider
│   │   └── cloudflare.rs        # Cloudflare Workers AI provider
│   ├── email/                   # Email provider integrations
│   │   ├── mod.rs               # Provider dispatch, failover chain, SMTP
│   │   ├── gmail.rs             # Gmail / Google Workspace SMTP
│   │   ├── resend.rs            # Resend API
│   │   ├── ses.rs               # Amazon SES (SigV4)
│   │   ├── postmark.rs          # Postmark API
│   │   ├── brevo.rs             # Brevo (Sendinblue) API
│   │   ├── sendpulse.rs         # SendPulse API (OAuth2)
│   │   ├── mailgun.rs           # Mailgun API
│   │   ├── moosend.rs           # Moosend API
│   │   ├── mandrill.rs          # Mandrill (Mailchimp Transactional) API
│   │   ├── sparkpost.rs         # SparkPost API
│   │   └── smtp.rs              # Custom SMTP
│   ├── security/                # Security module
│   │   ├── mod.rs               # Captcha dispatch, spam dispatch, helpers
│   │   ├── auth.rs              # Auth guards (Admin/Editor/Author/Authenticated), sessions, password
│   │   ├── firewall.rs          # Firewall fairing (bot/XSS/SQLi/geo-blocking/rate-limit)
│   │   ├── mfa.rs               # TOTP secret, QR code, verify, recovery codes
│   │   ├── magic_link.rs        # Token gen, email send, verify, cleanup
│   │   ├── password_reset.rs    # Password reset flow
│   │   ├── recaptcha.rs         # Google reCAPTCHA v2/v3
│   │   ├── turnstile.rs         # Cloudflare Turnstile
│   │   ├── hcaptcha.rs          # hCaptcha
│   │   ├── akismet.rs           # Akismet spam detection
│   │   ├── cleantalk.rs         # CleanTalk spam detection
│   │   └── oopspam.rs           # OOPSpam spam detection
│   ├── models/                  # Data models (Post, Portfolio, Category, Order, User, etc.)
│   └── routes/
│       ├── admin.rs             # Admin panel routes
│       ├── admin_api.rs         # Admin JSON API routes
│       ├── api.rs               # Public API (likes, comments, portfolio filter)
│       ├── public.rs            # Public-facing pages (blog, portfolio, archives)
│       ├── ai/                  # AI API routes (suggest, generate, status)
│       ├── commerce/            # Payment provider routes (paypal, stripe, razorpay, etc.)
│       └── security/            # Auth & security routes
│           └── auth/            # Login, MFA, magic link, setup, logout
├── website/
│   ├── site/                    # Site-specific data (single-site mode)
│   │   ├── db/velocty.db        # SQLite database
│   │   ├── uploads/             # User uploads
│   │   └── designs/             # Saved page designs
│   ├── templates/               # Tera templates (admin panel + super admin)
│   ├── static/                  # CSS, JS, images, TinyMCE
│   ├── sites.db                 # Central registry (multi-site mode only)
│   └── sites/                   # Per-site data with UUID folders (multi-site mode only)
│       └── <uuid>/              # Each site mirrors the site/ structure
│           ├── db/velocty.db
│           ├── uploads/
│           └── designs/
└── GeoLite2-City.mmdb           # Optional GeoIP database
```

---

## Build Phases

### Phase 1 — Core ✅

- Rocket + SQLite scaffold with full schema
- Admin panel with dark/light themes
- Journal: posts with TinyMCE, categories, tags, comments, RSS
- Portfolio: upload, masonry grid, lightbox, categories, tags, likes
- Browse by tag & category with pagination for both blog and portfolio
- Archives page (posts grouped by year/month)
- Dynamic URL slugs for blog and portfolio (configurable from settings)
- SEO Check button on post/portfolio editors (10-point analysis with A–F grade)
- Built-in SEO: meta fields, sitemap.xml, JSON-LD, OG/Twitter tags
- Built-in analytics with D3.js dashboard
- WordPress XML importer
- 16 settings sections with full configuration
- Authentication: password, Magic Link, MFA, captcha
- Login & comment rate limiting (in-memory, IP-based)
- Image right-click protection (configurable)
- 7 commerce provider configurations
- 11 email provider configurations
- **Multi-site/multi-tenancy** (optional `--features multi-site` Cargo flag)
  - Per-site SQLite databases in UUID-named folders (opaque to filesystem)
  - Central `sites.db` registry with hostname → UUID mapping
  - Super Admin panel at `/super/` for managing all sites
  - `SiteResolver` fairing for Host-based routing
  - `DashMap`-cached per-site connection pools

### Phase 2 — Commerce ✅

- 7 payment providers: PayPal (JS SDK), Stripe (Checkout + webhook), Razorpay (JS modal + HMAC verify), Mollie (redirect + API webhook), Square (redirect + HMAC webhook), 2Checkout (redirect + MD5 IPN), Payoneer (redirect + webhook)
- Per-item payment provider selection (dropdown if >1 enabled, auto-assign if 1)
- Order pipeline: create pending → provider checkout → finalize (download token + license key + email)
- Token-based secure downloads with configurable expiry and max download count
- Optional download file path per portfolio item (falls back to featured image)
- License key generation per purchase (XXXX-XXXX-XXXX-XXXX)
- Buyer email notifications via Gmail SMTP or custom SMTP
- Sales dashboard (revenue stats, order counts) + Orders page (filterable, paginated)
- Price auto-format in editor (25 → 25.00)
- Zero `.unwrap()` calls in all commerce routes — safe error handling throughout

### Phase 3 — Editors & Design Builder

- GrapesJS integration for drag-and-drop page layout design
- Design management: create, edit, duplicate, delete, activate, preview
- Custom components for content placeholders

### Phase 4 — AI ✅

- Pluggable LLM connector with failover chain (Ollama → OpenAI → Gemini → Groq → Cloudflare Workers AI)
- Provider-agnostic `ai::complete()` — automatic failover to next enabled provider on failure
- SEO suggestions: ✨ buttons on Slug, Tags, Meta Title, Meta Description fields
- Blog post generation from description (title, HTML content, excerpt, tags — all in one shot)
- TinyMCE inline assist: select text → ✨ AI menu → Expand, Rewrite, Summarise, Continue, More Formal, More Casual
- AI features conditionally shown only when at least one provider is enabled
- All AI responses parsed with robust JSON extraction (handles markdown fences, leading text)
- Settings UI: per-provider configuration, draggable failover chain ordering, model download for local LLM
- Zero hardcoded API keys — all credentials stored in settings DB

### Phase 5 — Users, Security & Polish ✅

- **Multi-user system** — users table with roles (admin/editor/author/subscriber), status (active/suspended/locked), per-user MFA
- **Auth guards** — AdminUser, EditorUser, AuthorUser, AuthenticatedUser with role-based route gating
- **User management UI** — admin page with create/edit/suspend/lock/unlock/delete
- **Firewall fairing** — bot detection, failed login tracking, auto-ban, XSS/SQLi/path traversal protection, rate limiting, geo-blocking, security headers
- **Password reset** — email-based flow
- **Background tasks** — tokio-spawned cleanup loops for sessions, magic link tokens, analytics data with configurable intervals
- **Settings search** — client-side search across all 16 settings tabs with `/` keyboard shortcut and sub-tab navigation
- **Editor enhancements** — inline category creation (JSON API), publish date picker, category edit on list page

---

## Documentation

Detailed documentation is in the `docs/` folder:

- **[Architecture.md](docs/Architecture.md)** — Technical architecture, auth system, design system, render pipeline, AI integration, full settings reference, database schema
- **[DESIGN.md](docs/DESIGN.md)** — Visual design specification for admin panel and default visitor design, color palettes (dark & light), wireframes, responsive breakpoints
- **[MULTI-SITE.md](docs/MULTI-SITE.md)** — Multi-site/multi-tenancy architecture: storage layout, central registry schema, request flow, key types, super admin panel, routing strategy, feature flag boundaries
- **[README-CMS.md](docs/README-CMS.md)** — Original CMS specification, feature overview, editor details, database schema

---

## License

All rights reserved. See [LICENSE](LICENSE) for details.
