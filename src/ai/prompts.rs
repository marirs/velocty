/// System prompt for SEO suggestions
pub fn seo_system() -> String {
    "You are an SEO expert assistant for a CMS. You provide concise, actionable suggestions. \
     Always respond in valid JSON format as specified. Do not include markdown fences or explanations outside the JSON."
        .to_string()
}

/// Suggest meta title and description for content
pub fn suggest_meta(title: &str, content_excerpt: &str, content_type: &str) -> String {
    format!(
        "Given this {} titled \"{}\" with content excerpt:\n\n{}\n\n\
         Generate an SEO-optimized meta title (≤60 chars) and meta description (120-155 chars).\n\
         Respond as JSON: {{\"meta_title\": \"...\", \"meta_description\": \"...\"}}",
        content_type, title, content_excerpt
    )
}

/// Suggest tags for content
pub fn suggest_tags(title: &str, content_excerpt: &str, existing_tags: &[String]) -> String {
    let existing = if existing_tags.is_empty() {
        "none".to_string()
    } else {
        existing_tags.join(", ")
    };
    format!(
        "Given content titled \"{}\" with excerpt:\n\n{}\n\n\
         Existing tags in the system: {}\n\n\
         Suggest 3-6 relevant tags. Prefer reusing existing tags when appropriate. \
         Respond as JSON: {{\"tags\": [\"tag1\", \"tag2\", ...]}}",
        title, content_excerpt, existing
    )
}

/// Suggest categories for content
pub fn suggest_categories(
    title: &str,
    content_excerpt: &str,
    existing_categories: &[String],
) -> String {
    let existing = if existing_categories.is_empty() {
        "none".to_string()
    } else {
        existing_categories.join(", ")
    };
    format!(
        "Given content titled \"{}\" with excerpt:\n\n{}\n\n\
         Existing categories: {}\n\n\
         Suggest 1-3 relevant categories. Prefer reusing existing categories when appropriate. \
         Respond as JSON: {{\"categories\": [\"cat1\", \"cat2\"]}}",
        title, content_excerpt, existing
    )
}

/// Suggest a URL slug
pub fn suggest_slug(title: &str) -> String {
    format!(
        "Generate an SEO-friendly URL slug for the title: \"{}\"\n\
         Rules: lowercase, hyphens only, no stop words, 3-6 words max.\n\
         Respond as JSON: {{\"slug\": \"...\"}}",
        title
    )
}

/// Suggest alt text for an image
pub fn suggest_alt_text(context: &str, image_filename: &str) -> String {
    format!(
        "Generate descriptive alt text for an image.\n\
         Context: {}\nFilename: {}\n\n\
         The alt text should be concise (≤125 chars), descriptive, and accessible.\n\
         Respond as JSON: {{\"alt_text\": \"...\"}}",
        context, image_filename
    )
}

/// Describe an image for content generation
pub fn describe_image() -> String {
    "Describe this image in detail. What is shown? What is the subject, setting, mood, colors, and style? \
     Be specific and descriptive in 2-3 sentences.\n\
     Respond as JSON: {\"description\": \"...\"}"
        .to_string()
}

/// Suggest a title from a description or image description
pub fn suggest_title(description: &str) -> String {
    format!(
        "Given this description:\n\n{}\n\n\
         Generate a compelling, SEO-friendly title (≤70 chars).\n\
         Respond as JSON: {{\"title\": \"...\"}}",
        description
    )
}

/// Generate a blog post from a description
pub fn generate_post(description: &str) -> String {
    format!(
        "Write a blog post based on this description:\n\n{}\n\n\
         Requirements:\n\
         - Use HTML formatting (h2, h3, p, ul/li, strong, em)\n\
         - Include an engaging introduction\n\
         - 3-5 sections with subheadings (h2)\n\
         - 600-1200 words\n\
         - Professional but approachable tone\n\
         - Do NOT include an h1 tag (the title is separate)\n\n\
         Also suggest a title, excerpt (1-2 sentences), and 3-5 tags.\n\n\
         Respond as JSON:\n\
         {{\"title\": \"...\", \"content_html\": \"...\", \"excerpt\": \"...\", \"tags\": [\"...\"]}}",
        description
    )
}

/// Inline assist: transform selected text
pub fn inline_assist(action: &str, selected_text: &str) -> String {
    let instruction = match action {
        "expand" => "Expand this text with more detail, examples, and depth. Keep the same tone and style. Return HTML.",
        "rewrite" => "Rewrite this text to improve clarity, flow, and readability. Keep the same meaning and length. Return HTML.",
        "summarise" => "Summarise this text into a concise version, keeping only the key points. Return HTML.",
        "continue" => "Continue writing from where this text ends, maintaining the same tone, style, and topic. Write 2-3 additional paragraphs. Return HTML.",
        "formal" => "Rewrite this text in a more formal, professional tone. Return HTML.",
        "casual" => "Rewrite this text in a more casual, conversational tone. Return HTML.",
        _ => "Improve this text. Return HTML.",
    };
    format!(
        "{}\n\nText:\n{}\n\nRespond as JSON: {{\"html\": \"...\"}}",
        instruction, selected_text
    )
}
