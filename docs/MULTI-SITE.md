# Multi-Site / Multi-Tenancy Architecture

**Feature flag:** `multi-site`

```bash
# Single-site (default, current behavior)
cargo build --release

# Multi-site enabled
cargo build --release --features multi-site
```

---

## Overview

When compiled with `--features multi-site`, Velocty becomes a multi-tenant CMS where a single binary serves multiple independent sites. Each site has its own:

- Database (SQLite file or MongoDB database — chosen during first-run setup)
- Uploads folder
- Admin panel (per-site admin)
- Settings, content, themes, analytics

A **Super Admin** panel manages all sites from a central dashboard.

### Database Backend

The database backend is chosen during the first-run setup wizard and stored in `velocty.toml`. Both backends are fully supported in multi-site mode:

| | SQLite | MongoDB |
|---|---|---|
| **Per-site storage** | `website/sites/<uuid>/db/velocty.db` | `velocty_site_<uuid>` database per site |
| **Central registry** | `website/sites.db` (always SQLite) | `website/sites.db` (always SQLite) |
| **Isolation** | Separate files per site | Separate databases per site |
| **Backup** | Copy individual `.db` files | `mongodump --db velocty_site_<uuid>` |
| **Best for** | Small deployments, few sites | Production, many sites, high availability |

MongoDB is especially compelling for multi-site because:
- Each site becomes a separate MongoDB database — clean isolation without filesystem management
- Replica sets provide automatic failover across all sites
- MongoDB Atlas allows fully managed cloud hosting
- No risk of accidental file deletion destroying a site

---

## Architecture

### Storage Layout

#### Single-Site Mode (default)

Site-specific data lives under `website/site/`, keeping it separate from shared assets:

```
website/
├── site/                       # All site-specific data
│   ├── db/velocty.db           # SQLite database
│   ├── uploads/                # User uploads
│   └── designs/                # Saved page designs
├── templates/                  # Shared Tera templates
└── static/                     # Shared static assets (CSS, JS, TinyMCE)
```

#### Multi-Site Mode (`--features multi-site`)

Site folders use **random UUIDs** so the filesystem doesn't reveal which database belongs to which site. Only `sites.db` knows the mapping.

```
website/
├── sites.db                    # Central registry (super-admin, site list, hostname→UUID mapping)
├── sites/
│   ├── a3f7c2e1-9b4d-4e8a-b6f0-1234abcd5678/
│   │   ├── db/velocty.db       # Site-specific database
│   │   ├── uploads/            # Site-specific uploads
│   │   └── designs/            # Site-specific designs
│   ├── e8b12f4a-7c3d-41a9-9e5f-abcdef012345/
│   │   ├── db/velocty.db
│   │   ├── uploads/
│   │   └── designs/
│   └── ...
├── templates/                  # Shared Tera templates
└── static/                     # Shared static assets (CSS, JS, TinyMCE)
```

Note: each site under `sites/<uuid>/` has the same internal structure as `site/` in single-site mode. This makes migration seamless.

### Central Registry (`sites.db`)

```sql
CREATE TABLE sites (
    id INTEGER PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,          -- random UUID (opaque folder name)
    hostname TEXT UNIQUE NOT NULL,      -- "example.com" (Host header match)
    display_name TEXT NOT NULL,         -- "Example Site"
    status TEXT NOT NULL DEFAULT 'active',  -- active, suspended, maintenance
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE super_admins (
    id INTEGER PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE super_sessions (
    token TEXT PRIMARY KEY,
    admin_id INTEGER NOT NULL,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (admin_id) REFERENCES super_admins(id)
);
```

### Request Flow

```
Request (Host: example.com)
    │
    ▼
┌───────────────────────────────────┐
│  SiteResolver Fairing             │
│  1. Read Host header              │
│  2. Lookup in sites.db            │
│  3. Check site status             │
│  4. Get/create Arc<dyn Store>     │
│     via SiteStoreManager          │
│  5. Inject SiteContext            │
└───────────────────────────────────┘
    │
    ▼
┌───────────────────────────────────┐
│  Route Handler                     │
│  Uses SiteContext.store            │
│  (Arc<dyn Store> — backend-agnostic)│
└───────────────────────────────────┘
```

### Key Types

```rust
/// Site record from the central registry
pub struct Site {
    pub id: i64,
    pub slug: String,           // random UUID (opaque folder name)
    pub hostname: String,       // "example.com" (Host header match)
    pub display_name: String,   // "Example Site"
    pub status: String,         // active, suspended, maintenance
    pub created_at: String,
    pub updated_at: String,
}

/// Injected per-request based on Host header
pub struct SiteContext {
    pub site: Site,             // Full site record from registry
    pub store: Arc<dyn Store>,  // Site-specific Store (SQLite or MongoDB)
    pub uploads_dir: String,    // "website/sites/<uuid>/uploads"
}

/// Manages per-site Store instances (cached, not re-created per request)
pub struct SiteStoreManager {
    stores: DashMap<String, Arc<dyn Store>>,  // site slug (UUID) -> Store
}

/// Legacy: manages per-site SQLite pools for health tools only
pub struct SitePoolManager {
    pools: DashMap<String, DbPool>,  // site slug (UUID) -> pool (SQLite only)
}
```

### Per-Site Store Creation

`create_site_store(slug)` reads `velocty.toml` to determine the backend:

```rust
fn create_site_store(slug: &str) -> Result<Arc<dyn Store>, String> {
    match read_backend() {
        "mongodb" => {
            // Each site gets its own MongoDB database: velocty_site_<slug>
            let db_name = format!("velocty_site_{}", slug);
            Ok(Arc::new(MongoStore::new(uri, &db_name)?))
        }
        _ => {
            // Each site gets its own SQLite file
            let db_path = format!("website/sites/{}/db/velocty.db", slug);
            Ok(Arc::new(SqliteStore::new(init_pool_at(&db_path)?)))
        }
    }
}
```

---

## Super Admin Panel

### Routes

All super-admin routes are behind `#[cfg(feature = "multi-site")]`.

| Route | Page |
|---|---|
| `/super/setup` | First-run setup (create super admin account) |
| `/super/login` | Super admin login |
| `/super/logout` | Logout |
| `/super/` | Dashboard — list all sites with status |
| `/super/health` | System health — disk, resources, DB, filesystem, sites overview |
| `/super/settings` | Super admin settings (email, password, global defaults) |
| `/super/sites/new` | Create new site |
| `/super/sites/<id>` | Edit site (hostname, display name, status) |
| `/super/sites/<id>/delete` | Delete site (with confirmation) |
| `POST /super/health/tool/<site_id>/<tool>` | Run a maintenance tool on a specific site (see below) |

### Navigation

The Super Admin topbar includes navigation links:

| Menu | Description |
|---|---|
| **Sites** | Site CRUD — list, create, edit, delete, status toggle |
| **Health** | System-level health dashboard (same data as per-site health, aggregated) |
| **Settings** | Super admin account management and global defaults |

### Health Page

The Super Admin health page (`/super/health`) shows system-wide health information:

- **Disk** — total/free space, uploads size, file count
- **Resources** — uptime, memory RSS, OS, process user, Velocty version
- **Database** — backend-aware (SQLite: file size, WAL, fragmentation, integrity; MongoDB: connection status, latency, URI)
- **Filesystem** — permission checks with owner:group, recommended vs actual perms, world-writable detection, root ownership warnings
- **Sites Overview** — table of all registered sites with hostname and status

The health data is gathered by `src/health.rs` which reads `velocty.toml` to detect the database backend and adapts checks accordingly.

### Maintenance Tools

The health page includes a **Maintenance Tools** section with a site selector dropdown. The Super Admin selects a target site, then runs any tool — it operates on that site's database and uploads directory.

Available tools (per-site):

| Tool | SQLite | MongoDB | Route suffix |
|---|---|---|---|
| **Integrity Check** | ✓ | — | `/integrity-check` |
| **Vacuum** | ✓ | — | `/vacuum` |
| **WAL Checkpoint** | ✓ | — | `/wal-checkpoint` |
| **Session Cleanup** | ✓ | ✓ | `/session-cleanup` |
| **Orphan File Scan** | ✓ | ✓ | `/orphan-scan` |
| **Delete Orphan Files** | ✓ | ✓ | `/orphan-delete` |
| **Unused Tags Cleanup** | ✓ | ✓ | `/unused-tags` |
| **Export Content** | ✓ | ✓ | `/export-content` |

Each tool POSTs to `/super/health/tool/<site_id>/<tool>`. The route resolves the site ID to its slug via the registry, gets the site's `Arc<dyn Store>` from `SiteStoreManager` (or `SitePoolManager` for SQLite-specific tools like vacuum). Orphan scan/delete use the per-site uploads path (`website/sites/<uuid>/uploads`).

Per-site admin tools (`/<admin_slug>/health`) work identically in both single-site and multi-site modes — they always operate on the current site.

### Dashboard

```
┌─────────────────────────────────────────────────────────┐
│  Velocty Super Admin                                    │
│                                                         │
│  Sites (3)                              [+ New Site]    │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Site              Hostname          Status       │  │
│  │  ─────────────────────────────────────────────    │  │
│  │  My Portfolio      example.com       ● Active     │  │
│  │  Client Blog       blog.client.com   ● Active     │  │
│  │  Test Site         test.local        ○ Suspended  │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  Click a site to manage, or use its admin panel         │
│  directly at https://<hostname>/<admin-slug>            │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Create New Site

1. Enter hostname (e.g., `blog.example.com`)
2. Enter display name
3. System generates a random UUID as the internal folder name (e.g., `a3f7c2e1-...`)
4. Creates folder structure: `website/sites/<uuid>/db/`, `uploads/`, `designs/`
5. Stores the `hostname → uuid` mapping in `sites.db`
6. Runs migrations + seeds on the new site DB
7. Redirects to site admin setup at `https://<hostname>/<admin-slug>/setup`

---

## Routing Strategy

### Option A: Host-based (Recommended)

Each site has its own domain/subdomain. The `SiteResolver` fairing matches on `Host` header.

- `example.com` → Site A
- `blog.example.com` → Site B
- `another.com` → Site C

### Option B: Path-based (Alternative)

All sites share one domain, differentiated by path prefix:

- `cms.example.com/site-a/` → Site A
- `cms.example.com/site-b/` → Site B

Host-based is cleaner and recommended. Path-based can be added later if needed.

---

## Static Assets & Templates

**Shared across all sites:**
- `/static/` — CSS, JS, TinyMCE, images (served once)
- `website/templates/` — Tera templates (admin + visitor)

**Per-site:**
- `/uploads/` — Rewritten by fairing to serve from `website/sites/<uuid>/uploads/`
- Designs — loaded from `website/sites/<uuid>/designs/`

---

## Migration Path

Migration from single-site to multi-site is **fully automatic**:

### Automatic Migration Flow

```
Old flat layout (pre-migration)     Boot auto-migration          Enable multi-site
website/db/velocty.db          →  website/site/db/velocty.db  →  website/sites/<uuid>/db/velocty.db
website/uploads/               →  website/site/uploads/       →  website/sites/<uuid>/uploads/
website/designs/               →  website/site/designs/       →  website/sites/<uuid>/designs/
```

### Step 1: Boot Migration (automatic, single-site)

On every startup, `boot::migrate_to_site_layout()` checks if the old flat layout exists (`website/db/velocty.db`). If so, it moves `db/`, `uploads/`, and `designs/` into `website/site/`. This is idempotent — it only runs once.

### Step 2: Multi-Site Migration (automatic)

1. Recompile with `--features multi-site`
2. Run the binary
3. `site::migrate_single_to_multi()` detects `website/site/` and automatically:
   - Generates a random UUID
   - Moves `website/site/` → `website/sites/<uuid>/`
   - Registers the site in `sites.db` with the hostname
4. Creates super-admin setup at `/super/setup`
5. All existing data is preserved — zero manual intervention

---

## Feature Flag Boundaries

| Component | Single-site | Multi-site |
|---|---|---|
| `src/main.rs` | `Arc<dyn Store>` in state | + `SiteStoreManager` + `SiteResolver` fairing |
| `src/store/` | Single Store instance | `create_site_store()` → per-site Store |
| `src/routes/super_admin/` | Not compiled | Full super-admin routes |
| `src/site.rs` | Not compiled | `SiteContext`, `SiteStoreManager`, `SiteResolver` |
| Route handlers | `store: &State<Arc<dyn Store>>` | `site: SiteContext` (via request guard) |
| Templates | No change | `super/` templates added |
| Uploads FileServer | Global `/uploads/` | Per-site via fairing rewrite |

---

## Dependencies (Multi-site only)

```toml
[dependencies]
dashmap = { version = "5", optional = true }

[features]
multi-site = ["dashmap"]
```

`DashMap` provides a concurrent hashmap for caching per-site `Arc<dyn Store>` instances without a mutex bottleneck.
