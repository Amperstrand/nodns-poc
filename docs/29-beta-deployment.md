# 29 — beta.nodns.shop Deployment

> **Status**: READY. Instructions for deploying the new frontend as beta.nodns.shop alongside the existing nodns.shop.

## Architecture

```
beta.nodns.shop → Caddy (TLS) → serves /var/www/nodns-beta/ (static files)
                                → proxies /api/* to 127.0.0.1:9090 (Rust bot)

nodns.shop      → Caddy (TLS) → serves /var/www/nodns/ (existing static files)
                                → proxies /api/* to 127.0.0.1:9090 (same Rust bot)
```

Both frontends share the same backend bot — they're just different static file roots.

## Steps on VPS (46.22.104.104)

### 1. Build the beta frontend locally

```bash
cd nodns-frontend
npm run build
```

The static output is in `out/`.

### 2. Upload to VPS

```bash
scp -r out/* root@46.22.104.104:/var/www/nodns-beta/
```

Create the directory first if needed:

```bash
ssh root@46.22.104.104 "mkdir -p /var/www/nodns-beta"
```

### 3. Add DNS record for beta.nodns.shop

Add an A record pointing `beta.nodns.shop` to `46.22.104.104` in the nodns.shop zone.

Via Knot DNS:
```
nodns-checkconf
# Use knupdate or nsupdate to add:
# beta.nodns.shop. IN A 46.22.104.104
```

### 4. Update Caddy config

Add a new site block to `/etc/caddy/Caddyfile`:

```
beta.nodns.shop {
    root * /var/www/nodns-beta
    file_server

    # API proxy to Rust bot
    handle /api/* {
        reverse_proxy 127.0.0.1:9090
    }

    # SPA fallback for client-side routing
    try_files {path} /index.html

    encode gzip
}
```

### 5. Reload Caddy

```bash
systemctl reload caddy
```

Caddy will automatically provision a TLS certificate for beta.nodns.shop via ACME (Let's Encrypt or ZeroSSL).

### 6. Verify

```bash
curl -I https://beta.nodns.shop
# Should return 200 with the new frontend

curl https://beta.nodns.shop/api/zones/nodns.shop/pricing
# Should return pricing JSON from the bot
```

## Rollback

If beta is broken, just remove the Caddy site block and reload:

```bash
# Edit /etc/caddy/Caddyfile, remove the beta.nodns.shop block
systemctl reload caddy
```

nodns.shop continues serving the old frontend unaffected.

## Update Flow

To push new changes to beta:

```bash
cd nodns-frontend
npm run build
scp -r out/* root@46.22.104.104:/var/www/nodns-beta/
```

No restart needed — static files are served directly by Caddy.
