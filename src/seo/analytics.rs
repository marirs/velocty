use serde_json::Value;

use super::html_escape;

/// Build analytics script tags for all enabled third-party analytics providers.
/// When cookie consent is enabled, scripts are gated behind consent with
/// `type="text/plain" data-consent="analytics"`.
pub fn build_analytics_scripts(settings: &Value) -> String {
    let get = |key: &str| -> &str {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
    };
    let enabled = |key: &str| -> bool { get(key) == "true" };

    // When cookie consent is enabled, gate analytics behind consent
    let consent = enabled("cookie_consent_enabled");
    let (stag, stag_async) = if consent {
        (r#"<script type="text/plain" data-consent="analytics""#, r#"<script type="text/plain" data-consent="analytics""#)
    } else {
        ("<script", "<script async")
    };

    let mut scripts = String::new();

    // Google Analytics (GA4)
    if enabled("seo_ga_enabled") {
        let id = get("seo_ga_measurement_id");
        if !id.is_empty() {
            scripts.push_str(&format!(
                r#"{stag_async} src="https://www.googletagmanager.com/gtag/js?id={id}"></script>
{stag}>window.dataLayer=window.dataLayer||[];function gtag(){{dataLayer.push(arguments);}}gtag('js',new Date());gtag('config','{id}');</script>
"#,
                stag_async = stag_async,
                stag = stag,
                id = html_escape(id)
            ));
        }
    }

    // Plausible
    if enabled("seo_plausible_enabled") {
        let domain = get("seo_plausible_domain");
        let host = get("seo_plausible_host");
        let host = if host.is_empty() { "https://plausible.io" } else { host };
        if !domain.is_empty() {
            scripts.push_str(&format!(
                r#"{stag} defer data-domain="{domain}" src="{host}/js/script.js"></script>
"#,
                stag = stag,
                domain = html_escape(domain),
                host = html_escape(host),
            ));
        }
    }

    // Fathom
    if enabled("seo_fathom_enabled") {
        let site_id = get("seo_fathom_site_id");
        if !site_id.is_empty() {
            scripts.push_str(&format!(
                r#"{stag} src="https://cdn.usefathom.com/script.js" data-site="{id}" defer></script>
"#,
                stag = stag,
                id = html_escape(site_id)
            ));
        }
    }

    // Matomo
    if enabled("seo_matomo_enabled") {
        let url = get("seo_matomo_url");
        let site_id = get("seo_matomo_site_id");
        if !url.is_empty() && !site_id.is_empty() {
            scripts.push_str(&format!(
                r#"{stag}>var _paq=window._paq=window._paq||[];_paq.push(['trackPageView']);_paq.push(['enableLinkTracking']);(function(){{var u='{url}/';_paq.push(['setTrackerUrl',u+'matomo.php']);_paq.push(['setSiteId','{site_id}']);var d=document,g=d.createElement('script'),s=d.getElementsByTagName('script')[0];g.async=true;g.src=u+'matomo.js';s.parentNode.insertBefore(g,s);}})();</script>
"#,
                stag = stag,
                url = html_escape(url),
                site_id = html_escape(site_id),
            ));
        }
    }

    // Cloudflare Web Analytics
    if enabled("seo_cloudflare_analytics_enabled") {
        let token = get("seo_cloudflare_analytics_token");
        if !token.is_empty() {
            scripts.push_str(&format!(
                r#"{stag} defer src="https://static.cloudflareinsights.com/beacon.min.js" data-cf-beacon='{{"token":"{token}"}}'></script>
"#,
                stag = stag,
                token = html_escape(token)
            ));
        }
    }

    // Clicky
    if enabled("seo_clicky_enabled") {
        let site_id = get("seo_clicky_site_id");
        if !site_id.is_empty() {
            scripts.push_str(&format!(
                r#"{stag_async} data-id="{id}" src="//static.getclicky.com/js"></script>
"#,
                stag_async = stag_async,
                id = html_escape(site_id)
            ));
        }
    }

    // Umami
    if enabled("seo_umami_enabled") {
        let website_id = get("seo_umami_website_id");
        let host = get("seo_umami_host");
        let host = if host.is_empty() { "https://analytics.umami.is" } else { host };
        if !website_id.is_empty() {
            scripts.push_str(&format!(
                r#"{stag} defer src="{host}/script.js" data-website-id="{id}"></script>
"#,
                stag = stag,
                host = html_escape(host),
                id = html_escape(website_id),
            ));
        }
    }

    scripts
}
