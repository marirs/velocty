use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;

/// Dangerous SVG elements that can execute scripts or load external content
const DANGEROUS_ELEMENTS: &[&str] = &[
    "script",
    "foreignobject",
    "iframe",
    "embed",
    "object",
    "applet",
    "set",
    "animate",
    "animatetransform",
    "animatemotion",
    "handler",
    "listener",
];

/// Dangerous attribute prefixes (event handlers)
const DANGEROUS_ATTR_PREFIXES: &[&str] = &["on"];

/// Dangerous URI schemes in href/xlink:href/src attributes
const DANGEROUS_SCHEMES: &[&str] = &[
    "javascript:",
    "data:text/html",
    "data:application",
    "vbscript:",
];

/// Attributes that can contain URIs and need scheme checking
const URI_ATTRIBUTES: &[&str] = &["href", "xlink:href", "src", "action", "formaction"];

/// Sanitize an SVG file by removing dangerous elements and attributes.
/// Returns the sanitized SVG bytes, or None if parsing fails.
pub fn sanitize_svg(input: &[u8]) -> Option<Vec<u8>> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(false);

    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut skip_depth: usize = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let tag_name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }

                if is_dangerous_element(&tag_name) {
                    skip_depth = 1;
                    continue;
                }

                // Clean attributes
                let cleaned = clean_attributes(e);
                writer.write_event(Event::Start(cleaned)).ok()?;
            }
            Ok(Event::End(ref e)) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                writer.write_event(Event::End(e.to_owned())).ok()?;
            }
            Ok(Event::Empty(ref e)) => {
                if skip_depth > 0 {
                    continue;
                }

                let tag_name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if is_dangerous_element(&tag_name) {
                    continue;
                }

                // For <use> elements, check if href points to external resource
                if tag_name == "use" && has_external_use_href(e) {
                    continue;
                }

                let cleaned = clean_attributes(e);
                writer.write_event(Event::Empty(cleaned)).ok()?;
            }
            Ok(Event::Text(ref e)) => {
                if skip_depth > 0 {
                    continue;
                }
                writer.write_event(Event::Text(e.to_owned())).ok()?;
            }
            Ok(Event::CData(ref e)) => {
                if skip_depth > 0 {
                    continue;
                }
                writer.write_event(Event::CData(e.to_owned())).ok()?;
            }
            Ok(Event::Comment(_)) => {
                // Strip comments — they can contain IE conditional tricks
                continue;
            }
            Ok(Event::Decl(ref e)) => {
                writer.write_event(Event::Decl(e.to_owned())).ok()?;
            }
            Ok(Event::PI(_)) => {
                // Strip processing instructions
                continue;
            }
            Ok(Event::DocType(_)) => {
                // Strip DOCTYPE — can reference external entities
                continue;
            }
            Err(_) => return None,
        }
    }

    Some(writer.into_inner().into_inner())
}

fn is_dangerous_element(tag: &str) -> bool {
    DANGEROUS_ELEMENTS.contains(&tag)
}

fn is_dangerous_attribute(name: &str) -> bool {
    let lower = name.to_lowercase();
    for prefix in DANGEROUS_ATTR_PREFIXES {
        if lower.starts_with(prefix) && lower.len() > prefix.len() {
            // "on" + at least one more char = event handler (onclick, onload, etc.)
            return true;
        }
    }
    false
}

fn has_dangerous_uri(value: &str) -> bool {
    let trimmed = value.trim().to_lowercase();
    DANGEROUS_SCHEMES
        .iter()
        .any(|scheme| trimmed.starts_with(scheme))
}

fn has_external_use_href(e: &BytesStart) -> bool {
    for attr in e.attributes().flatten() {
        let name = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
        let lower = name.to_lowercase();
        if lower == "href" || lower == "xlink:href" {
            let val = std::str::from_utf8(&attr.value).unwrap_or("");
            // External if starts with http://, https://, or // (protocol-relative)
            let trimmed = val.trim();
            if trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || trimmed.starts_with("//")
            {
                return true;
            }
        }
    }
    false
}

fn clean_attributes(e: &BytesStart) -> BytesStart<'static> {
    let mut cleaned = BytesStart::new(
        std::str::from_utf8(e.name().as_ref())
            .unwrap_or("g")
            .to_string(),
    );

    for attr in e.attributes().flatten() {
        let name = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
        let value = std::str::from_utf8(&attr.value).unwrap_or("");

        // Skip event handler attributes
        if is_dangerous_attribute(name) {
            continue;
        }

        // Check URI attributes for dangerous schemes
        let lower_name = name.to_lowercase();
        if URI_ATTRIBUTES.contains(&lower_name.as_str()) && has_dangerous_uri(value) {
            continue;
        }

        // Check style attribute for url() with dangerous schemes
        if lower_name == "style" {
            let lower_val = value.to_lowercase();
            if lower_val.contains("javascript:") || lower_val.contains("expression(") {
                continue;
            }
        }

        cleaned.push_attribute((name, value));
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_svg_passes_through() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="40" fill="red"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(out.contains("<circle"));
        assert!(out.contains("fill=\"red\""));
    }

    #[test]
    fn test_strips_script_element() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert('xss')</script><circle cx="50" cy="50" r="40"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("<script"));
        assert!(!out.contains("alert"));
        assert!(out.contains("<circle"));
    }

    #[test]
    fn test_strips_event_handlers() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><circle cx="50" cy="50" r="40" onload="alert(1)" onclick="evil()"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("onload"));
        assert!(!out.contains("onclick"));
        assert!(out.contains("<circle"));
    }

    #[test]
    fn test_strips_javascript_href() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><a href="javascript:alert(1)"><text>click</text></a></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn test_strips_foreignobject() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body xmlns="http://www.w3.org/1999/xhtml"><script>alert(1)</script></body></foreignObject></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("foreignObject"));
        assert!(!out.contains("foreignobject"));
        assert!(!out.contains("<script"));
    }

    #[test]
    fn test_strips_external_use() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><use href="https://evil.com/sprite.svg#icon"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("evil.com"));
    }

    #[test]
    fn test_allows_internal_use() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><circle id=\"c\" cx=\"50\" cy=\"50\" r=\"40\"/></defs><use href=\"#c\"/></svg>";
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(out.contains("use"));
        assert!(out.contains("#c"));
    }

    #[test]
    fn test_strips_data_uri_href() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><a href="data:text/html,<script>alert(1)</script>"><text>x</text></a></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("data:text/html"));
    }

    #[test]
    fn test_strips_style_expression() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><rect style="width:expression(alert(1))" width="100" height="100"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("expression("));
    }

    #[test]
    fn test_strips_comments() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><!-- [if IE]><script>alert(1)</script><![endif] --><circle cx="50" cy="50" r="40"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("<!--"));
        assert!(!out.contains("alert"));
        assert!(out.contains("<circle"));
    }

    #[test]
    fn test_strips_iframe_element() {
        let svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><iframe src="https://evil.com"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("iframe"));
    }

    #[test]
    fn test_strips_embed_element() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><embed src="evil.swf"/></svg>"#;
        let result = sanitize_svg(svg).unwrap();
        let out = String::from_utf8(result).unwrap();
        assert!(!out.contains("embed"));
    }
}
