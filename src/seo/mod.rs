pub mod analytics;
pub mod jsonld;
pub mod meta;
pub mod sitemap;
pub mod webmaster;

// Re-export commonly used functions
pub use analytics::build_analytics_scripts;
pub use meta::build_meta;
pub use webmaster::build_webmaster_meta;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
