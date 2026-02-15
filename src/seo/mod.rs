pub mod meta;
pub mod jsonld;
pub mod sitemap;
pub mod analytics;
pub mod webmaster;

// Re-export commonly used functions
pub use meta::build_meta;
pub use jsonld::{build_post_jsonld, build_portfolio_jsonld};
pub use sitemap::generate_sitemap;
pub use analytics::build_analytics_scripts;
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
