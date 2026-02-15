use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn init_pool() -> Result<DbPool, Box<dyn std::error::Error>> {
    init_pool_at("website/site/db/velocty.db").map_err(|e| e.into())
}

pub fn init_pool_at(path: &str) -> Result<DbPool, String> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let manager = SqliteConnectionManager::file(path);
    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .map_err(|e| e.to_string())?;

    // Enable WAL mode for better concurrent read performance
    let conn = pool.get().map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string())?;

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
            purchase_note TEXT DEFAULT '',
            payment_provider TEXT DEFAULT '',
            download_file_path TEXT DEFAULT '',
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

        -- Orders
        CREATE TABLE IF NOT EXISTS orders (
            id INTEGER PRIMARY KEY,
            portfolio_id INTEGER NOT NULL,
            buyer_email TEXT NOT NULL,
            buyer_name TEXT DEFAULT '',
            amount REAL NOT NULL,
            currency TEXT NOT NULL DEFAULT 'USD',
            provider TEXT NOT NULL,
            provider_order_id TEXT DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (portfolio_id) REFERENCES portfolio(id)
        );

        -- Download tokens (one per order)
        CREATE TABLE IF NOT EXISTS download_tokens (
            id INTEGER PRIMARY KEY,
            order_id INTEGER NOT NULL,
            token TEXT UNIQUE NOT NULL,
            downloads_used INTEGER DEFAULT 0,
            max_downloads INTEGER DEFAULT 3,
            expires_at DATETIME NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (order_id) REFERENCES orders(id)
        );

        -- Licenses (one per order)
        CREATE TABLE IF NOT EXISTS licenses (
            id INTEGER PRIMARY KEY,
            order_id INTEGER NOT NULL,
            license_key TEXT UNIQUE NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (order_id) REFERENCES orders(id)
        );

        CREATE INDEX IF NOT EXISTS idx_orders_portfolio ON orders(portfolio_id);
        CREATE INDEX IF NOT EXISTS idx_orders_email ON orders(buyer_email);
        CREATE INDEX IF NOT EXISTS idx_download_tokens_token ON download_tokens(token);
        CREATE INDEX IF NOT EXISTS idx_download_tokens_order ON download_tokens(order_id);

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

        -- Magic link tokens
        CREATE TABLE IF NOT EXISTS magic_links (
            id INTEGER PRIMARY KEY,
            token TEXT UNIQUE NOT NULL,
            email TEXT NOT NULL,
            created_at DATETIME NOT NULL,
            expires_at DATETIME NOT NULL,
            used INTEGER NOT NULL DEFAULT 0
        );

        -- Likes tracking (IP-based)
        CREATE TABLE IF NOT EXISTS likes (
            id INTEGER PRIMARY KEY,
            portfolio_id INTEGER NOT NULL,
            ip_hash TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(portfolio_id, ip_hash),
            FOREIGN KEY (portfolio_id) REFERENCES portfolio(id)
        );

        -- Users
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            email TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            display_name TEXT NOT NULL DEFAULT '',
            role TEXT NOT NULL DEFAULT 'subscriber',
            status TEXT NOT NULL DEFAULT 'active',
            avatar TEXT DEFAULT '',
            mfa_enabled INTEGER NOT NULL DEFAULT 0,
            mfa_secret TEXT DEFAULT '',
            mfa_recovery_codes TEXT DEFAULT '[]',
            last_login_at DATETIME,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
        CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

        -- Firewall: ban list
        CREATE TABLE IF NOT EXISTS fw_bans (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ip TEXT NOT NULL,
            reason TEXT NOT NULL,
            detail TEXT,
            banned_at DATETIME NOT NULL DEFAULT (datetime('now')),
            expires_at DATETIME,
            country TEXT,
            user_agent TEXT,
            active INTEGER NOT NULL DEFAULT 1
        );
        CREATE INDEX IF NOT EXISTS idx_fw_bans_ip ON fw_bans(ip);
        CREATE INDEX IF NOT EXISTS idx_fw_bans_active ON fw_bans(active);

        -- Firewall: event log
        CREATE TABLE IF NOT EXISTS fw_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ip TEXT NOT NULL,
            event_type TEXT NOT NULL,
            detail TEXT,
            country TEXT,
            user_agent TEXT,
            request_path TEXT,
            created_at DATETIME NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_fw_events_ip ON fw_events(ip);
        CREATE INDEX IF NOT EXISTS idx_fw_events_type ON fw_events(event_type);
        CREATE INDEX IF NOT EXISTS idx_fw_events_created ON fw_events(created_at);

        -- Audit log
        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER,
            user_name TEXT,
            action TEXT NOT NULL,
            entity_type TEXT,
            entity_id INTEGER,
            entity_title TEXT,
            details TEXT,
            ip_address TEXT,
            created_at DATETIME NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_audit_log_user ON audit_log(user_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
        CREATE INDEX IF NOT EXISTS idx_audit_log_entity ON audit_log(entity_type);
        CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);
        ",
    )?;

    // ── Schema migrations for existing databases ──────────────
    // Add purchase_note to portfolio if missing
    let has_purchase_note: bool = conn
        .prepare("SELECT purchase_note FROM portfolio LIMIT 0")
        .is_ok();
    if !has_purchase_note {
        conn.execute_batch("ALTER TABLE portfolio ADD COLUMN purchase_note TEXT DEFAULT '';")?;
    }

    // Add payment_provider to portfolio if missing
    let has_payment_provider: bool = conn
        .prepare("SELECT payment_provider FROM portfolio LIMIT 0")
        .is_ok();
    if !has_payment_provider {
        conn.execute_batch("ALTER TABLE portfolio ADD COLUMN payment_provider TEXT DEFAULT '';")?;
    }

    // Add download_file_path to portfolio if missing
    let has_download_file_path: bool = conn
        .prepare("SELECT download_file_path FROM portfolio LIMIT 0")
        .is_ok();
    if !has_download_file_path {
        conn.execute_batch("ALTER TABLE portfolio ADD COLUMN download_file_path TEXT DEFAULT '';")?;
    }

    // Drop legacy downloads table and replace with orders + download_tokens + licenses
    let has_old_downloads: bool = conn
        .prepare("SELECT transaction_id FROM downloads LIMIT 0")
        .is_ok();
    if has_old_downloads {
        conn.execute_batch("DROP TABLE IF EXISTS downloads;")?;
    }

    // Add user_id to sessions if missing
    let has_session_user_id: bool = conn
        .prepare("SELECT user_id FROM sessions LIMIT 0")
        .is_ok();
    if !has_session_user_id {
        conn.execute_batch("ALTER TABLE sessions ADD COLUMN user_id INTEGER DEFAULT NULL;")?;
    }

    // Add user_id to posts if missing
    let has_post_user_id: bool = conn
        .prepare("SELECT user_id FROM posts LIMIT 0")
        .is_ok();
    if !has_post_user_id {
        conn.execute_batch("ALTER TABLE posts ADD COLUMN user_id INTEGER DEFAULT NULL;")?;
    }

    // Add user_id to portfolio if missing
    let has_portfolio_user_id: bool = conn
        .prepare("SELECT user_id FROM portfolio LIMIT 0")
        .is_ok();
    if !has_portfolio_user_id {
        conn.execute_batch("ALTER TABLE portfolio ADD COLUMN user_id INTEGER DEFAULT NULL;")?;
    }

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
        ("rss_feed_count", "25"),
        ("admin_email", ""),
        ("admin_display_name", "Admin"),
        ("admin_theme", "dark"),
        ("admin_bio", ""),
        ("admin_avatar", ""),
        // Security
        ("admin_slug", "admin"),
        ("login_method", "password"),
        ("mfa_enabled", "false"),
        ("mfa_secret", ""),
        ("mfa_recovery_codes", "[]"),
        ("session_expiry_hours", "24"),
        ("login_rate_limit", "5"),
        ("login_captcha_enabled", "false"),
        ("login_captcha_provider", ""),
        // Anti-spam / Captcha services
        ("security_akismet_enabled", "false"),
        ("security_akismet_api_key", ""),
        ("security_cleantalk_enabled", "false"),
        ("security_cleantalk_api_key", ""),
        ("security_oopspam_enabled", "false"),
        ("security_oopspam_api_key", ""),
        ("security_recaptcha_enabled", "false"),
        ("security_recaptcha_site_key", ""),
        ("security_recaptcha_secret_key", ""),
        ("security_recaptcha_version", "v3"),
        ("security_turnstile_enabled", "false"),
        ("security_turnstile_site_key", ""),
        ("security_turnstile_secret_key", ""),
        ("security_hcaptcha_enabled", "false"),
        ("security_hcaptcha_site_key", ""),
        ("security_hcaptcha_secret_key", ""),
        // Visitors (Design)
        ("design_site_search", "true"),
        ("design_back_to_top", "false"),
        ("cookie_consent_enabled", "false"),
        ("cookie_consent_style", "minimal"),
        ("cookie_consent_position", "bottom"),
        ("cookie_consent_policy_url", "/privacy"),
        ("cookie_consent_theme", "auto"),
        ("cookie_consent_show_reject", "false"),
        ("privacy_policy_enabled", "false"),
        ("privacy_policy_content", ""),
        ("terms_of_use_enabled", "false"),
        ("terms_of_use_content", ""),
        // Journal
        ("journal_enabled", "true"),
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
        ("portfolio_enabled", "false"),
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
        ("images_storage_path", "website/site/uploads/"),
        ("images_max_upload_mb", "10"),
        ("images_thumb_small", "150x150"),
        ("images_thumb_medium", "300x300"),
        ("images_thumb_large", "1024x1024"),
        ("images_quality", "85"),
        ("images_webp_convert", "true"),
        ("images_allowed_types", "jpg,jpeg,png,gif,webp,svg,tiff,heic"),
        // Video
        ("video_upload_enabled", "false"),
        ("video_max_upload_mb", "100"),
        ("video_allowed_types", "mp4,webm,mov,avi,mkv"),
        ("video_max_duration", "0"),
        ("video_generate_thumbnail", "true"),
        // Media Organization
        ("media_organization", "flat"),
        // SEO
        ("seo_title_template", "{{title}} — {{site_name}}"),
        ("seo_default_description", ""),
        ("seo_sitemap_enabled", "true"),
        ("seo_structured_data", "true"),
        ("seo_open_graph", "true"),
        ("seo_twitter_cards", "true"),
        ("seo_canonical_base", ""),
        ("seo_robots_txt", "User-agent: *\nAllow: /"),
        // SEO — Webmaster verification
        ("seo_google_verification", ""),
        ("seo_bing_verification", ""),
        ("seo_yandex_verification", ""),
        ("seo_pinterest_verification", ""),
        ("seo_baidu_verification", ""),
        // SEO — Google Analytics
        ("seo_ga_enabled", "false"),
        ("seo_ga_measurement_id", ""),
        // SEO — Plausible
        ("seo_plausible_enabled", "false"),
        ("seo_plausible_domain", ""),
        ("seo_plausible_host", "https://plausible.io"),
        // SEO — Fathom
        ("seo_fathom_enabled", "false"),
        ("seo_fathom_site_id", ""),
        // SEO — Matomo
        ("seo_matomo_enabled", "false"),
        ("seo_matomo_url", ""),
        ("seo_matomo_site_id", "1"),
        // SEO — Cloudflare Web Analytics
        ("seo_cloudflare_analytics_enabled", "false"),
        ("seo_cloudflare_analytics_token", ""),
        // SEO — Clicky
        ("seo_clicky_enabled", "false"),
        ("seo_clicky_site_id", ""),
        // SEO — Umami
        ("seo_umami_enabled", "false"),
        ("seo_umami_website_id", ""),
        ("seo_umami_host", "https://analytics.umami.is"),
        // Frontend
        ("design_back_to_top", "true"),
        ("cookie_consent_enabled", "false"),
        ("cookie_consent_style", "minimal"),
        ("cookie_consent_position", "bottom"),
        ("cookie_consent_policy_url", "/privacy"),
        ("cookie_consent_show_reject", "true"),
        ("cookie_consent_theme", "auto"),
        // Privacy Policy
        ("privacy_policy_enabled", "false"),
        ("privacy_policy_content", "<h1>Privacy Policy</h1><p><strong>Last updated:</strong> [Date]</p><h2>1. Introduction</h2><p>Welcome to [Site Name] (&ldquo;we&rdquo;, &ldquo;us&rdquo;, or &ldquo;our&rdquo;). We respect your privacy and are committed to protecting your personal data. This privacy policy explains how we collect, use, and safeguard your information when you visit our website.</p><h2>2. Information We Collect</h2><h3>Information you provide</h3><ul><li><strong>Contact information</strong> &mdash; name and email address when you submit comments or contact forms</li><li><strong>Account information</strong> &mdash; if you create an account or make a purchase</li></ul><h3>Information collected automatically</h3><ul><li><strong>Usage data</strong> &mdash; pages visited, time spent, referral source, browser type, device type</li><li><strong>IP address</strong> &mdash; anonymized/hashed for analytics purposes</li><li><strong>Cookies</strong> &mdash; see our Cookie Policy section below</li></ul><h2>3. How We Use Your Information</h2><p>We use your information to:</p><ul><li>Provide and maintain our website</li><li>Respond to your comments and inquiries</li><li>Analyze website usage to improve our content and user experience</li><li>Process purchases and deliver digital products</li><li>Send notifications related to your purchases</li></ul><h2>4. Cookies</h2><p>We use cookies and similar technologies to:</p><ul><li>Remember your preferences</li><li>Understand how you use our website</li><li>Improve your browsing experience</li></ul><p><strong>Essential cookies</strong> are required for the website to function and cannot be disabled.</p><p><strong>Analytics cookies</strong> help us understand how visitors interact with our website. These are only set if you consent.</p><p>You can manage your cookie preferences at any time using the cookie consent banner.</p><h2>5. Third-Party Services</h2><p>We may use third-party services for:</p><ul><li><strong>Analytics</strong> &mdash; to understand website traffic (data is anonymized)</li><li><strong>Payment processing</strong> &mdash; to handle purchases securely</li><li><strong>Content delivery</strong> &mdash; to serve fonts and assets efficiently</li></ul><p>These services have their own privacy policies governing their use of your data.</p><h2>6. Data Retention</h2><p>We retain your personal data only for as long as necessary to fulfill the purposes outlined in this policy. Analytics data is periodically pruned.</p><h2>7. Your Rights</h2><p>Depending on your location, you may have the right to:</p><ul><li>Access the personal data we hold about you</li><li>Request correction of inaccurate data</li><li>Request deletion of your data</li><li>Object to or restrict processing of your data</li><li>Data portability</li></ul><p>To exercise any of these rights, please contact us at [email].</p><h2>8. Data Security</h2><p>We implement appropriate technical and organizational measures to protect your personal data against unauthorized access, alteration, disclosure, or destruction.</p><h2>9. Children&rsquo;s Privacy</h2><p>Our website is not intended for children under 13. We do not knowingly collect personal data from children.</p><h2>10. Changes to This Policy</h2><p>We may update this privacy policy from time to time. We will notify you of any changes by posting the new policy on this page and updating the &ldquo;Last updated&rdquo; date.</p><h2>11. Contact Us</h2><p>If you have any questions about this privacy policy, please contact us at [email].</p>"),
        // Terms of Use
        ("terms_of_use_enabled", "false"),
        ("terms_of_use_content", "<h1>Terms of Use</h1><p><strong>Last updated:</strong> [Date]</p><h2>1. Acceptance of Terms</h2><p>By accessing and using [Site Name] (&ldquo;the Website&rdquo;), you accept and agree to be bound by these Terms of Use. If you do not agree to these terms, please do not use the Website.</p><h2>2. Use of the Website</h2><p>You may use the Website for lawful purposes only. You agree not to:</p><ul><li>Use the Website in any way that violates applicable laws or regulations</li><li>Attempt to gain unauthorized access to any part of the Website</li><li>Interfere with or disrupt the Website or its servers</li><li>Scrape, crawl, or use automated tools to extract content without permission</li><li>Upload or transmit viruses or malicious code</li></ul><h2>3. Intellectual Property</h2><p>All content on the Website &mdash; including text, images, photographs, graphics, logos, and software &mdash; is the property of [Site Name] or its content creators and is protected by copyright and intellectual property laws.</p><p>You may not reproduce, distribute, modify, or create derivative works from any content without explicit written permission.</p><h2>4. User-Generated Content</h2><p>If you submit comments or other content to the Website:</p><ul><li>You retain ownership of your content</li><li>You grant us a non-exclusive, royalty-free license to display your content on the Website</li><li>You are responsible for ensuring your content does not violate any third-party rights</li><li>We reserve the right to remove any content at our discretion</li></ul><h2>5. Digital Purchases</h2><p>If you purchase digital products from the Website:</p><ul><li>All sales are final due to the nature of digital goods</li><li>You receive a limited, non-transferable license to use the purchased content</li><li>Specific license terms are provided with each purchase</li><li>Download links are subject to expiration and download limits</li></ul><h2>6. Disclaimer of Warranties</h2><p>The Website is provided &ldquo;as is&rdquo; and &ldquo;as available&rdquo; without warranties of any kind, either express or implied. We do not warrant that:</p><ul><li>The Website will be uninterrupted or error-free</li><li>The content is accurate, complete, or current</li><li>The Website is free of viruses or harmful components</li></ul><h2>7. Limitation of Liability</h2><p>To the fullest extent permitted by law, [Site Name] shall not be liable for any indirect, incidental, special, consequential, or punitive damages arising from your use of the Website.</p><h2>8. Links to Third-Party Websites</h2><p>The Website may contain links to third-party websites. We are not responsible for the content or practices of these external sites.</p><h2>9. Modifications</h2><p>We reserve the right to modify these Terms of Use at any time. Changes will be effective immediately upon posting. Your continued use of the Website after changes constitutes acceptance of the modified terms.</p><h2>10. Governing Law</h2><p>These Terms of Use shall be governed by and construed in accordance with the laws of [Jurisdiction], without regard to conflict of law principles.</p><h2>11. Contact Us</h2><p>If you have any questions about these Terms of Use, please contact us at [email].</p>"),
        // Design
        ("design_active_id", "1"),
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
        ("share_enabled", "false"),
        ("share_facebook", "true"),
        ("share_x", "true"),
        ("share_linkedin", "true"),
        // Commerce (Phase 2 — defaults ready)
        ("commerce_paypal_enabled", "false"),
        ("paypal_mode", "sandbox"),
        ("paypal_client_id", ""),
        ("paypal_secret", ""),
        ("paypal_license_text", "DIGITAL DOWNLOAD LICENSE AGREEMENT\n\nBy purchasing and downloading digital content from this website, you agree to the following terms:\n\n1. GRANT OF LICENSE\nUpon completed payment, the Seller grants the Buyer a non-exclusive, non-transferable, revocable license to download and use the purchased digital file(s) for personal, non-commercial purposes only.\n\n2. PERMITTED USE\n- Personal use (e.g., desktop wallpaper, personal prints, personal social media with credit)\n- One (1) personal print per purchased image\n\n3. RESTRICTIONS\nThe Buyer may NOT:\n- Resell, redistribute, sublicense, or share the file(s) with third parties\n- Use the file(s) for commercial purposes without a separate commercial license\n- Claim ownership or authorship of the file(s)\n- Use the file(s) in any defamatory, illegal, or misleading context\n- Remove or alter any embedded metadata or watermarks\n\n4. INTELLECTUAL PROPERTY\nAll intellectual property rights in the digital content remain with the Seller. This license does not transfer ownership of the content.\n\n5. DELIVERY & REFUNDS\nDigital files are delivered electronically. Due to the nature of digital goods, all sales are final. No refunds will be issued once the download link has been accessed.\n\n6. DOWNLOAD LIMITS\nEach purchase includes a limited number of downloads within a specified time period. Expired or exhausted download links will not be renewed without a new purchase.\n\n7. LIABILITY\nThe digital content is provided \"as is\" without warranty of any kind. The Seller is not liable for any damages arising from the use of the purchased content.\n\n8. TERMINATION\nThis license is effective until terminated. The Seller may terminate this license at any time if the Buyer breaches any of these terms. Upon termination, the Buyer must destroy all copies of the downloaded content.\n\nBy completing your purchase, you acknowledge that you have read, understood, and agree to be bound by these terms."),
        ("paypal_email", ""),
        ("paypal_currency", "USD"),
        ("paypal_button_color", "gold"),
        ("commerce_payoneer_enabled", "false"),
        ("payoneer_client_id", ""),
        ("payoneer_client_secret", ""),
        ("payoneer_mode", "sandbox"),
        ("payoneer_program_id", ""),
        ("commerce_stripe_enabled", "false"),
        ("stripe_mode", "test"),
        ("stripe_publishable_key", ""),
        ("stripe_secret_key", ""),
        ("stripe_webhook_secret", ""),
        ("commerce_2checkout_enabled", "false"),
        ("twocheckout_merchant_code", ""),
        ("twocheckout_secret_key", ""),
        ("twocheckout_mode", "sandbox"),
        ("commerce_square_enabled", "false"),
        ("square_application_id", ""),
        ("square_access_token", ""),
        ("square_location_id", ""),
        ("square_mode", "sandbox"),
        ("commerce_razorpay_enabled", "false"),
        ("razorpay_key_id", ""),
        ("razorpay_key_secret", ""),
        ("commerce_mollie_enabled", "false"),
        ("mollie_api_key", ""),
        ("commerce_currency", "USD"),
        ("downloads_max_per_purchase", "3"),
        ("downloads_expiry_hours", "48"),
        ("downloads_license_template", "DIGITAL DOWNLOAD LICENSE AGREEMENT\n\nThis license is granted by Oneguy (\"Licensor\") to the purchaser (\"Licensee\").\n\n1. GRANT OF LICENSE\nThe Licensor grants the Licensee a non-exclusive, non-transferable, worldwide license to use the purchased digital file (\"Work\") subject to the terms below.\n\n2. PERMITTED USES\n- Personal use (prints, wallpapers, personal projects)\n- Commercial use in a single end product (website, marketing material, publication)\n- Social media use with credit to the Licensor\n\n3. RESTRICTIONS\n- The Work may NOT be resold, sublicensed, or redistributed as-is\n- The Work may NOT be used in on-demand print services (POD) without a separate license\n- The Work may NOT be included in any competing stock/download service\n- The Work may NOT be used to train AI or machine learning models\n\n4. ATTRIBUTION\nAttribution is appreciated but not required for personal or commercial use.\n\n5. WARRANTY\nThe Work is provided \"as is\" without warranty of any kind. The Licensor is not liable for any damages arising from the use of the Work.\n\n6. TERMINATION\nThis license is effective until terminated. It terminates automatically if the Licensee breaches any terms. Upon termination, the Licensee must destroy all copies of the Work.\n\nBy downloading the Work, the Licensee agrees to these terms."),
        // AI (Phase 4 — defaults ready)
        ("ai_failover_chain", "ollama,openai,gemini,groq,cloudflare"),
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
        ("ai_groq_enabled", "false"),
        ("ai_groq_api_key", ""),
        ("ai_groq_model", "llama-3.3-70b-versatile"),
        ("ai_suggest_meta", "true"),
        ("ai_suggest_tags", "true"),
        ("ai_suggest_categories", "false"),
        ("ai_suggest_alt_text", "true"),
        ("ai_suggest_slug", "true"),
        ("ai_theme_generation", "true"),
        ("ai_post_generation", "true"),
        ("ai_temperature", "0.7"),
        // Email
        ("email_failover_enabled", "false"),
        ("email_failover_chain", "gmail,resend,ses,postmark,brevo,sendpulse,mailgun,moosend,mandrill,sparkpost,smtp"),
        ("email_from_name", ""),
        ("email_from_address", ""),
        ("email_reply_to", ""),
        ("email_gmail_enabled", "false"),
        ("email_gmail_address", ""),
        ("email_gmail_app_password", ""),
        ("email_resend_enabled", "false"),
        ("email_resend_api_key", ""),
        ("email_ses_enabled", "false"),
        ("email_ses_access_key", ""),
        ("email_ses_secret_key", ""),
        ("email_ses_region", "us-east-1"),
        ("email_postmark_enabled", "false"),
        ("email_postmark_server_token", ""),
        ("email_brevo_enabled", "false"),
        ("email_brevo_api_key", ""),
        ("email_sendpulse_enabled", "false"),
        ("email_sendpulse_client_id", ""),
        ("email_sendpulse_client_secret", ""),
        ("email_mailgun_enabled", "false"),
        ("email_mailgun_api_key", ""),
        ("email_mailgun_domain", ""),
        ("email_mailgun_region", "us"),
        ("email_moosend_enabled", "false"),
        ("email_moosend_api_key", ""),
        ("email_mandrill_enabled", "false"),
        ("email_mandrill_api_key", ""),
        ("email_sparkpost_enabled", "false"),
        ("email_sparkpost_api_key", ""),
        ("email_sparkpost_region", "us"),
        ("email_smtp_enabled", "false"),
        ("email_smtp_host", ""),
        ("email_smtp_port", "587"),
        ("email_smtp_username", ""),
        ("email_smtp_password", ""),
        ("email_smtp_encryption", "tls"),
        // Firewall
        ("firewall_enabled", "false"),
        ("fw_monitor_bots", "true"),
        ("fw_bot_auto_ban", "false"),
        ("fw_bot_ban_threshold", "10"),
        ("fw_bot_ban_duration", "24h"),
        ("fw_failed_login_tracking", "true"),
        ("fw_failed_login_ban_threshold", "5"),
        ("fw_failed_login_ban_duration", "1h"),
        ("fw_ban_unknown_users", "false"),
        ("fw_unknown_user_ban_duration", "24h"),
        ("fw_xss_protection", "true"),
        ("fw_sqli_protection", "true"),
        ("fw_path_traversal_protection", "true"),
        ("fw_csrf_strict", "true"),
        ("fw_injection_ban_duration", "7d"),
        ("fw_rate_limit_enabled", "true"),
        ("fw_rate_limit_requests", "100"),
        ("fw_rate_limit_window", "60"),
        ("fw_rate_limit_ban_duration", "1h"),
        ("fw_payment_abuse_detection", "true"),
        ("fw_payment_ban_threshold", "3"),
        ("fw_payment_ban_duration", "30d"),
        ("fw_geo_blocking_enabled", "false"),
        ("fw_geo_block_visitors", "true"),
        ("fw_geo_block_admin", "true"),
        ("fw_geo_blocked_countries", ""),
        ("fw_geo_allowed_countries", ""),
        ("fw_security_headers", "true"),
        // Background Tasks
        ("task_session_cleanup_interval", "30"),
        ("task_session_max_age_days", "30"),
        ("task_magic_link_cleanup_interval", "60"),
        ("task_scheduled_publish_interval", "1"),
        ("task_audit_log_cleanup_interval", "1440"),
        ("task_audit_log_max_age_days", "90"),
        ("task_analytics_cleanup_interval", "1440"),
        ("task_analytics_max_age_days", "365"),
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

    // Data migration: remove 'local' from AI failover chain, ensure 'groq' is present
    {
        let chain: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_failover_chain'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "ollama,openai,gemini,groq,cloudflare".to_string());

        let mut providers: Vec<&str> = chain.split(',').map(|s| s.trim()).filter(|s| *s != "local" && !s.is_empty()).collect();
        if !providers.contains(&"groq") {
            // Insert groq before cloudflare, or at the end
            if let Some(pos) = providers.iter().position(|p| *p == "cloudflare") {
                providers.insert(pos, "groq");
            } else {
                providers.push("groq");
            }
        }
        let new_chain = providers.join(",");
        if new_chain != chain {
            conn.execute(
                "UPDATE settings SET value = ?1 WHERE key = 'ai_failover_chain'",
                params![new_chain],
            )?;
        }
    }

    // Add parent_id to comments for threaded replies
    let has_parent_id: bool = conn
        .prepare("SELECT parent_id FROM comments LIMIT 0")
        .is_ok();
    if !has_parent_id {
        conn.execute_batch("ALTER TABLE comments ADD COLUMN parent_id INTEGER DEFAULT NULL;")?;
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

    // ── Auto-migrate settings-based admin into users table ──
    let user_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users",
        [],
        |row| row.get(0),
    )?;

    if user_count == 0 {
        // Migrate existing admin from settings → users row #1
        let admin_email: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'admin_email'", [], |row| row.get(0))
            .unwrap_or_default();
        let admin_hash: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'admin_password_hash'", [], |row| row.get(0))
            .unwrap_or_default();
        let admin_name: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'admin_display_name'", [], |row| row.get(0))
            .unwrap_or_else(|_| "Admin".to_string());
        let admin_avatar: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'admin_avatar'", [], |row| row.get(0))
            .unwrap_or_default();
        let mfa_enabled: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'mfa_enabled'", [], |row| row.get(0))
            .unwrap_or_else(|_| "false".to_string());
        let mfa_secret: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'mfa_secret'", [], |row| row.get(0))
            .unwrap_or_default();
        let mfa_codes: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'mfa_recovery_codes'", [], |row| row.get(0))
            .unwrap_or_else(|_| "[]".to_string());

        if !admin_email.is_empty() && !admin_hash.is_empty() {
            let mfa_int: i32 = if mfa_enabled == "true" { 1 } else { 0 };
            conn.execute(
                "INSERT INTO users (email, password_hash, display_name, role, status, avatar, mfa_enabled, mfa_secret, mfa_recovery_codes)
                 VALUES (?1, ?2, ?3, 'admin', 'active', ?4, ?5, ?6, ?7)",
                params![admin_email, admin_hash, admin_name, admin_avatar, mfa_int, mfa_secret, mfa_codes],
            )?;

            // Assign user_id=1 to all existing posts, portfolio, and sessions
            conn.execute_batch("UPDATE posts SET user_id = 1 WHERE user_id IS NULL;")?;
            conn.execute_batch("UPDATE portfolio SET user_id = 1 WHERE user_id IS NULL;")?;
            conn.execute_batch("UPDATE sessions SET user_id = 1 WHERE user_id IS NULL;")?;

            log::info!("Migrated admin '{}' from settings to users table (id=1)", admin_email);
        }
    }

    // Always backfill any orphaned sessions (e.g. created before migration ran)
    let has_orphan_sessions: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE user_id IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) > 0;
    if has_orphan_sessions {
        // Assign to first admin user
        let first_admin_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM users WHERE role = 'admin' ORDER BY id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(admin_id) = first_admin_id {
            conn.execute(
                "UPDATE sessions SET user_id = ?1 WHERE user_id IS NULL",
                params![admin_id],
            )?;
            log::info!("Backfilled orphaned sessions with user_id={}", admin_id);
        }
    }

    // Migrate any "suspended" status to "locked" (suspend concept removed)
    conn.execute_batch("UPDATE users SET status = 'locked' WHERE status = 'suspended';")?;

    Ok(())
}
