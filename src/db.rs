use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn init_pool() -> Result<DbPool, Box<dyn std::error::Error>> {
    let manager = SqliteConnectionManager::file("website/db/velocty.db");
    let pool = Pool::builder().max_size(10).build(manager)?;

    // Enable WAL mode for better concurrent read performance
    let conn = pool.get()?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    Ok(pool)
}

pub fn run_migrations(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    conn.execute_batch(
        "
        -- Blog posts
        CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL,
            content_json TEXT NOT NULL DEFAULT '{}',
            content_html TEXT NOT NULL DEFAULT '',
            excerpt TEXT,
            featured_image TEXT,
            meta_title TEXT,
            meta_description TEXT,
            status TEXT NOT NULL DEFAULT 'draft',
            published_at DATETIME,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        -- Portfolio items
        CREATE TABLE IF NOT EXISTS portfolio (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL,
            description_json TEXT,
            description_html TEXT,
            image_path TEXT NOT NULL,
            thumbnail_path TEXT,
            meta_title TEXT,
            meta_description TEXT,
            sell_enabled INTEGER DEFAULT 0,
            price REAL,
            likes INTEGER DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'draft',
            published_at DATETIME,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        -- Categories (shared between posts and portfolio)
        CREATE TABLE IF NOT EXISTS categories (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL,
            type TEXT NOT NULL
        );

        -- Tags (shared)
        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT UNIQUE NOT NULL
        );

        -- Many-to-many: content <-> categories
        CREATE TABLE IF NOT EXISTS content_categories (
            content_id INTEGER NOT NULL,
            content_type TEXT NOT NULL,
            category_id INTEGER NOT NULL,
            UNIQUE(content_id, content_type, category_id)
        );

        -- Many-to-many: content <-> tags
        CREATE TABLE IF NOT EXISTS content_tags (
            content_id INTEGER NOT NULL,
            content_type TEXT NOT NULL,
            tag_id INTEGER NOT NULL,
            UNIQUE(content_id, content_type, tag_id)
        );

        -- Comments
        CREATE TABLE IF NOT EXISTS comments (
            id INTEGER PRIMARY KEY,
            post_id INTEGER NOT NULL,
            content_type TEXT NOT NULL DEFAULT 'post',
            author_name TEXT NOT NULL,
            author_email TEXT,
            body TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (post_id) REFERENCES posts(id)
        );

        -- Downloads / sales (Phase 2, table created now)
        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY,
            token TEXT UNIQUE NOT NULL,
            portfolio_id INTEGER NOT NULL,
            buyer_email TEXT NOT NULL,
            transaction_id TEXT NOT NULL,
            download_count INTEGER DEFAULT 0,
            max_downloads INTEGER DEFAULT 3,
            created_at DATETIME NOT NULL,
            expires_at DATETIME NOT NULL,
            FOREIGN KEY (portfolio_id) REFERENCES portfolio(id)
        );

        -- Designs
        CREATE TABLE IF NOT EXISTS designs (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            layout_html TEXT NOT NULL DEFAULT '',
            style_css TEXT NOT NULL DEFAULT '',
            thumbnail_path TEXT,
            is_active INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        -- Design templates (one per page type per design)
        CREATE TABLE IF NOT EXISTS design_templates (
            id INTEGER PRIMARY KEY,
            design_id INTEGER NOT NULL,
            template_type TEXT NOT NULL,
            layout_html TEXT NOT NULL,
            style_css TEXT NOT NULL DEFAULT '',
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (design_id) REFERENCES designs(id),
            UNIQUE(design_id, template_type)
        );

        -- Settings (key-value)
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        -- Import history
        CREATE TABLE IF NOT EXISTS imports (
            id INTEGER PRIMARY KEY,
            source TEXT NOT NULL,
            filename TEXT,
            posts_count INTEGER DEFAULT 0,
            portfolio_count INTEGER DEFAULT 0,
            comments_count INTEGER DEFAULT 0,
            skipped_count INTEGER DEFAULT 0,
            log TEXT,
            imported_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        -- Admin sessions
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            created_at DATETIME NOT NULL,
            expires_at DATETIME NOT NULL,
            ip_address TEXT,
            user_agent TEXT
        );

        -- Built-in analytics
        CREATE TABLE IF NOT EXISTS page_views (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL,
            ip_hash TEXT NOT NULL,
            country TEXT,
            city TEXT,
            referrer TEXT,
            user_agent TEXT,
            device_type TEXT,
            browser TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_views_path ON page_views(path);
        CREATE INDEX IF NOT EXISTS idx_views_date ON page_views(created_at);
        CREATE INDEX IF NOT EXISTS idx_views_country ON page_views(country);
        CREATE INDEX IF NOT EXISTS idx_views_referrer ON page_views(referrer);

        -- Likes tracking (IP-based)
        CREATE TABLE IF NOT EXISTS likes (
            id INTEGER PRIMARY KEY,
            portfolio_id INTEGER NOT NULL,
            ip_hash TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(portfolio_id, ip_hash),
            FOREIGN KEY (portfolio_id) REFERENCES portfolio(id)
        );
        ",
    )?;

    Ok(())
}

pub fn seed_defaults(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    let defaults = vec![
        // General
        ("site_name", "Velocty"),
        ("site_caption", ""),
        ("site_logo", ""),
        ("site_favicon", ""),
        ("site_url", "http://localhost:8000"),
        ("timezone", "UTC"),
        ("date_format", "%B %d, %Y"),
        ("admin_email", ""),
        ("admin_display_name", "Admin"),
        ("admin_bio", ""),
        ("admin_avatar", ""),
        // Security
        ("mfa_enabled", "false"),
        ("mfa_secret", ""),
        ("mfa_recovery_codes", "[]"),
        ("session_expiry_hours", "24"),
        ("login_rate_limit", "5"),
        // Journal
        ("blog_slug", "journal"),
        ("blog_posts_per_page", "10"),
        ("blog_display_type", "grid"),
        ("blog_grid_columns", "3"),
        ("blog_list_style", "compact"),
        ("blog_excerpt_words", "40"),
        ("blog_pagination_type", "classic"),
        ("blog_show_author", "true"),
        ("blog_show_date", "true"),
        ("blog_show_reading_time", "true"),
        ("blog_default_status", "draft"),
        ("blog_featured_image_required", "false"),
        // Portfolio
        ("portfolio_slug", "portfolio"),
        ("portfolio_display_type", "masonry"),
        ("portfolio_items_per_page", "12"),
        ("portfolio_grid_columns", "3"),
        ("portfolio_pagination_type", "classic"),
        ("portfolio_enable_likes", "true"),
        ("portfolio_heart_position", "image-bottom-right"),
        ("portfolio_image_protection", "false"),
        ("portfolio_featured_image_scale", "original"),
        ("portfolio_fade_animation", "true"),
        ("portfolio_show_categories", "true"),
        ("portfolio_show_tags", "true"),
        ("portfolio_click_mode", "lightbox"),
        ("portfolio_lightbox_border_color", "#D4A017"),
        ("portfolio_lightbox_title_color", "#FFFFFF"),
        ("portfolio_lightbox_tag_color", "#AAAAAA"),
        ("portfolio_lightbox_nav_color", "#FFFFFF"),
        ("portfolio_lightbox_show_title", "true"),
        ("portfolio_lightbox_show_tags", "true"),
        ("portfolio_lightbox_show_likes", "true"),
        ("portfolio_lightbox_nav", "true"),
        ("portfolio_lightbox_keyboard", "true"),
        // Comments
        ("comments_enabled", "true"),
        ("comments_on_blog", "true"),
        ("comments_on_portfolio", "false"),
        ("comments_moderation", "manual"),
        ("comments_honeypot", "true"),
        ("comments_rate_limit", "5"),
        ("comments_require_name", "true"),
        ("comments_require_email", "true"),
        // Fonts
        ("font_primary", "Inter"),
        ("font_heading", "Inter"),
        ("font_source", "google"),
        ("font_size_body", "16px"),
        ("font_size_h1", "2.5rem"),
        ("font_size_h2", "2rem"),
        ("font_size_h3", "1.75rem"),
        ("font_size_h4", "1.5rem"),
        ("font_size_h5", "1.25rem"),
        ("font_size_h6", "1rem"),
        ("font_google_enabled", "true"),
        ("font_google_custom", ""),
        ("font_adobe_enabled", "false"),
        ("font_adobe_project_id", ""),
        ("font_custom_name", ""),
        ("font_sitewide", "true"),
        ("font_body", ""),
        ("font_headings", ""),
        ("font_navigation", ""),
        ("font_buttons", ""),
        ("font_captions", ""),
        ("font_text_transform", "none"),
        // Images
        ("images_storage_path", "website/uploads/"),
        ("images_max_upload_mb", "10"),
        ("images_thumb_small", "150x150"),
        ("images_thumb_medium", "300x300"),
        ("images_thumb_large", "1024x1024"),
        ("images_quality", "85"),
        ("images_webp_convert", "true"),
        ("images_allowed_types", "jpg,jpeg,png,gif,webp,svg,tiff,heic"),
        // SEO
        ("seo_title_template", "{{title}} — {{site_name}}"),
        ("seo_default_description", ""),
        ("seo_sitemap_enabled", "true"),
        ("seo_structured_data", "true"),
        ("seo_open_graph", "true"),
        ("seo_twitter_cards", "true"),
        ("seo_canonical_base", ""),
        ("seo_robots_txt", "User-agent: *\nAllow: /"),
        // Design
        ("design_active_id", "1"),
        ("design_back_to_top", "true"),
        ("social_instagram", ""),
        ("social_twitter", ""),
        ("social_facebook", ""),
        ("social_youtube", ""),
        ("social_tiktok", ""),
        ("social_linkedin", ""),
        ("social_pinterest", ""),
        ("social_behance", ""),
        ("social_dribbble", ""),
        ("social_github", ""),
        ("social_vimeo", ""),
        ("social_500px", ""),
        ("social_brand_colors", "true"),
        // PayPal (Phase 2 — defaults ready)
        ("paypal_mode", "sandbox"),
        ("paypal_client_id", ""),
        ("paypal_secret", ""),
        ("paypal_license_text", "DIGITAL DOWNLOAD LICENSE AGREEMENT\n\nBy purchasing and downloading digital content from this website, you agree to the following terms:\n\n1. GRANT OF LICENSE\nUpon completed payment, the Seller grants the Buyer a non-exclusive, non-transferable, revocable license to download and use the purchased digital file(s) for personal, non-commercial purposes only.\n\n2. PERMITTED USE\n- Personal use (e.g., desktop wallpaper, personal prints, personal social media with credit)\n- One (1) personal print per purchased image\n\n3. RESTRICTIONS\nThe Buyer may NOT:\n- Resell, redistribute, sublicense, or share the file(s) with third parties\n- Use the file(s) for commercial purposes without a separate commercial license\n- Claim ownership or authorship of the file(s)\n- Use the file(s) in any defamatory, illegal, or misleading context\n- Remove or alter any embedded metadata or watermarks\n\n4. INTELLECTUAL PROPERTY\nAll intellectual property rights in the digital content remain with the Seller. This license does not transfer ownership of the content.\n\n5. DELIVERY & REFUNDS\nDigital files are delivered electronically. Due to the nature of digital goods, all sales are final. No refunds will be issued once the download link has been accessed.\n\n6. DOWNLOAD LIMITS\nEach purchase includes a limited number of downloads within a specified time period. Expired or exhausted download links will not be renewed without a new purchase.\n\n7. LIABILITY\nThe digital content is provided \"as is\" without warranty of any kind. The Seller is not liable for any damages arising from the use of the purchased content.\n\n8. TERMINATION\nThis license is effective until terminated. The Seller may terminate this license at any time if the Buyer breaches any of these terms. Upon termination, the Buyer must destroy all copies of the downloaded content.\n\nBy completing your purchase, you acknowledge that you have read, understood, and agree to be bound by these terms."),
        ("paypal_email", ""),
        ("paypal_currency", "USD"),
        ("paypal_button_color", "gold"),
        ("downloads_max_per_purchase", "3"),
        ("downloads_expiry_hours", "48"),
        ("downloads_license_template", "License granted for personal use."),
        // AI (Phase 4 — defaults ready)
        ("ai_failover_chain", "local,ollama,openai,gemini,cloudflare"),
        ("ai_local_enabled", "false"),
        ("ai_local_model", "smollm2-1.7b"),
        ("ai_local_model_downloaded", "false"),
        ("ai_ollama_enabled", "false"),
        ("ai_ollama_url", "http://localhost:11434"),
        ("ai_ollama_model", ""),
        ("ai_openai_enabled", "false"),
        ("ai_openai_api_key", ""),
        ("ai_openai_model", "gpt-4"),
        ("ai_openai_base_url", ""),
        ("ai_gemini_enabled", "false"),
        ("ai_gemini_api_key", ""),
        ("ai_gemini_model", "gemini-pro"),
        ("ai_cloudflare_enabled", "false"),
        ("ai_cloudflare_account_id", ""),
        ("ai_cloudflare_api_token", ""),
        ("ai_cloudflare_model", "@cf/meta/llama-3-8b-instruct"),
        ("ai_suggest_meta", "true"),
        ("ai_suggest_tags", "true"),
        ("ai_suggest_categories", "false"),
        ("ai_suggest_alt_text", "true"),
        ("ai_suggest_slug", "true"),
        ("ai_theme_generation", "true"),
        ("ai_post_generation", "true"),
        ("ai_temperature", "0.7"),
    ];

    for (key, value) in defaults {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }

    // Seed default design if none exists
    let design_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM designs", [], |row| row.get(0))?;

    if design_count == 0 {
        conn.execute(
            "INSERT INTO designs (name, layout_html, style_css, is_active) VALUES (?1, ?2, ?3, 1)",
            params!["Default", "", ""],
        )?;
    }

    // Seed admin password if not set
    let admin_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM settings WHERE key = 'admin_password_hash'",
        [],
        |row| row.get(0),
    )?;

    if admin_exists == 0 {
        // Default password: "admin" — user MUST change on first login
        let hash = bcrypt::hash("admin", bcrypt::DEFAULT_COST)
            .expect("Failed to hash default password");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('admin_password_hash', ?1)",
            params![hash],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('admin_setup_complete', 'false')",
            params![],
        )?;
    }

    Ok(())
}
