use serde_json::Value;

use super::html_escape;

/// Build webmaster verification meta tags for all configured search engines.
pub fn build_webmaster_meta(settings: &Value) -> String {
    let get = |key: &str| -> &str {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
    };

    let mut meta = String::new();
    let verifications = [
        ("seo_google_verification", "google-site-verification"),
        ("seo_bing_verification", "msvalidate.01"),
        ("seo_yandex_verification", "yandex-verification"),
        ("seo_pinterest_verification", "p:domain_verify"),
        ("seo_baidu_verification", "baidu-site-verification"),
    ];

    for (key, name) in &verifications {
        let val = get(key);
        if !val.is_empty() {
            meta.push_str(&format!(
                r#"    <meta name="{}" content="{}">"#,
                name,
                html_escape(val)
            ));
            meta.push('\n');
        }
    }
    meta
}
