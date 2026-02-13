use quick_xml::events::Event;
use quick_xml::Reader;

use crate::db::DbPool;
use crate::models::category::{Category, CategoryForm};
use crate::models::comment::{Comment, CommentForm};
use crate::models::import::Import;
use crate::models::post::{Post, PostForm};
use crate::models::tag::Tag;

/// Result of a WordPress import
pub struct WpImportResult {
    pub posts_imported: i64,
    pub portfolio_imported: i64,
    pub comments_imported: i64,
    pub skipped: i64,
    pub log: Vec<String>,
}

/// Parse and import a WordPress WXR XML export file
pub fn import_wxr(pool: &DbPool, xml_content: &str) -> Result<WpImportResult, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut result = WpImportResult {
        posts_imported: 0,
        portfolio_imported: 0,
        comments_imported: 0,
        skipped: 0,
        log: Vec::new(),
    };

    let mut buf = Vec::new();
    let mut current_element = String::new();
    let mut in_item = false;

    // Current item fields
    let mut item_title = String::new();
    let mut item_content = String::new();
    let mut item_excerpt = String::new();
    let mut item_slug = String::new();
    let mut item_status = String::new();
    let mut item_post_type = String::new();
    let mut item_date = String::new();
    let mut item_categories: Vec<String> = Vec::new();
    let mut item_tags: Vec<String> = Vec::new();

    // Comment fields
    let mut in_comment = false;
    let mut comment_author = String::new();
    let mut comment_email = String::new();
    let mut comment_content = String::new();
    let mut comment_approved = String::new();
    let mut item_comments: Vec<(String, String, String, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.clone();

                if name == "item" {
                    in_item = true;
                    item_title.clear();
                    item_content.clear();
                    item_excerpt.clear();
                    item_slug.clear();
                    item_status.clear();
                    item_post_type.clear();
                    item_date.clear();
                    item_categories.clear();
                    item_tags.clear();
                    item_comments.clear();
                }

                if name == "wp:comment" {
                    in_comment = true;
                    comment_author.clear();
                    comment_email.clear();
                    comment_content.clear();
                    comment_approved.clear();
                }

                // Category/tag on item
                if in_item && name == "category" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        if key == "domain" {
                            if val == "category" {
                                // Will read text in next event
                                current_element = "wp:item_category".to_string();
                            } else if val == "post_tag" {
                                current_element = "wp:item_tag".to_string();
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();

                if in_comment {
                    match current_element.as_str() {
                        "wp:comment_author" => comment_author = text,
                        "wp:comment_author_email" => comment_email = text,
                        "wp:comment_content" => comment_content = text,
                        "wp:comment_approved" => comment_approved = text,
                        _ => {}
                    }
                } else if in_item {
                    match current_element.as_str() {
                        "title" => item_title = text,
                        "content:encoded" => item_content = text,
                        "excerpt:encoded" => item_excerpt = text,
                        "wp:post_name" => item_slug = text,
                        "wp:status" => item_status = text,
                        "wp:post_type" => item_post_type = text,
                        "wp:post_date" => item_date = text,
                        "wp:item_category" => item_categories.push(text),
                        "wp:item_tag" => item_tags.push(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();

                if in_comment {
                    match current_element.as_str() {
                        "wp:comment_content" => comment_content = text,
                        "wp:comment_author" => comment_author = text,
                        _ => {}
                    }
                } else if in_item {
                    match current_element.as_str() {
                        "content:encoded" => item_content = text,
                        "excerpt:encoded" => item_excerpt = text,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if name == "wp:comment" && in_comment {
                    in_comment = false;
                    if comment_approved == "1" || comment_approved == "approve" {
                        item_comments.push((
                            comment_author.clone(),
                            comment_email.clone(),
                            comment_content.clone(),
                            "approved".to_string(),
                        ));
                    }
                }

                if name == "item" && in_item {
                    in_item = false;

                    // Process the item
                    match item_post_type.as_str() {
                        "post" => {
                            match import_post(pool, &item_title, &item_slug, &item_content,
                                &item_excerpt, &item_status, &item_date,
                                &item_categories, &item_tags, &item_comments)
                            {
                                Ok(_) => {
                                    result.posts_imported += 1;
                                    result.comments_imported += item_comments.len() as i64;
                                    result.log.push(format!("Imported post: {}", item_title));
                                }
                                Err(e) => {
                                    result.skipped += 1;
                                    result.log.push(format!("Skipped post '{}': {}", item_title, e));
                                }
                            }
                        }
                        "portfolio" => {
                            // Portfolio items — import as portfolio if possible
                            result.log.push(format!("Skipped portfolio '{}': portfolio import not yet implemented", item_title));
                            result.skipped += 1;
                        }
                        "page" | "attachment" | "nav_menu_item" | "revision" => {
                            // Skip these types silently
                        }
                        _ => {
                            result.skipped += 1;
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    // Record import in history
    let log_json = serde_json::to_string(&result.log).unwrap_or_default();
    let _ = Import::create(
        pool,
        "wordpress",
        None,
        result.posts_imported,
        result.portfolio_imported,
        result.comments_imported,
        result.skipped,
        Some(&log_json),
    );

    Ok(result)
}

fn import_post(
    pool: &DbPool,
    title: &str,
    slug: &str,
    content: &str,
    excerpt: &str,
    status: &str,
    date: &str,
    categories: &[String],
    tags: &[String],
    comments: &[(String, String, String, String)],
) -> Result<i64, String> {
    if title.is_empty() || slug.is_empty() {
        return Err("Missing title or slug".to_string());
    }

    // Check for duplicate
    if Post::find_by_slug(pool, slug).is_some() {
        return Err("Duplicate slug".to_string());
    }

    let velocty_status = match status {
        "publish" => "published",
        "draft" => "draft",
        "private" => "draft",
        "pending" => "draft",
        _ => "draft",
    };

    let form = PostForm {
        title: title.to_string(),
        slug: slug.to_string(),
        content_json: "{}".to_string(),
        content_html: content.to_string(),
        excerpt: if excerpt.is_empty() {
            None
        } else {
            Some(excerpt.to_string())
        },
        featured_image: None,
        meta_title: None,
        meta_description: None,
        status: velocty_status.to_string(),
        published_at: if velocty_status == "published" {
            Some(date.to_string())
        } else {
            None
        },
        category_ids: None,
        tag_ids: None,
    };

    let post_id = Post::create(pool, &form)?;

    // Import categories
    for cat_name in categories {
        let cat_slug = slug::slugify(cat_name);
        let cat_id = match Category::find_by_slug(pool, &cat_slug) {
            Some(c) => c.id,
            None => Category::create(
                pool,
                &CategoryForm {
                    name: cat_name.clone(),
                    slug: cat_slug,
                    r#type: "post".to_string(),
                },
            )?,
        };
        Category::set_for_content(pool, post_id, "post", &[cat_id])?;
    }

    // Import tags
    for tag_name in tags {
        let tag_id = Tag::find_or_create(pool, tag_name)?;
        Tag::set_for_content(pool, post_id, "post", &[tag_id])?;
    }

    // Import comments
    for (author, email, body, status) in comments {
        let comment_form = CommentForm {
            post_id,
            content_type: Some("post".to_string()),
            author_name: author.clone(),
            author_email: if email.is_empty() {
                None
            } else {
                Some(email.clone())
            },
            body: body.clone(),
            honeypot: None,
        };
        if let Ok(cid) = Comment::create(pool, &comment_form) {
            let _ = Comment::update_status(pool, cid, status);
        }
    }

    Ok(post_id)
}
