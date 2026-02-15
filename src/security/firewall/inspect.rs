/// XSS pattern detection
pub fn contains_xss(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("<script") ||
    lower.contains("javascript:") ||
    lower.contains("onerror=") ||
    lower.contains("onload=") ||
    lower.contains("onmouseover=") ||
    lower.contains("onfocus=") ||
    lower.contains("onclick=") ||
    lower.contains("data:text/html") ||
    lower.contains("vbscript:") ||
    lower.contains("expression(") ||
    lower.contains("eval(")
}

/// SQL injection pattern detection
pub fn contains_sqli(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("union select") ||
    lower.contains("union all select") ||
    lower.contains("' or '1'='1") ||
    lower.contains("' or 1=1") ||
    lower.contains("\" or 1=1") ||
    lower.contains("or 1=1--") ||
    lower.contains("'; drop ") ||
    lower.contains("; drop ") ||
    lower.contains("1=1 --") ||
    lower.contains("/**/") ||
    lower.contains("char(") ||
    lower.contains("concat(") ||
    lower.contains("information_schema") ||
    lower.contains("sleep(") ||
    lower.contains("benchmark(") ||
    lower.contains("waitfor delay")
}

/// Path traversal pattern detection
pub fn contains_path_traversal(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("../") ||
    lower.contains("..\\") ||
    lower.contains("%2e%2e") ||
    lower.contains("%252e") ||
    lower.contains("%00") ||
    lower.contains("/etc/passwd") ||
    lower.contains("/proc/self") ||
    lower.contains("\\windows\\")
}

/// Suspicious bot user-agent detection
pub fn is_suspicious_bot(ua: &str) -> bool {
    if ua.is_empty() {
        return true;
    }
    let lower = ua.to_lowercase();
    lower.contains("sqlmap") ||
    lower.contains("nikto") ||
    lower.contains("nmap") ||
    lower.contains("masscan") ||
    lower.contains("zgrab") ||
    lower.contains("gobuster") ||
    lower.contains("dirbuster") ||
    lower.contains("wpscan") ||
    lower.contains("nuclei") ||
    lower.contains("httpx") ||
    lower.contains("python-requests") && lower.contains("scan") ||
    lower.starts_with("-") ||
    lower == "mozilla" ||
    lower == "curl" ||
    lower == "wget"
}
