# Velocty

**CMS almost at the speed of light.**

A purpose-built, blazing-fast CMS written in Rust. Focused on blogging and portfolio/photography — no bloat, no plugin ecosystem, just what you need.

**Website:** [velocty.io](https://velocty.io)

---

## Why

WordPress loads ~50+ PHP files per request, a full MySQL abstraction layer, a plugin/hook system, theme hierarchy, REST API, etc. — most of which goes unused. Velocty serves your exact use case with microsecond response times, a tiny footprint, and rock-solid security.

## Tech Stack

| Layer | Technology |
|---|---|
| **Backend** | Rust + Rocket |
| **Database** | SQLite |
| **Page layout builder** | GrapesJS (admin only) |
| **Content editor** | Editor.js (admin only) |
| **AI** | Ollama (local) / OpenAI / Anthropic (configurable) |
| **Frontend (visitors)** | Pure HTML/CSS + minimal vanilla JS (likes, comments, PayPal) |
| **SEO** | Built-in (no plugins) |

---

## Core Features

### 1. Blog

- Posts with block-based WYSIWYG editor (Editor.js)
- Categories, tags
- Comments (with honeypot spam protection, rate limiting)
- RSS feed
- Markdown support within text blocks

### 2. Portfolio

- Image upload with auto-thumbnail generation
- Categories, tags, description
- Likes (cookie/IP-based for anonymous visitors)
- Sell toggle: price, PayPal client-side checkout, token-based secure downloads
- License file generation per purchase

### 3. Commerce (7 Payment Providers)

- **PayPal** (JS SDK client-side), **Stripe** (Checkout redirect + HMAC-SHA256 webhook), **Razorpay** (JS modal + HMAC verify), **Mollie** (redirect + API fetch-back webhook), **Square** (redirect + HMAC-SHA256 webhook), **2Checkout** (redirect + MD5 IPN), **Payoneer** (redirect + webhook)
- Per-item payment provider selection — seller picks which processor per portfolio item
- Sandbox / Live mode toggle per provider
- Shared order pipeline: `create_pending_order` → provider checkout → `finalize_order`
- Secure token-based download links with configurable expiry and download count limits
- Optional download file per item (falls back to featured image if not set)
- License key auto-generation per purchase (XXXX-XXXX-XXXX-XXXX)
- Buyer email notifications via Gmail SMTP or custom SMTP
- Sales dashboard + Orders page in admin
- Price auto-format in editor (25 → 25.00)

### 4. Built-in SEO (No Plugins)

- Meta title / description fields on every post and portfolio item
- Auto-generated `sitemap.xml`
- Structured data (JSON-LD) for blog posts and portfolio items
- Open Graph / Twitter card meta tags
- Canonical URLs
- All configurable from settings

---

## Editors

### Content Editor — Editor.js

Block-based WYSIWYG for writing blog posts and portfolio descriptions. Each block is drag-and-drop reorderable.

**Block types:**

| Block | Description |
|---|---|
| **Text** | Rich text paragraph |
| **Heading** | H1–H4 |
| **Image** | Upload/select + caption + alt text |
| **Gallery** | Grid of images |
| **Quote** | Styled blockquote |
| **Code** | Syntax-highlighted code block |
| **Divider** | Horizontal rule |
| **Columns** | 2–3 column flex layout |
| **Embed** | YouTube, Vimeo, etc. |
| **HTML** | Raw HTML for power users |
| **AI** | "Describe what goes here" → AI fills the block |

- Output: structured JSON → Rust renders to HTML at serve time
- ~30KB, no framework dependency
- Admin-only — visitors never load it

### Page Layout Builder — GrapesJS

Wix-style drag-and-drop page/theme designer for creating site layouts.

- Drag elements onto a canvas — header, footer, sidebar, content area, grids
- Free positioning or section-based layout
- WYSIWYG — what you see is what gets saved
- Outputs clean HTML + CSS
- Save as reusable designs, switch active design from admin
- ~200KB, no React/Vue dependency
- Admin-only — visitors get pure static HTML/CSS

### How the Two Editors Coexist

```
GrapesJS  → Defines the page shell (header, footer, sidebar, where content goes)
Editor.js → Defines the actual content blocks within the layout

At render time:
  Layout (GrapesJS HTML/CSS) + Content (Editor.js JSON → HTML) = Final Page
```

---

## Design System

### Storage

```
designs/
├── design-001/
│   ├── meta.toml        # name, author, created_at
│   ├── layout.html      # saved HTML structure from GrapesJS
│   ├── style.css        # saved CSS from GrapesJS
│   └── thumbnail.png    # auto-generated preview
├── design-002/
│   └── ...
```

### Admin → Designer Page

- Thumbnail grid of all saved designs
- **Edit** — opens GrapesJS with saved HTML/CSS loaded
- **Set Active** — marks one design as the live site template
- **Duplicate** — clone a design to iterate on it
- **Delete** — remove unused designs

### AI Theme Generation

1. User describes what they want: *"Minimal dark portfolio with a full-width hero, masonry grid, and a sidebar blog"*
2. AI generates GrapesJS-compatible HTML + CSS using a predefined component library
3. Output loads into GrapesJS for drag-and-drop refinement
4. User tweaks, saves, activates

---

## AI Integration

### Configuration (Admin → Settings → AI)

```
AI Provider:       [Local (Ollama) ▼]  /  [OpenAI]  /  [Anthropic]
Endpoint:          [http://localhost:11434]
Model:             [llama3:8b ▼]
Auto-suggest:      [✓] Meta  [✓] Tags  [ ] Categories
Theme generation:  [Enabled]
Post generation:   [Enabled]
```

Supports:
- **Ollama** — local LLM (Llama 3, Mistral, Salesforce CodeGen/xGen, etc.)
- **OpenAI** — GPT-4o, GPT-4o-mini
- **Anthropic** — Claude

### AI Features

#### Content Suggestions (on save/publish)

| Field | AI Does |
|---|---|
| **Meta title** | Summarise post title for SEO (≤60 chars) |
| **Meta description** | Summarise content (≤160 chars) |
| **Tags** | Extract keywords from content |
| **Categories** | Suggest from existing categories |
| **Alt text** | Describe uploaded images |
| **Slug** | Generate clean URL-friendly slug |

User clicks **"AI Suggest"** → backend sends content to configured LLM → returns suggestions → user accepts or edits.

#### Blog Post Generation

1. User clicks **"New Post"** → **"Write with AI"**
2. Describes the post: *"Write about my recent trip to Kyoto, focusing on temples and street photography"*
3. AI generates a full draft as Editor.js blocks
4. Draft loads into the block editor — user rearranges, refines, publishes

#### Inline AI Assist

- **Expand** — select a paragraph, ask AI to elaborate
- **Rewrite** — select text, change tone/style
- **Summarise** — generate a TL;DR or excerpt
- **Continue** — AI continues writing from where you left off

#### Theme Generation

- Describe a layout in natural language → AI generates GrapesJS HTML/CSS
- Constrained to the component library (hero, grid, sidebar, navbar, footer, card, etc.)
- Loads into GrapesJS for visual refinement

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│  Admin Panel                                      │
│  ┌───────────┐ ┌───────────┐ ┌─────────────────┐ │
│  │ Content    │ │ Design    │ │ AI Assistant     │ │
│  │ Editor     │ │ Builder   │ │ - Theme gen      │ │
│  │ (Editor.js)│ │(GrapesJS) │ │ - Post gen       │ │
│  │            │ │           │ │ - SEO suggest    │ │
│  │            │ │           │ │ - Inline assist  │ │
│  └───────────┘ └───────────┘ └─────────────────┘ │
└────────────────────┬─────────────────────────────┘
                     │ saves JSON/HTML/CSS
            ┌────────▼────────┐
            │  Rust / Rocket   │
            │  + SQLite        │
            │  + LLM connector │──→ Local (Ollama)
            │    (pluggable)   │──→ Cloud (OpenAI / Anthropic)
            └────────┬────────┘
                     │ serves merged output
            ┌────────▼────────┐
            │  Pure HTML/CSS   │  ← microsecond responses
            │  + minimal JS    │     (likes, comments, PayPal)
            └─────────────────┘
```

---

## Admin Settings

| Section | Settings |
|---|---|
| **General** | Site name, tagline, logo, favicon |
| **Fonts** | Primary font, heading font (Google Fonts or local) |
| **Frontend** | Back-to-top button, cookie consent banner (style, position, theme), privacy policy page, terms of use page |
| **Images** | Storage folder path, max upload size, auto-thumbnail sizes |
| **PayPal** | Mode (sandbox/live), Client ID, seller email, currency |
| **Downloads** | Max downloads per purchase, link expiry hours |
| **SEO** | Default meta, sitemap settings, structured data |
| **AI** | Provider, endpoint, model, auto-suggest toggles |
| **Comments** | Enable/disable, moderation, spam protection |
| **Health** | System health dashboard — disk, DB stats, filesystem permissions, resource monitoring, maintenance tools (vacuum, orphan scan, export). Backend-aware (SQLite vs MongoDB) |

---

## Database Schema (SQLite)

```sql
-- Blog posts
CREATE TABLE posts (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    content_json TEXT NOT NULL,      -- Editor.js JSON
    content_html TEXT NOT NULL,      -- pre-rendered HTML
    excerpt TEXT,
    featured_image TEXT,
    meta_title TEXT,
    meta_description TEXT,
    status TEXT DEFAULT 'draft',     -- draft, published, archived
    published_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Portfolio items
CREATE TABLE portfolio (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    description_json TEXT,           -- Editor.js JSON
    description_html TEXT,
    image_path TEXT NOT NULL,
    thumbnail_path TEXT,
    meta_title TEXT,
    meta_description TEXT,
    sell_enabled INTEGER DEFAULT 0,
    price REAL,
    purchase_note TEXT DEFAULT '',   -- what buyer receives (shown before/after purchase)
    payment_provider TEXT DEFAULT '',-- selected provider for this item
    download_file_path TEXT DEFAULT '',-- optional file URL; falls back to featured image
    likes INTEGER DEFAULT 0,
    status TEXT DEFAULT 'draft',
    published_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Categories (shared)
CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    type TEXT NOT NULL               -- 'post' or 'portfolio'
);

-- Tags (shared)
CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL
);

-- Many-to-many: content ↔ categories
CREATE TABLE content_categories (
    content_id INTEGER NOT NULL,
    content_type TEXT NOT NULL,       -- 'post' or 'portfolio'
    category_id INTEGER NOT NULL,
    UNIQUE(content_id, content_type, category_id)
);

-- Many-to-many: content ↔ tags
CREATE TABLE content_tags (
    content_id INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    tag_id INTEGER NOT NULL,
    UNIQUE(content_id, content_type, tag_id)
);

-- Comments
CREATE TABLE comments (
    id INTEGER PRIMARY KEY,
    post_id INTEGER NOT NULL,
    author_name TEXT NOT NULL,
    author_email TEXT,
    body TEXT NOT NULL,
    status TEXT DEFAULT 'pending',   -- pending, approved, spam
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (post_id) REFERENCES posts(id)
);

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

-- Designs
CREATE TABLE designs (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    layout_html TEXT NOT NULL,
    style_css TEXT NOT NULL,
    thumbnail_path TEXT,
    is_active INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Settings (key-value)
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

---

## Build Phases

### Phase 1 — Core ✅
- Rocket project scaffold + SQLite schema (all tables)
- Blog: posts CRUD (TinyMCE), comments with honeypot + rate limiting, RSS feed (configurable count)
- Portfolio: upload with auto-thumbnails, categories, tags, heart/like (IP-based)
- Built-in SEO: meta fields, sitemap.xml, JSON-LD, OG/Twitter, canonical URLs, robots.txt
- Admin panel: Tera server-rendered forms, dashboard, dark/light themes, keyboard shortcuts
- Settings: general, blog, portfolio, comments, fonts, images, SEO, security, frontend, social, email, AI, commerce
- WordPress XML importer with drag-and-drop UI
- Cookie consent banner (GDPR-compliant, 3 styles, analytics gating)
- Privacy Policy & Terms of Use pages (TinyMCE editor, `/privacy`, `/terms`)
- Health dashboard with maintenance tools (vacuum, orphan scan, export, etc.)
- 11 email providers + magic link auth
- Login captcha + anti-spam services
- Multi-site support (optional feature flag)

### Phase 2 — Commerce & Auth ✅
- MFA flow: enable TOTP via QR code, verify codes, download recovery codes, disable MFA, login challenge page
- DB schema: orders, download_tokens, licenses tables + portfolio purchase_note, payment_provider, download_file_path columns
- Order/DownloadToken/License models with full CRUD and query helpers
- Portfolio editor: sell_enabled toggle, price input (auto-format 25→25.00), purchase_note textarea, payment provider dropdown (conditional), download file URL input
- Commerce settings UI: all 7 providers (PayPal, Stripe, Payoneer, 2Checkout, Square, Razorpay, Mollie) + downloads + license config
- Checkout routes for all 7 providers: PayPal (JS SDK create/capture), Stripe (Checkout session + HMAC-SHA256 webhook), Razorpay (order create + HMAC verify), Mollie (payment create + API webhook), Square (payment link + HMAC-SHA256 webhook), 2Checkout (hosted checkout + MD5 IPN), Payoneer (checkout link + webhook)
- Per-item payment provider selection: dropdown if >1 enabled, auto-assign if 1, warning if 0
- Sales sidebar menu (conditionally visible when any provider enabled) with Dashboard + Orders tabs
- Sales Dashboard: total/30d/7d revenue, order counts by status, recent orders table
- Orders page: filterable by status (all/completed/pending/refunded), paginated list
- Public portfolio: single buy button per item based on selected provider, email capture, purchase lookup for returning buyers
- Download page: token-based with license key display, download count tracking, expiry enforcement
- Optional download file path per item (falls back to featured image)
- Buyer email notifications: purchase receipt with download link + license key via SMTP (Gmail + custom SMTP via lettre)
- Zero `.unwrap()` calls in all commerce routes — safe error handling throughout

### Phase 3 — Editors
- Editor.js integration for blog/portfolio content (block-based WYSIWYG)
- GrapesJS integration for page/theme layout design
- Design management: save, list, activate, edit, duplicate

### Phase 4 — AI
- LLM connector: pluggable provider (Ollama local / OpenAI / Anthropic)
- AI content suggestions: meta, tags, categories, alt text
- AI blog post generation from description
- AI inline assist: expand, rewrite, summarise, continue
- AI theme generation → GrapesJS
- AI settings in admin

---

## What You Gain vs WordPress

| | **Velocty** | **WordPress** |
|---|---|---|
| **Response time** | Microseconds | 200–500ms |
| **Deployment** | Single binary + SQLite file | PHP + MySQL + Apache/Nginx |
| **Memory** | ~10–20MB | ~50–100MB |
| **Attack surface** | Minimal (no plugin ecosystem) | Huge (plugins, themes, XML-RPC) |
| **Codebase** | ~15–20 source files | Thousands |
| **Dependencies** | Rust crates (compiled in) | PHP extensions, plugins |
| **Updates** | Replace one binary | Core + plugin + theme updates |

---

## Next Steps

1. Share frontend screenshot → define first design template + component library
2. Scaffold Rocket project + SQLite
3. Build phase by phase
