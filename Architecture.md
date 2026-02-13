# Velocty — Architecture & Specification

**CMS almost at the speed of light.**

This document expands on `README-CMS.md` with detailed architecture decisions, settings structure, authentication, the design/render system, AI integration, and import pipeline.

---

## Tech Stack

| Layer | Technology |
|---|---|
| **Backend** | Rust + Rocket |
| **Database** | SQLite (via rusqlite) |
| **Templates (admin)** | Tera (Rocket's built-in template engine) |
| **Page layout builder** | GrapesJS (admin only, ~200KB) |
| **Content editor** | Editor.js (admin only, ~30KB) |
| **AI** | Ollama (local) / OpenAI / Anthropic (pluggable) |
| **Frontend (visitors)** | Pure HTML/CSS + minimal vanilla JS |
| **Auth** | Session-based + optional MFA (TOTP) |

---

## Authentication & Security

### Admin Login

- Single admin user (configured on first run / setup wizard)
- Session-based auth with secure cookies (`SameSite=Strict`, `HttpOnly`, `Secure`)
- Bcrypt password hashing
- Login rate limiting (max 5 attempts per 15 minutes per IP)
- Session expiry (configurable, default 24h)

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
| `blog_list_style` | compact / editorial (when list) | "compact" |
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

### Images

| Key | Description | Default |
|---|---|---|
| `images_storage_path` | Upload directory | "uploads/" |
| `images_max_upload_mb` | Max upload size in MB | "10" |
| `images_thumb_small` | Small thumbnail dimensions | "150x150" |
| `images_thumb_medium` | Medium thumbnail dimensions | "300x300" |
| `images_thumb_large` | Large thumbnail dimensions | "1024x1024" |
| `images_quality` | JPEG/WebP quality (1-100) | "85" |
| `images_webp_convert` | Auto-convert to WebP | "true" |

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

### Design

| Key | Description | Default |
|---|---|---|
| `design_active_id` | Active design ID | "default" |
| `design_back_to_top` | Back-to-top button | "true" |
| `social_links` | Social media links (JSON) | "[]" |
| `social_brand_colors` | Use brand colors for icons | "true" |

### PayPal / Commerce (Phase 2)

| Key | Description | Default |
|---|---|---|
| `paypal_mode` | sandbox / live | "sandbox" |
| `paypal_client_id` | PayPal REST API Client ID | "" |
| `paypal_email` | Seller PayPal email | "" |
| `paypal_currency` | Currency code | "USD" |
| `paypal_button_color` | gold / blue / silver / white / black | "gold" |
| `downloads_max_per_purchase` | Max downloads per token | "3" |
| `downloads_expiry_hours` | Link expiry in hours | "48" |
| `downloads_license_template` | License text template | (default license text) |

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
```

---

## Project Structure (Phase 1)

```
velocty/
├── Cargo.toml
├── README-CMS.md
├── Architecture.md
├── src/
│   ├── main.rs                      # Rocket launch, DB init, route mounting
│   ├── db.rs                        # SQLite connection pool, migrations
│   ├── auth.rs                      # Login, sessions, MFA (TOTP)
│   ├── models/
│   │   ├── mod.rs
│   │   ├── post.rs                  # Post struct, CRUD
│   │   ├── portfolio.rs             # Portfolio struct, CRUD
│   │   ├── category.rs              # Category struct, CRUD
│   │   ├── tag.rs                   # Tag struct, CRUD
│   │   ├── comment.rs               # Comment struct, CRUD
│   │   ├── design.rs                # Design struct, CRUD
│   │   ├── settings.rs              # Settings get/set helpers
│   │   └── import.rs                # Import history
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── public.rs                # Visitor-facing routes (blog, portfolio, RSS, sitemap)
│   │   ├── admin.rs                 # Admin panel routes (dashboard, CRUD forms)
│   │   ├── api.rs                   # JSON API endpoints (likes, comments, search)
│   │   └── auth.rs                  # Login/logout/MFA routes
│   ├── render.rs                    # Design + content merge, placeholder replacement
│   ├── seo.rs                       # Meta tags, JSON-LD, OG, sitemap generation
│   ├── rss.rs                       # RSS feed generation
│   ├── images.rs                    # Upload handling, thumbnail generation, WebP
│   └── import/
│       ├── mod.rs
│       └── wordpress.rs             # WP XML parser (Phase 1)
├── templates/                       # Tera templates (admin panel only)
│   ├── admin/
│   │   ├── base.html                # Admin layout shell
│   │   ├── login.html
│   │   ├── dashboard.html
│   │   ├── posts/
│   │   │   ├── list.html
│   │   │   └── edit.html
│   │   ├── portfolio/
│   │   │   ├── list.html
│   │   │   └── edit.html
│   │   ├── comments/
│   │   │   └── list.html
│   │   ├── settings/
│   │   │   ├── general.html
│   │   │   ├── blog.html
│   │   │   ├── portfolio.html
│   │   │   ├── comments.html
│   │   │   ├── fonts.html
│   │   │   ├── images.html
│   │   │   ├── seo.html
│   │   │   ├── security.html
│   │   │   └── design.html
│   │   └── import/
│   │       └── index.html
│   └── default_design/             # Hardcoded default design (Phase 1)
│       ├── homepage.html
│       ├── blog_list.html
│       ├── blog_single.html
│       ├── portfolio_grid.html
│       ├── portfolio_single.html
│       ├── page.html
│       └── 404.html
├── static/                          # Static assets for admin
│   ├── css/
│   │   └── admin.css
│   └── js/
│       └── admin.js
├── uploads/                         # User uploads (images, files)
├── designs/                         # Saved GrapesJS designs (Phase 3)
└── velocty.db                       # SQLite database (created at runtime)
```

---

## Build Phases (Updated)

### Phase 1 — Core
- Rocket project scaffold + SQLite schema (all tables)
- Auth: admin login with bcrypt + sessions + optional MFA (TOTP)
- Blog: posts CRUD (Markdown/plain), comments with honeypot + rate limiting, RSS feed
- Portfolio: upload with auto-thumbnails, categories, tags, heart/like (IP-based)
- SEO: meta fields, sitemap.xml, JSON-LD, OG/Twitter tags, canonical URLs
- Admin panel: Tera server-rendered forms, dashboard
- Settings: general, blog, portfolio, comments, fonts, images, SEO, security, design
- Default hardcoded design (clean minimalist layout)
- WordPress XML importer (basic)

### Phase 2 — Commerce
- PayPal JS SDK checkout on portfolio items
- Token-based secure downloads with expiry and count limits
- License file generation per purchase
- Buyer email notifications (SMTP config in settings)
- Sales dashboard in admin
- PayPal / Commerce settings

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
