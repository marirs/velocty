pub mod generate;
pub mod status;
pub mod suggest;

use serde_json::{json, Value};

// ── Helpers ───────────────────────────────────────────

/// Extract JSON from LLM response text (handles markdown fences, leading text, etc.)
pub fn parse_json_from_text(text: &str) -> Option<Value> {
    log::debug!("AI raw response: {}", &text[..text.len().min(500)]);

    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }

    // Try to find JSON within markdown code fences
    let stripped = text.replace("```json", "").replace("```", "");
    if let Ok(v) = serde_json::from_str::<Value>(stripped.trim()) {
        return Some(v);
    }

    // Try to find first { ... } block (handle nested braces)
    if let Some(start) = text.find('{') {
        let mut depth = 0;
        let mut end_pos = None;
        for (i, ch) in text[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end_pos {
            let candidate = &text[start..=end];
            if let Ok(v) = serde_json::from_str::<Value>(candidate) {
                return Some(v);
            }
            // Try fixing common issues: trailing commas, single quotes
            let fixed = candidate
                .replace(",}", "}")
                .replace(",]", "]")
                .replace("'", "\"");
            if let Ok(v) = serde_json::from_str::<Value>(&fixed) {
                return Some(v);
            }
        }
    }

    // Last resort: try to build JSON from the raw text for known fields
    let text_lower = text.to_lowercase();
    // For slug suggestions
    if text_lower.contains("slug") || text.contains('/') || text.contains('-') {
        let clean = text.trim().trim_matches('"').trim_matches('\'');
        // If it looks like a slug (lowercase, hyphens, no spaces)
        let slug_candidate = clean.split_whitespace().last().unwrap_or(clean);
        if slug_candidate.contains('-')
            && !slug_candidate.contains(' ')
            && slug_candidate.len() < 100
        {
            return Some(
                json!({"slug": slug_candidate.trim_matches('"').trim_matches('.').to_lowercase()}),
            );
        }
    }
    // For title suggestions
    if !text.contains('{') && text.len() < 200 {
        let clean = text.trim().trim_matches('"').trim_matches('\'').trim();
        if !clean.is_empty() {
            // Check if it looks like a title (not code, not too long)
            let first_line = clean.lines().next().unwrap_or(clean).trim();
            if !first_line.is_empty() && first_line.len() < 150 {
                return Some(json!({"title": first_line}));
            }
        }
    }
    // For meta suggestions — look for lines with "title" and "description"
    if text_lower.contains("title") && text_lower.contains("description") {
        let mut meta_title = String::new();
        let mut meta_desc = String::new();
        for line in text.lines() {
            let l = line.trim().to_lowercase();
            if l.starts_with("title") || l.starts_with("meta title") || l.starts_with("meta_title")
            {
                meta_title = line
                    .split(':')
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(":")
                    .trim()
                    .trim_matches('"')
                    .to_string();
            }
            if l.starts_with("description")
                || l.starts_with("meta description")
                || l.starts_with("meta_description")
            {
                meta_desc = line
                    .split(':')
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(":")
                    .trim()
                    .trim_matches('"')
                    .to_string();
            }
        }
        if !meta_title.is_empty() || !meta_desc.is_empty() {
            return Some(json!({"meta_title": meta_title, "meta_description": meta_desc}));
        }
    }
    // For tag suggestions — look for comma-separated or bulleted lists
    if text_lower.contains("tag") {
        let mut tags: Vec<String> = Vec::new();
        for line in text.lines() {
            let l = line.trim();
            // Skip header lines
            if l.to_lowercase().starts_with("tag") && l.contains(':') {
                let after_colon = l.split(':').skip(1).collect::<Vec<_>>().join(":");
                for t in after_colon.split(',') {
                    let t = t
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_matches('[')
                        .trim_matches(']')
                        .trim();
                    if !t.is_empty() && t.len() < 50 {
                        tags.push(t.to_string());
                    }
                }
                continue;
            }
            // Bulleted items
            let stripped = l
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim_start_matches("• ")
                .trim();
            if !stripped.is_empty()
                && stripped.len() < 50
                && !stripped.to_lowercase().starts_with("tag")
            {
                tags.push(stripped.trim_matches('"').to_string());
            }
        }
        if !tags.is_empty() {
            return Some(json!({"tags": tags}));
        }
    }

    log::warn!(
        "Failed to parse AI response as JSON: {}",
        &text[..text.len().min(300)]
    );
    None
}

// ── Route Registration ────────────────────────────────

pub fn routes() -> Vec<rocket::Route> {
    routes![
        suggest::suggest_all,
        suggest::suggest_meta,
        suggest::suggest_tags,
        suggest::suggest_categories,
        suggest::suggest_slug,
        suggest::suggest_alt_text,
        suggest::suggest_title,
        generate::generate_post,
        generate::suggest_content,
        generate::inline_assist,
        generate::describe_image,
        status::ai_status,
    ]
}
