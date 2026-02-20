use serde::{Deserialize, Serialize};

/// Individual SEO check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoIssue {
    pub code: String,
    pub severity: String, // "error", "warning", "info"
    pub message: String,
    pub points_lost: i32,
}

/// Full SEO audit result for a single content item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoAudit {
    pub score: i32, // 0-100
    pub grade: String, // "good", "needs_work", "poor"
    pub issues: Vec<SeoIssue>,
}

/// Input for scoring — works for both posts and portfolio items
pub struct SeoInput<'a> {
    pub title: &'a str,
    pub slug: &'a str,
    pub meta_title: &'a str,
    pub meta_description: &'a str,
    pub body_html: &'a str,
    pub featured_image: &'a str,
    pub content_type: &'a str, // "post" or "portfolio"
}

/// Compute SEO score for a content item.
/// Starts at 100 and deducts points for each issue found.
pub fn compute_score(input: &SeoInput) -> SeoAudit {
    let mut issues = Vec::new();
    let mut deductions = 0i32;

    // ── Meta Title ──
    let effective_title = if input.meta_title.is_empty() {
        input.title
    } else {
        input.meta_title
    };

    if effective_title.is_empty() {
        issues.push(SeoIssue {
            code: "meta_title_missing".into(),
            severity: "error".into(),
            message: "Meta title is missing".into(),
            points_lost: 15,
        });
        deductions += 15;
    } else {
        let len = effective_title.len();
        if len < 30 {
            issues.push(SeoIssue {
                code: "meta_title_short".into(),
                severity: "warning".into(),
                message: format!("Meta title is too short ({} chars, aim for 50-60)", len),
                points_lost: 5,
            });
            deductions += 5;
        } else if len > 70 {
            issues.push(SeoIssue {
                code: "meta_title_long".into(),
                severity: "warning".into(),
                message: format!(
                    "Meta title is too long ({} chars, may be truncated in search results)",
                    len
                ),
                points_lost: 3,
            });
            deductions += 3;
        }
    }

    // ── Meta Description ──
    if input.meta_description.is_empty() {
        issues.push(SeoIssue {
            code: "meta_desc_missing".into(),
            severity: "error".into(),
            message: "Meta description is missing".into(),
            points_lost: 15,
        });
        deductions += 15;
    } else {
        let len = input.meta_description.len();
        if len < 70 {
            issues.push(SeoIssue {
                code: "meta_desc_short".into(),
                severity: "warning".into(),
                message: format!(
                    "Meta description is too short ({} chars, aim for 120-160)",
                    len
                ),
                points_lost: 5,
            });
            deductions += 5;
        } else if len > 170 {
            issues.push(SeoIssue {
                code: "meta_desc_long".into(),
                severity: "warning".into(),
                message: format!(
                    "Meta description is too long ({} chars, may be truncated)",
                    len
                ),
                points_lost: 3,
            });
            deductions += 3;
        }
    }

    // ── Slug Quality ──
    if input.slug.is_empty() {
        issues.push(SeoIssue {
            code: "slug_missing".into(),
            severity: "error".into(),
            message: "URL slug is missing".into(),
            points_lost: 10,
        });
        deductions += 10;
    } else if input.slug.len() > 75 {
        issues.push(SeoIssue {
            code: "slug_long".into(),
            severity: "warning".into(),
            message: format!("URL slug is very long ({} chars)", input.slug.len()),
            points_lost: 3,
        });
        deductions += 3;
    }

    // ── Featured Image / Media ──
    if input.featured_image.is_empty() || input.featured_image == "placeholder.jpg" {
        let label = if input.content_type == "portfolio" {
            "Featured media"
        } else {
            "Featured image"
        };
        issues.push(SeoIssue {
            code: "featured_image_missing".into(),
            severity: "warning".into(),
            message: format!("{} is missing", label),
            points_lost: 10,
        });
        deductions += 10;
    }

    // ── Content Analysis (body HTML) ──
    let body_text = strip_html_tags(input.body_html);
    let word_count = count_words(&body_text);

    if input.content_type == "post" {
        // Posts should have substantial content
        if word_count == 0 {
            issues.push(SeoIssue {
                code: "content_empty".into(),
                severity: "error".into(),
                message: "Post has no content".into(),
                points_lost: 20,
            });
            deductions += 20;
        } else if word_count < 100 {
            issues.push(SeoIssue {
                code: "content_thin".into(),
                severity: "error".into(),
                message: format!("Content is very thin ({} words, aim for 300+)", word_count),
                points_lost: 15,
            });
            deductions += 15;
        } else if word_count < 300 {
            issues.push(SeoIssue {
                code: "content_short".into(),
                severity: "warning".into(),
                message: format!("Content is short ({} words, aim for 300+)", word_count),
                points_lost: 5,
            });
            deductions += 5;
        }
    } else {
        // Portfolio items: description is optional but helpful
        if word_count < 20 && !input.body_html.is_empty() {
            issues.push(SeoIssue {
                code: "description_thin".into(),
                severity: "info".into(),
                message: format!("Description is very short ({} words)", word_count),
                points_lost: 3,
            });
            deductions += 3;
        }
    }

    // ── Heading Structure ──
    if input.content_type == "post" && word_count > 100 {
        let has_h2 = input.body_html.contains("<h2") || input.body_html.contains("<H2");
        if !has_h2 {
            issues.push(SeoIssue {
                code: "no_headings".into(),
                severity: "warning".into(),
                message: "No H2 headings found — use headings to structure content".into(),
                points_lost: 5,
            });
            deductions += 5;
        }
    }

    // ── Images in Body ──
    let img_count = count_pattern(input.body_html, "<img ");
    let img_no_alt = count_images_without_alt(input.body_html);
    if img_count > 0 && img_no_alt > 0 {
        issues.push(SeoIssue {
            code: "images_missing_alt".into(),
            severity: "warning".into(),
            message: format!(
                "{} of {} images missing alt text",
                img_no_alt, img_count
            ),
            points_lost: 5,
        });
        deductions += 5;
    }

    // ── Internal Links (posts only) ──
    if input.content_type == "post" && word_count > 200 {
        let has_links = input.body_html.contains("<a ");
        if !has_links {
            issues.push(SeoIssue {
                code: "no_links".into(),
                severity: "info".into(),
                message: "No links found in content — consider adding internal or external links"
                    .into(),
                points_lost: 3,
            });
            deductions += 3;
        }
    }

    let score = (100 - deductions).max(0).min(100);
    let grade = if score >= 80 {
        "good"
    } else if score >= 50 {
        "needs_work"
    } else {
        "poor"
    }
    .to_string();

    SeoAudit {
        score,
        grade,
        issues,
    }
}

/// Serialize issues to a compact JSON string for DB storage
pub fn issues_to_json(issues: &[SeoIssue]) -> String {
    serde_json::to_string(issues).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize issues from JSON string
pub fn issues_from_json(json: &str) -> Vec<SeoIssue> {
    serde_json::from_str(json).unwrap_or_default()
}

// ── Helpers ──

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
            result.push(' ');
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.len() > 1 || w.chars().all(|c| c.is_alphanumeric()))
        .count()
}

fn count_pattern(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn count_images_without_alt(html: &str) -> usize {
    let mut count = 0;
    let lower = html.to_lowercase();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<img ") {
        let abs_start = pos + start;
        let end = lower[abs_start..].find('>').unwrap_or(lower.len() - abs_start);
        let tag = &lower[abs_start..abs_start + end + 1];
        // Missing alt, or alt=""
        if !tag.contains("alt=") || tag.contains("alt=\"\"") || tag.contains("alt=''") {
            count += 1;
        }
        pos = abs_start + end + 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input<'a>(
        title: &'a str,
        meta_title: &'a str,
        meta_desc: &'a str,
        body: &'a str,
        image: &'a str,
        content_type: &'a str,
    ) -> SeoInput<'a> {
        SeoInput {
            title,
            slug: "test-slug",
            meta_title,
            meta_description: meta_desc,
            body_html: body,
            featured_image: image,
            content_type,
        }
    }

    #[test]
    fn perfect_post_scores_100() {
        let body = "<h2>Introduction</h2><p>".to_string()
            + &"word ".repeat(350)
            + "</p><a href='/other'>link</a><img src='x.jpg' alt='photo'>";
        let input = make_input(
            "A Great Title That Is Fifty Characters Long Exactly",
            "A Great Title That Is Fifty Characters Long Exactly",
            "This is a meta description that is between 120 and 160 characters long, which is the ideal length for search engine results pages.",
            &body,
            "photo.jpg",
            "post",
        );
        let result = compute_score(&input);
        assert_eq!(result.score, 100);
        assert_eq!(result.grade, "good");
        assert!(result.issues.is_empty());
    }

    #[test]
    fn missing_everything_scores_low() {
        let input = make_input("", "", "", "", "", "post");
        let result = compute_score(&input);
        assert!(result.score < 50);
        assert_eq!(result.grade, "poor");
        assert!(result.issues.len() >= 3);
    }

    #[test]
    fn missing_meta_desc_deducts_15() {
        let body = "<h2>Heading</h2><p>".to_string()
            + &"word ".repeat(350)
            + "</p><a href='/x'>link</a>";
        let input = make_input(
            "A Good Title That Is Long Enough For SEO",
            "A Good Title That Is Long Enough For SEO",
            "",
            &body,
            "photo.jpg",
            "post",
        );
        let result = compute_score(&input);
        assert_eq!(result.score, 85);
        assert!(result.issues.iter().any(|i| i.code == "meta_desc_missing"));
    }

    #[test]
    fn thin_content_deducts() {
        let input = make_input(
            "A Good Title That Is Long Enough For SEO",
            "A Good Title That Is Long Enough For SEO",
            "A good meta description that is between 120 and 160 characters long for search engine optimization purposes here.",
            "<p>Short post.</p>",
            "photo.jpg",
            "post",
        );
        let result = compute_score(&input);
        assert!(result.issues.iter().any(|i| i.code == "content_thin"));
    }

    #[test]
    fn portfolio_without_image_deducts() {
        let input = make_input(
            "My Portfolio Item",
            "My Portfolio Item With Good Title",
            "A description for this portfolio item that is long enough to pass the check.",
            "",
            "placeholder.jpg",
            "portfolio",
        );
        let result = compute_score(&input);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == "featured_image_missing"));
    }

    #[test]
    fn images_without_alt_detected() {
        let body = "<p>Text</p><img src='a.jpg'><img src='b.jpg' alt='good'><img src='c.jpg' alt=''>";
        let input = make_input(
            "Title That Is Long Enough For Good SEO Score",
            "Title That Is Long Enough For Good SEO Score",
            "A meta description that is between 120 and 160 characters long for search engine optimization purposes here today.",
            body,
            "photo.jpg",
            "post",
        );
        let result = compute_score(&input);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == "images_missing_alt"));
    }

    #[test]
    fn issues_json_roundtrip() {
        let issues = vec![SeoIssue {
            code: "test".into(),
            severity: "warning".into(),
            message: "Test issue".into(),
            points_lost: 5,
        }];
        let json = issues_to_json(&issues);
        let parsed = issues_from_json(&json);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].code, "test");
    }

    #[test]
    fn strip_html_works() {
        assert_eq!(
            strip_html_tags("<p>Hello <b>world</b></p>").trim(),
            "Hello  world"
        );
    }

    #[test]
    fn count_words_works() {
        assert_eq!(count_words("Hello world, this is a test"), 6);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn grade_boundaries() {
        // Score 80+ = good
        let body = "<h2>H</h2><p>".to_string()
            + &"word ".repeat(350)
            + "</p><a href='/x'>l</a>";
        let input = make_input(
            "Title That Is Long Enough",
            "Title That Is Long Enough",
            "A meta description that is between 120 and 160 characters long for search engine optimization purposes here today.",
            &body,
            "photo.jpg",
            "post",
        );
        let r = compute_score(&input);
        assert_eq!(r.grade, "good");
    }
}
