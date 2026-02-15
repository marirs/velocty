# Firewall Module — Specification

## Overview
A built-in application firewall for Velocty. When enabled, it adds a "Firewall" item to the admin left menu with an operational dashboard, and configurable protection rules under Settings > Security > Firewall.

## Architecture
- **Settings UI**: Sub-tab under Settings > Security
- **Dashboard**: Left menu item (visible only when `firewall_enabled = true`)
- **Middleware**: Rocket fairing checked on every request
- **Storage**: Two SQLite tables (`fw_bans`, `fw_events`)
- **GeoIP**: MaxMind GeoLite2 Country `.mmdb` file (optional, user-provided)

---

## Settings (28 total)

### Master
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `firewall_enabled` | bool | `false` | Master switch; shows/hides left menu item |

### Bot Detection
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_monitor_bots` | bool | `true` | Log bot fingerprints (UA, rate, patterns) |
| `fw_bot_auto_ban` | bool | `false` | Auto-ban after threshold |
| `fw_bot_ban_threshold` | int | `10` | Suspicious requests before ban |
| `fw_bot_ban_duration` | select | `24h` | 1h / 6h / 24h / 7d / 30d / permanent |

### Login Protection
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_failed_login_tracking` | bool | `true` | Log failed attempts with IP/UA/username |
| `fw_failed_login_ban_threshold` | int | `5` | Failed attempts before ban |
| `fw_failed_login_ban_duration` | select | `1h` | Ban duration after threshold |
| `fw_ban_unknown_users` | bool | `false` | Auto-ban if username doesn't exist in user directory |
| `fw_unknown_user_ban_duration` | select | `24h` | Ban duration for unknown user attempts |

### Injection Protection (OWASP)
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_xss_protection` | bool | `true` | Detect script tags, event handlers, data URIs |
| `fw_sqli_protection` | bool | `true` | Detect SQL injection patterns |
| `fw_path_traversal_protection` | bool | `true` | Block ../, %2e%2e, null bytes |
| `fw_csrf_strict` | bool | `true` | Enforce origin/referer on state-changing requests |
| `fw_injection_ban_duration` | select | `7d` | Any injection attempt = immediate ban |

### Rate Limiting
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_rate_limit_enabled` | bool | `true` | Global request rate limiting |
| `fw_rate_limit_requests` | int | `100` | Requests per window |
| `fw_rate_limit_window` | select | `60s` | 10s / 30s / 60s / 5m |
| `fw_rate_limit_ban_duration` | select | `1h` | Ban duration after exceeding |

### Payment Endpoint Protection
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_payment_abuse_detection` | bool | `true` | Monitor payment endpoints |
| `fw_payment_ban_threshold` | int | `3` | Failed/suspicious payment attempts |
| `fw_payment_ban_duration` | select | `30d` | Ban duration |

### Country Blocking
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_geo_blocking_enabled` | bool | `false` | Master toggle for geo features |
| `fw_geo_block_visitors` | bool | `true` | Block from public pages |
| `fw_geo_block_admin` | bool | `true` | Block from admin panel |
| `fw_geo_blocked_countries` | text | `""` | Comma-separated ISO 3166-1 alpha-2 codes |
| `fw_geo_allowed_countries` | text | `""` | Whitelist mode (overrides block list if set) |

### Security Headers
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fw_security_headers` | bool | `true` | X-Frame-Options, X-Content-Type-Options, CSP, HSTS, Referrer-Policy, Permissions-Policy |

---

## Database Tables

```sql
CREATE TABLE IF NOT EXISTS fw_bans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ip TEXT NOT NULL,
    reason TEXT NOT NULL,       -- 'bot', 'login', 'injection', 'rate_limit', 'payment', 'geo', 'manual'
    detail TEXT,                -- JSON: username tried, pattern matched, country, etc.
    banned_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,            -- NULL = permanent
    country TEXT,
    user_agent TEXT,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_fw_bans_ip ON fw_bans(ip);
CREATE INDEX idx_fw_bans_active ON fw_bans(active);

CREATE TABLE IF NOT EXISTS fw_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ip TEXT NOT NULL,
    event_type TEXT NOT NULL,   -- 'bot_hit', 'failed_login', 'injection', 'rate_exceeded', 'payment_abuse', 'banned', 'geo_blocked'
    detail TEXT,                -- JSON blob
    country TEXT,
    user_agent TEXT,
    request_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_fw_events_ip ON fw_events(ip);
CREATE INDEX idx_fw_events_type ON fw_events(event_type);
CREATE INDEX idx_fw_events_created ON fw_events(created_at);
```

Auto-prune: DELETE oldest rows when count exceeds 10,000.

---

## Dashboard (Left Menu)

Visible only when `firewall_enabled = true`.

### Sections
1. **Overview** — active bans count, events/24h, top 10 offending IPs, blocked countries
2. **Event Log** — filterable table (type, IP, country, date range), paginated
3. **Ban List** — active + expired, manual add/remove, bulk unban
4. **Country Stats** — requests by country, blocked count (requires GeoIP DB)

### Left Menu Structure
```
🔥 Firewall
   ├─ Overview
   ├─ Event Log
   ├─ Ban List
   └─ Country Stats
```

---

## Middleware (Rocket Fairing)

Execution order on every request:
1. **IP extraction** — from X-Forwarded-For / X-Real-IP / peer addr
2. **Ban check** — lookup IP in `fw_bans` (in-memory cache, refreshed every 60s)
3. **Country check** — GeoIP lookup → check against block/allow lists
4. **Rate limit check** — increment counter, ban if exceeded
5. **Injection scan** — query params, form bodies, headers (compiled regex via lazy_static)
6. **Log event** if suspicious
7. **Return 403** if banned, with minimal response body

---

## OWASP Top 10 Coverage

| # | Category | Coverage |
|---|----------|----------|
| A01 | Broken Access Control | Auth middleware on admin routes + horizontal privilege detection |
| A02 | Cryptographic Failures | HTTPS redirect when site_url is https |
| A03 | Injection | XSS, SQLi, path traversal filters |
| A04 | Insecure Design | Rate limit on auth/reset/payment endpoints |
| A05 | Security Misconfiguration | Security headers toggle |
| A06 | Vulnerable Components | N/A (compiled Rust, no plugins) |
| A07 | Auth Failures | Login protection, unknown user banning, session management |
| A08 | Data Integrity | CSP to prevent inline script injection |
| A09 | Logging Failures | Event log + dashboard |
| A10 | SSRF | URL sanitization on user-provided URLs |

---

## Implementation Phases

| Phase | Scope | Estimated Effort |
|-------|-------|-----------------|
| **Phase 1** | Settings UI, DB tables, ban middleware, login protection, manual ban/unban | Foundation |
| **Phase 2** | Bot detection, injection filters (XSS/SQLi/path traversal), rate limiting | Core protection |
| **Phase 3** | Payment abuse, country blocking, GeoIP integration, security headers fairing | Extended protection |
| **Phase 4** | Dashboard UI (overview, event log, ban list, country stats) | Operational visibility |

---

## Dependencies
- `maxminddb` — GeoIP lookups (optional, only if user provides .mmdb file)
- `lazy_static` or `once_cell` — compiled regex patterns for injection detection
- `chrono` — already in use, for ban expiry calculations

## GeoIP Setup
User downloads MaxMind GeoLite2 Country database (free with registration).
Place `GeoLite2-Country.mmdb` in the Velocty data directory.
If file is absent, all geo features are silently disabled.
