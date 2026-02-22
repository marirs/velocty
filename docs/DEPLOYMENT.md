# Deployment Guide

How to deploy Velocty behind a reverse proxy (Nginx) for both single-site and multi-site setups.

---

## Single-Site Behind Nginx

A standard reverse proxy configuration is all you need:

```nginx
server {
    listen 80;
    server_name yourdomain.com;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Key Points

- **Site URL** — Set `site_url` in **Settings > General** to your public domain (e.g. `https://yourdomain.com`). This is used for SEO meta tags, RSS feeds, sitemaps, Open Graph URLs, and canonical links. It does not affect routing.
- **Routing** — Velocty binds to `0.0.0.0:8000` and responds to all incoming requests regardless of the `Host` header. Nginx simply proxies traffic to it.
- **HTTPS** — Handle TLS at the Nginx layer (e.g. with Let's Encrypt / certbot). Velocty serves plain HTTP internally. Pass `X-Forwarded-Proto` so Velocty knows the original scheme.
- **DNS** — Standard A/AAAA record pointing `yourdomain.com` to your server's IP. No special DNS configuration required.

### HTTPS with Let's Encrypt

```bash
sudo certbot --nginx -d yourdomain.com
```

Certbot will automatically update your Nginx config to handle TLS termination.

---

## Multi-Site Behind Nginx

With the `multi-site` feature, a single Velocty binary serves multiple independent sites. Each site has its own **domain** and its own **SQLite database**. Velocty uses the incoming `Host` header to route requests to the correct site.

### How It Works

1. The **super admin** panel (accessible at a designated domain) manages the site registry. Each entry has a `domain` and a `db_path`.
2. On every request, Velocty reads the `Host` header, looks it up in the registry, and loads that site's database.
3. Each site is fully independent — its own settings, posts, designs, users, uploads, etc.

### Nginx Configuration

All domains upstream to the same Velocty binary:

```nginx
upstream velocty {
    server 127.0.0.1:8000;
}

server {
    listen 80;
    server_name site-a.com site-b.com admin.example.com;

    location / {
        proxy_pass http://velocty;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Key Points

- **DNS** — Each domain needs an A/AAAA record pointing to the same server IP. No special DNS configuration beyond that.
- **Host header is critical** — `proxy_set_header Host $host;` is essential. Without it, Nginx sends `127.0.0.1` as the Host and Velocty cannot match the domain to a site.
- **One binary, one port** — All sites are served by the same process on the same port. Domain-based routing happens inside Velocty, not at the Nginx level.
- **Separate TLS certs** — Each domain needs its own certificate (or use a wildcard for subdomains). Certbot can handle multiple domains:
  ```bash
  sudo certbot --nginx -d site-a.com -d site-b.com -d admin.example.com
  ```
- **Per-site `site_url`** — Each site should set its own public domain in **Settings > General**.

---

## Quick Reference

| | Single-Site | Multi-Site |
|---|---|---|
| **Binary** | `velocty` | `velocty` (built with `--features multi-site`) |
| **DNS** | 1 domain → server IP | N domains → same server IP |
| **Nginx** | Standard reverse proxy | Same config, `server_name` lists all domains |
| **Host header** | Recommended | **Required** for domain routing |
| **Site URL setting** | Set to your domain | Each site sets its own domain |
| **Database** | Single `velocty.db` | One DB per site, managed by super admin |
| **Special config** | None | None beyond DNS + Host header |

---

## Running as a Systemd Service

### 1. Prepare the Environment

Create a dedicated user and directory:

```bash
# Create a system user (no login shell, no home dir)
sudo useradd --system --no-create-home --shell /usr/sbin/nologin velocty

# Create the application directory
sudo mkdir -p /opt/velocty
sudo mkdir -p /opt/velocty/uploads

# Copy the binary and website assets
sudo cp target/release/velocty /opt/velocty/
sudo cp -r website /opt/velocty/

# Set ownership
sudo chown -R velocty:velocty /opt/velocty
```

### 2. Create the Service File

Save this as `/etc/systemd/system/velocty.service`:

```ini
[Unit]
Description=Velocty CMS
Documentation=https://github.com/marirs/velocty
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=velocty
Group=velocty
WorkingDirectory=/opt/velocty
ExecStart=/opt/velocty/velocty

# Bind to localhost only (Nginx handles public traffic)
Environment=ROCKET_ADDRESS=127.0.0.1
Environment=ROCKET_PORT=8000
Environment=ROCKET_LOG_LEVEL=normal

# Restart policy
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
ReadWritePaths=/opt/velocty

# Resource limits
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

### 3. Enable and Start

```bash
# Reload systemd to pick up the new service file
sudo systemctl daemon-reload

# Enable on boot
sudo systemctl enable velocty

# Start the service
sudo systemctl start velocty

# Check status
sudo systemctl status velocty
```

### 4. Useful Commands

```bash
# View live logs
sudo journalctl -u velocty -f

# View last 100 log lines
sudo journalctl -u velocty -n 100

# Restart after a binary update
sudo systemctl restart velocty

# Stop the service
sudo systemctl stop velocty
```

### 5. Updating the Binary

```bash
sudo systemctl stop velocty
sudo cp target/release/velocty /opt/velocty/velocty
sudo chown velocty:velocty /opt/velocty/velocty
sudo systemctl start velocty
```

### Multi-Site Service

For a multi-site deployment, the service file is identical — just ensure the binary was built with `--features multi-site`. The `WorkingDirectory` should contain the super admin registry database, and each site's `db_path` in the registry should be an absolute path or relative to the working directory.
