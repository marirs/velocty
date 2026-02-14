<p align="center">
  <img src="website/static/images/logo-transparent.png" alt="Velocty" width="200">
</p>
<p align="center"><strong>CMS almost at the speed of light.</strong></p>

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

- Teams needing multi-user roles and permissions (Velocty is single-admin)
- Sites needing a plugin/extension ecosystem
- E-commerce stores with physical products, inventory, or shipping
- Sites requiring server-side rendering frameworks (React, Next.js, etc.)

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

- **Journal (Blog)** — Rich text posts with TinyMCE, categories, tags, excerpts, featured images
- **Portfolio** — Image gallery with masonry grid, lightbox, categories, tags, likes
- **Browse by tag** — `/tag/<slug>` routes for both blog and portfolio with pagination
- **Browse by category** — `/category/<slug>` routes for both blog and portfolio with pagination
- **Archives** — `/archives` page with posts grouped by year/month, drill-down to `/archives/<year>/<month>`
- **Dynamic URL slugs** — Blog and portfolio base URLs are configurable (e.g. `/journal`, `/gallery`) from settings
- **Comments** — Built-in commenting with honeypot spam protection, rate limiting, moderation queue
- **RSS Feed** — Auto-generated Atom/RSS feed
- **WordPress Import** — Import posts, portfolio items, categories, tags, and comments from WP XML export

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

- **7 payment providers** — PayPal, Stripe, Payoneer, 2Checkout, Square, Razorpay, Mollie
- **Sandbox/Live modes** per provider
- **Secure token-based downloads** with configurable expiry and download limits
- **Digital Download License** — customizable license agreement included with every purchase
- **License.txt generation** — per-purchase file with item name, buyer info, transaction ID, date

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
- **Keyboard shortcuts** — Cmd+S to save from any form
- **Flash notifications** — success/error toasts on save

### Security

- **Bcrypt password hashing**
- **Session-based auth** with secure cookies (SameSite=Strict, HttpOnly)
- **Configurable admin URL slug** — change `/admin` to anything for security through obscurity
- **Authentication modes:**
  - **Email & Password** — traditional login
  - **Magic Link** — passwordless login via email (requires email provider)
- **Optional TOTP MFA** — Google Authenticator, Authy, etc.
- **Login rate limiting** — in-memory IP-based enforcement, configurable attempts per 15 minutes
- **Comment rate limiting** — in-memory enforcement, configurable per 15-minute window
- **Login captcha** — reCAPTCHA v3, Cloudflare Turnstile, or hCaptcha
- **Anti-spam services** — Akismet, CleanTalk, OOPSpam
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

### Settings (13 sections)

| Section | What it controls |
|---|---|
| **Site** | Name, tagline, logo, favicon, URL, timezone, date format |
| **Journal** | Posts per page, display type, excerpt length, reading time |
| **Portfolio** | Grid columns, likes, lightbox, image protection, animations |
| **Comments** | Enable/disable, moderation mode, spam protection, rate limits |
| **Typography** | Fonts, sizes, sources, per-element assignment |
| **Images** | Upload path, max size, thumbnail dimensions, quality, WebP |
| **SEO** | Title template, meta defaults, sitemap, structured data, robots.txt, webmaster verification, 7 analytics providers |
| **Security** | Admin slug, auth method, MFA, sessions, rate limits, captcha, anti-spam |
| **Frontend** | Active design, back-to-top button |
| **Social** | Social media links with brand color icons |
| **Email** | 11 provider configurations |
| **Commerce** | 7 payment providers, currency, download limits, license template |
| **AI** | Provider chain, model selection, failover (Phase 4) |

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

Open `http://localhost:8000/admin/setup` to create your admin account.

### Multi-Site Mode

To serve multiple independent sites from a single binary:

```bash
cargo build --release --features multi-site
./target/release/velocty
```

Open `http://localhost:8000/super/setup` to create the super admin account, then add sites from the dashboard. See [MULTI-SITE.md](docs/MULTI-SITE.md) for full architecture details.

### Configuration

All configuration is done through the admin panel — no config files to edit. Settings are stored in SQLite and take effect immediately (except admin slug, which requires a restart).

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
│   ├── auth.rs                  # Login, sessions, bcrypt, MFA
│   ├── analytics.rs             # Page view logging fairing, GeoIP
│   ├── render.rs                # Design + content merge
│   ├── seo.rs                   # Meta tags, JSON-LD, sitemap
│   ├── rss.rs                   # RSS/Atom feed generation
│   ├── images.rs                # Upload, thumbnails, WebP conversion
│   ├── license.rs               # Purchase license.txt generation
│   ├── rate_limit.rs            # In-memory rate limiter (login, comments)
│   ├── site.rs                  # Multi-site: SiteContext, SitePoolManager, SiteResolver (feature-gated)
│   ├── models/                  # Data models (Post, Portfolio, Category, etc.)
│   └── routes/                  # Route handlers (admin, auth, public, API, super_admin)
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

### Phase 1 — Core (Current) ✅

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
- 13 settings sections with full configuration
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

### Phase 2 — Commerce

- Payment processing (PayPal, Stripe, etc.)
- Token-based secure downloads with expiry
- License file generation per purchase
- Buyer email notifications
- Sales dashboard in admin

### Phase 3 — Editors & Design Builder

- GrapesJS integration for drag-and-drop page layout design
- Design management: create, edit, duplicate, delete, activate, preview
- Custom components for content placeholders

### Phase 4 — AI

- Pluggable LLM connector with failover chain (Local → Ollama → OpenAI → Gemini → Cloudflare)
- SEO suggestions: meta title, description, tags, categories, alt text, slug
- Blog post generation from description
- Inline assist: expand, rewrite, summarise, continue

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
