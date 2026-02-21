//! Oneguy portfolio renderers.
//! Provides the default portfolio grid and single-item views.
//! Other designs that don't have their own portfolio renderer fall back to these.

use serde_json::Value;

use crate::render::{
    build_comments_section, build_commerce_html, build_share_buttons, format_likes, html_escape,
    render_404, slug_url,
};

/// Render the portfolio grid/masonry listing page.
pub fn render_grid(context: &Value) -> String {
    let items = match context.get("items") {
        Some(Value::Array(items)) => items,
        _ => return "<p>No portfolio items yet.</p>".to_string(),
    };

    if items.is_empty() {
        return "<p>No portfolio items yet.</p>".to_string();
    }

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let display_type = sg("portfolio_display_type", "masonry");
    let show_tags_mode_raw = sg("portfolio_show_tags", "false");
    let show_tags_mode = if show_tags_mode_raw == "true" {
        "below_left".to_string()
    } else {
        show_tags_mode_raw
    };
    let show_tags = show_tags_mode != "false";
    let show_cats_mode_raw = sg("portfolio_show_categories", "false");
    let show_cats_mode = if show_cats_mode_raw == "true" {
        "below_left".to_string()
    } else {
        show_cats_mode_raw
    };
    let show_cats = show_cats_mode != "false";
    let show_likes_enabled = sg("portfolio_enable_likes", "true") == "true";
    let grid_hearts_pos = sg("portfolio_like_grid_position", "hidden");
    let show_grid_hearts = show_likes_enabled && grid_hearts_pos != "hidden";
    let fade_mode = sg("portfolio_fade_animation", "true");
    let fade_anim = fade_mode != "none" && fade_mode != "false";
    let fade_class = if fade_mode == "slide_up" {
        "slide-up"
    } else if fade_anim {
        "fade-in"
    } else {
        ""
    };
    let border_style = sg("portfolio_border_style", "none");
    let show_title = sg("portfolio_show_title", "false") == "true";
    let show_price = sg("commerce_show_price", "true") == "true";
    let price_position = sg("commerce_price_position", "top_right");
    let commerce_currency = {
        let c = sg("commerce_currency", "USD");
        if c.is_empty() {
            "USD".to_string()
        } else {
            c
        }
    };

    let grid_class = if display_type == "grid" {
        "css-grid"
    } else {
        "masonry-grid"
    };
    let border_class = match border_style.as_str() {
        "standard" => " border-standard",
        "polaroid" => " border-polaroid",
        _ => "",
    };
    let item_class_str = format!(
        "grid-item{}{}",
        if fade_class.is_empty() {
            String::new()
        } else {
            format!(" {}", fade_class)
        },
        border_class
    );

    // Determine if we need overlay positioning on the link
    let _cats_is_overlay = matches!(
        show_cats_mode.as_str(),
        "hover" | "bottom_left" | "bottom_right"
    );
    let _tags_is_overlay = matches!(
        show_tags_mode.as_str(),
        "hover" | "bottom_left" | "bottom_right"
    );
    let cats_is_below = matches!(show_cats_mode.as_str(), "below_left" | "below_right");
    let tags_is_below = matches!(show_tags_mode.as_str(), "below_left" | "below_right");

    let mut html = format!(r#"<div class="{}">"#, grid_class);

    let portfolio_slug = sg("portfolio_slug", "portfolio");

    for entry in items {
        let item = entry.get("item").unwrap_or(entry);
        let tags = entry.get("tags").and_then(|t| t.as_array());

        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let image = item
            .get("thumbnail_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("image_path").and_then(|v| v.as_str()))
            .unwrap_or("");
        let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);
        let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let item_price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let item_sell = item
            .get("sell_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let cats_data = entry
            .get("categories")
            .and_then(|c| c.as_array())
            .map(|cats| {
                cats.iter()
                    .filter_map(|c| c.get("slug").and_then(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        let tag_data = if show_tags {
            tags.map(|tl| {
                tl.iter()
                    .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
        } else {
            String::new()
        };

        // Build category inner HTML (just the links, no wrapper div)
        let cats_inner = if show_cats {
            let cat_list = entry.get("categories").and_then(|c| c.as_array());
            if let Some(cats) = cat_list {
                if !cats.is_empty() {
                    let cat_strs: Vec<String> = cats
                        .iter()
                        .filter_map(|c| {
                            let name = c.get("name").and_then(|v| v.as_str())?;
                            let cslug = c.get("slug").and_then(|v| v.as_str())?;
                            Some(format!(
                                "<a href=\"{}\">{}</a>",
                                slug_url(&portfolio_slug, &format!("category/{}", cslug)),
                                html_escape(name)
                            ))
                        })
                        .collect();
                    if !cat_strs.is_empty() {
                        cat_strs.join(" · ")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Build tag inner HTML (just the links, no wrapper div)
        let tags_inner = if show_tags {
            if let Some(tag_list) = tags {
                if !tag_list.is_empty() {
                    let tag_strs: Vec<String> = tag_list
                        .iter()
                        .filter_map(|t| {
                            let name = t.get("name").and_then(|v| v.as_str())?;
                            let tslug = t.get("slug").and_then(|v| v.as_str())?;
                            Some(format!(
                                "<a href=\"{}\">{}</a>",
                                slug_url(&portfolio_slug, &format!("tag/{}", tslug)),
                                html_escape(name)
                            ))
                        })
                        .collect();
                    if !tag_strs.is_empty() {
                        tag_strs.join(" · ")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let item_url = slug_url(&portfolio_slug, slug);
        // Price badge HTML (overlay positions: inside the <a> tag)
        let has_price_overlay =
            show_price && item_sell && item_price > 0.0 && price_position != "below_title";
        let price_badge_overlay = if has_price_overlay {
            let pos_style = match price_position.as_str() {
                "top_left" => "position:absolute;top:8px;left:8px",
                "bottom_left" => "position:absolute;bottom:8px;left:8px",
                _ => "position:absolute;top:8px;right:8px", // top_right default
            };
            format!(
                r#"<span class="price-badge" style="{};background:rgba(0,0,0,.75);color:#fff;padding:4px 10px;border-radius:4px;font-size:12px;font-weight:700;z-index:2">{} {:.2}</span>"#,
                pos_style,
                html_escape(&commerce_currency),
                item_price
            )
        } else {
            String::new()
        };

        // Heart overlay on grid thumbnail
        let heart_overlay = if show_grid_hearts {
            let h_is_top = grid_hearts_pos.starts_with("top");
            let h_is_right = grid_hearts_pos.ends_with("right");
            // Check if price badge is on the same corner
            let price_is_top = price_position.starts_with("top");
            let price_is_right = price_position.ends_with("right") || price_position == "top_right";
            let same_corner =
                has_price_overlay && (h_is_top == price_is_top) && (h_is_right == price_is_right);
            let v_offset = if same_corner {
                if h_is_top {
                    "top:36px"
                } else {
                    "bottom:36px"
                }
            } else if h_is_top {
                "top:8px"
            } else {
                "bottom:8px"
            };
            let h_offset = if h_is_right { "right:8px" } else { "left:8px" };
            format!(
                r#"<span class="like-btn grid-heart" data-id="{}" style="position:absolute;{};{};z-index:3;background:rgba(0,0,0,.45);color:#fff;padding:3px 8px;border-radius:16px;font-size:12px;cursor:pointer;user-select:none">&hearts; <span class="like-count">{}</span></span>"#,
                item_id,
                v_offset,
                h_offset,
                format_likes(likes)
            )
        } else {
            String::new()
        };

        let is_video = crate::routes::admin::is_video_filename(image);
        let media_tag = if is_video {
            format!(
                r#"<video src="/uploads/{}" autoplay muted loop playsinline preload="metadata"></video>"#,
                image
            )
        } else {
            format!(
                r#"<img src="/uploads/{}" alt="{}" loading="lazy">"#,
                image,
                html_escape(title)
            )
        };
        let media_type_attr = if is_video {
            " data-media-type=\"video\""
        } else {
            ""
        };

        html.push_str(&format!(
            r#"<div class="{item_class}" data-categories="{cats_data}" data-price="{price}" data-sell="{sell}"{media_type}>
    <a href="{item_url}" class="portfolio-link" data-id="{item_id}" data-title="{title}" data-likes="{likes}" data-tags="{tag_data}" style="position:relative;display:block">
        {media_tag}
        {price_badge}
        {heart_overlay}
    </a>"#,
            item_class = item_class_str,
            cats_data = cats_data,
            item_url = item_url,
            item_id = item_id,
            title = html_escape(title),
            likes = likes,
            tag_data = html_escape(&tag_data),
            price = item_price,
            sell = item_sell,
            media_type = media_type_attr,
            media_tag = media_tag,
            price_badge = price_badge_overlay,
            heart_overlay = heart_overlay,
        ));

        // Hover overlay: shared container with dimmed background, centered content
        let cats_is_hover = show_cats_mode == "hover";
        let tags_is_hover = show_tags_mode == "hover";
        if (cats_is_hover && !cats_inner.is_empty()) || (tags_is_hover && !tags_inner.is_empty()) {
            html.push_str("<div class=\"item-hover-overlay\">");
            html.push_str("<div class=\"item-hover-content\">");
            if cats_is_hover && !cats_inner.is_empty() {
                html.push_str(&format!(
                    "<div class=\"item-categories item-meta-hover\">{}</div>",
                    cats_inner
                ));
            }
            if tags_is_hover && !tags_inner.is_empty() {
                html.push_str(&format!(
                    "<div class=\"item-tags item-meta-hover\">{}</div>",
                    tags_inner
                ));
            }
            html.push_str("</div></div>");
        }

        // Corner overlays (bottom_left, bottom_right) — always visible, positioned in corners
        if show_cats_mode == "bottom_left" && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-bottom_left\">{}</div>",
                cats_inner
            ));
        }
        if show_cats_mode == "bottom_right" && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-bottom_right\">{}</div>",
                cats_inner
            ));
        }
        if show_tags_mode == "bottom_left" && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-bottom_left\">{}</div>",
                tags_inner
            ));
        }
        if show_tags_mode == "bottom_right" && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-bottom_right\">{}</div>",
                tags_inner
            ));
        }

        // Title below image
        if show_title && !title.is_empty() {
            html.push_str(&format!(
                r#"<div class="item-title">{}</div>"#,
                html_escape(title)
            ));
        }

        // Price badge: below_title position
        if show_price && item_sell && item_price > 0.0 && price_position == "below_title" {
            html.push_str(&format!(
                r#"<div class="price-badge-below" style="font-size:13px;font-weight:700;color:#333;padding:4px 0">{} {:.2}</div>"#,
                html_escape(&commerce_currency), item_price
            ));
        }

        // Below-image categories
        if cats_is_below && !cats_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-categories item-meta-{}\">{}</div>",
                show_cats_mode, cats_inner
            ));
        }

        // Below-image tags
        if tags_is_below && !tags_inner.is_empty() {
            html.push_str(&format!(
                "<div class=\"item-tags item-meta-{}\">{}</div>",
                show_tags_mode, tags_inner
            ));
        }

        html.push_str("</div>\n");
    }

    html.push_str("</div>");

    // Pagination
    let current_page = context
        .get("current_page")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let total_pages = context
        .get("total_pages")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let pagination_type = sg("portfolio_pagination_type", "classic");

    if total_pages > 1 {
        match pagination_type.as_str() {
            "load_more" => {
                html.push_str(&format!(
                    "<div class=\"pagination\" style=\"justify-content:center\">\
                     <button id=\"load-more-btn\" data-page=\"{}\" data-total=\"{}\" \
                     style=\"padding:10px 28px;border:1px solid #ddd;border-radius:4px;background:transparent;cursor:pointer;font-size:14px\">\
                     Load More</button></div>",
                    current_page + 1, total_pages
                ));
            }
            "infinite" => {
                html.push_str(&format!(
                    "<div id=\"infinite-sentinel\" data-page=\"{}\" data-total=\"{}\" \
                     style=\"height:1px\"></div>",
                    current_page + 1,
                    total_pages
                ));
            }
            _ => {
                // Classic pagination
                html.push_str(&crate::render::build_pagination(current_page, total_pages));
            }
        }
    }

    html
}

/// Render a single portfolio item page.
pub fn render_single(context: &Value) -> String {
    let item = match context.get("item") {
        Some(i) => i,
        None => return render_404(context),
    };

    let settings = context.get("settings").cloned().unwrap_or_default();
    let sg = |key: &str, def: &str| -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(def)
            .to_string()
    };
    let show_likes = sg("portfolio_enable_likes", "true") == "true";
    let show_cats = sg("portfolio_show_categories", "false") != "false";
    let show_tags = sg("portfolio_show_tags", "false") != "false";
    let portfolio_slug = sg("portfolio_slug", "portfolio");

    let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let image = item
        .get("image_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let desc = item
        .get("description_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let likes = item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0);
    let item_id = item.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

    let tags = context.get("tags").and_then(|t| t.as_array());
    let categories = context.get("categories").and_then(|c| c.as_array());

    let hearts_pos = sg("portfolio_like_position", "bottom_right");
    let like_overlay = if show_likes {
        let (h_top, h_left) = match hearts_pos.as_str() {
            "top_left" => ("top:12px", "left:12px"),
            "top_right" => ("top:12px", "right:12px"),
            "bottom_left" => ("bottom:12px", "left:12px"),
            _ => ("bottom:12px", "right:12px"),
        };
        format!(
            r#"<span class="like-btn" data-id="{}" style="position:absolute;{};{};z-index:10;background:rgba(0,0,0,.45);padding:4px 10px;border-radius:20px;color:#fff;font-size:14px">♥ <span class="like-count">{}</span></span>"#,
            item_id,
            h_top,
            h_left,
            format_likes(likes)
        )
    } else {
        String::new()
    };

    let share_pos = sg("share_icons_position", "below_content");
    let site_url = sg("site_url", "");
    let item_slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let page_url = if !site_url.is_empty() {
        format!("{}{}", site_url, slug_url(&portfolio_slug, item_slug))
    } else {
        String::new()
    };

    // Share buttons below image (between image and meta)
    let share_below_image = if share_pos == "below_image" && !page_url.is_empty() {
        build_share_buttons(&settings, &page_url, title)
    } else {
        String::new()
    };

    // Pre-build commerce HTML so we can place it at the right position
    let commerce_enabled = context
        .get("commerce_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let commerce_html = if commerce_enabled {
        let price = item.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let purchase_note = item
            .get("purchase_note")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let payment_provider = item
            .get("payment_provider")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        build_commerce_html(price, purchase_note, item_id, &settings, payment_provider)
    } else {
        String::new()
    };
    let commerce_pos = sg("commerce_button_position", "below_description");
    let is_sidebar_commerce = commerce_pos == "sidebar_right" && !commerce_html.is_empty();

    let mut html = String::from(r#"<article class="portfolio-single">"#);

    // Sidebar layout: wrap image+content in a flex row with commerce on the right
    if is_sidebar_commerce {
        html.push_str(r#"<div class="portfolio-single-row" style="display:flex;gap:32px;align-items:flex-start">"#);
        html.push_str(r#"<div class="portfolio-single-main" style="flex:1;min-width:0">"#);
    }

    html.push_str(&format!(
        r#"<div class="portfolio-image" style="position:relative">
        <img src="/uploads/{image}" alt="{title}">
        {like_overlay}
    </div>
    {share_below_image}"#,
        image = image,
        title = html_escape(title),
        like_overlay = like_overlay,
        share_below_image = share_below_image,
    ));

    // Commerce: below_image position
    if commerce_pos == "below_image" && !commerce_html.is_empty() {
        html.push_str(&commerce_html);
    }

    html.push_str(&format!(
        r#"<div class="portfolio-meta">
        <h1>{title}</h1>
    </div>"#,
        title = html_escape(title),
    ));

    if show_cats {
        if let Some(cats) = categories {
            if !cats.is_empty() {
                html.push_str(r#"<div class="portfolio-categories">"#);
                for cat in cats {
                    let name = cat.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let slug = cat.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                    html.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        slug_url(&portfolio_slug, &format!("category/{}", slug)),
                        html_escape(name)
                    ));
                }
                html.push_str("</div>");
            }
        }
    }

    if show_tags {
        if let Some(tag_list) = tags {
            if !tag_list.is_empty() {
                html.push_str(r#"<div class="item-tags" style="margin-bottom:12px">"#);
                let tag_strs: Vec<String> = tag_list
                    .iter()
                    .filter_map(|t| {
                        let name = t.get("name").and_then(|v| v.as_str())?;
                        let tslug = t.get("slug").and_then(|v| v.as_str())?;
                        Some(format!(
                            "<a href=\"{}\">{}</a>",
                            slug_url(&portfolio_slug, &format!("tag/{}", tslug)),
                            html_escape(name)
                        ))
                    })
                    .collect();
                html.push_str(&tag_strs.join(" · "));
                html.push_str("</div>");
            }
        }
    }

    if !desc.is_empty() {
        html.push_str(&format!(
            r#"<div class="portfolio-description">{}</div>"#,
            desc
        ));
    }

    // Share buttons — below content (after description)
    if share_pos == "below_content" && !page_url.is_empty() {
        html.push_str(&build_share_buttons(&settings, &page_url, title));
    }

    // Commerce: below_description position (default)
    if commerce_pos == "below_description" && !commerce_html.is_empty() {
        html.push_str(&commerce_html);
    }

    // Close sidebar layout main column and add commerce sidebar
    if is_sidebar_commerce {
        html.push_str("</div>"); // end portfolio-single-main
        html.push_str(
            r#"<div class="portfolio-single-sidebar" style="width:340px;flex-shrink:0">"#,
        );
        html.push_str(&commerce_html);
        html.push_str("</div>"); // end portfolio-single-sidebar
        html.push_str("</div>"); // end portfolio-single-row
    }

    // Comments on portfolio (gated on comments_enabled flag from route)
    let comments_on = context
        .get("comments_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if comments_on {
        html.push_str(&build_comments_section(
            context,
            &settings,
            item_id,
            "portfolio",
        ));
    }

    html.push_str("</article>");

    // JSON-LD structured data
    if settings.get("seo_structured_data").and_then(|v| v.as_str()) == Some("true") {
        let site_name = settings
            .get("site_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Velocty");
        let site_url = settings
            .get("site_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:8000");
        let portfolio_slug = settings
            .get("portfolio_slug")
            .and_then(|v| v.as_str())
            .unwrap_or("portfolio");
        let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        let meta_desc = item
            .get("meta_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        html.push_str(&format!(
            r#"<script type="application/ld+json">
{{
    "@context": "https://schema.org",
    "@type": "ImageObject",
    "name": "{}",
    "description": "{}",
    "contentUrl": "{}/uploads/{}",
    "url": "{}{}",
    "publisher": {{ "@type": "Organization", "name": "{}" }}
}}
</script>"#,
            html_escape(title),
            html_escape(meta_desc),
            site_url,
            image,
            site_url,
            slug_url(portfolio_slug, slug),
            html_escape(site_name),
        ));
    }

    html
}
