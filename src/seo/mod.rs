pub mod analytics;
pub mod jsonld;
pub mod meta;
pub mod sitemap;
pub mod webmaster;

// Re-export commonly used functions
pub use analytics::build_analytics_scripts;
#[allow(unused_imports)]
pub use jsonld::{build_portfolio_jsonld, build_post_jsonld};
pub use meta::build_meta;
#[allow(unused_imports)]
pub use sitemap::generate_sitemap;
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
