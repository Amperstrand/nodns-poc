# Backwards-Compatible APIs

> **Status**: ACTIVE

NoDNS stores DNS records in Nostr events (kind 11111), but not everyone wants to publish events directly. To make NoDNS usable with existing tooling, we implement three standard DNS protocols that map incoming requests to Nostr events behind the scenes.

This document covers the three backwards-compatible APIs:

- **DynDNS v2**: For routers and ddclient
- **acme-dns**: For Let's Encrypt and ZeroSSL certificate automation
- **RFC 2136 DNS UPDATE**: For nsupdate, BIND, and other DNS clients

These are implemented in `nodns-bot-rs` as HTTP and UDP endpoints. They translate standard protocol requests into Nostr events that the bot validates and pushes to Knot DNS via DDNS.

---

## Protocol 1: DynDNS v2

### Overview

The DynDNS v2 protocol is a simple HTTP API used by home routers, ddclient, inadyn, and other dynamic DNS clients. It lets devices update their public IP address automatically.

NoDNS implements the `/nic/update` endpoint with HTTP Basic Auth where the username is an npub and the password is the corresponding nsec. The bot verifies the signature, checks authorization, then publishes an A or AAAA record as a Nostr event.

**Compatible with**: ddclient, inadyn, most home router DDNS features, OpenWrt ddns-scripts, pfSense Dynamic DNS.

### Authentication

HTTP Basic Auth with Nostr keys:

- **Username**: bech32-encoded npub (public key)
- **Password**: bech32-encoded nsec (private key)

The bot derives the npub from the nsec and verifies it matches the username. This ensures only the legitimate owner can update records.

For example:
```
Username: npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr
Password: nsec1... (corresponding private key)
```

### Request Format

**Endpoint**: `GET /nic/update` or `POST /nic/update`

**Query Parameters**:

| Parameter | Required | Description |
|-----------|----------|-------------|
| `hostname` | Yes | FQDN to update (e.g. `npub1hw6am.nodns.shop` or `alice.nodns.shop`) |
| `myip` | No | IP address to set. If omitted, uses the client's IP. Auto-detects A vs AAAA based on IP version. |

**Example with explicit IP**:

```bash
curl -u "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr:nsec1..." \
  "https://nodns.shop/nic/update?hostname=npub1hw6am.nodns.shop&myip=1.2.3.4"
```

**Example using client IP** (router omits `myip`):

```bash
curl -u "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr:nsec1..." \
  "https://nodns.shop/nic/update?hostname=npub1hw6am.nodns.shop"
```

### Response Format

Responses are plain text with HTTP status codes:

| Status | Response | Meaning |
|--------|----------|---------|
| 200 | `good <ip>` | IP updated successfully |
| 200 | `nochg <ip>` | IP unchanged (no update needed) |
| 401 | `badauth` | Invalid or missing credentials |
| 403 | `nohost` | Hostname exists but not owned by this user |
| 400 | `notfqdn` | Invalid FQDN or not in a managed zone |
| 500 | `911` | Server error |

**Example success response**:

```
good 1.2.3.4
```

### Authorization Rules

The bot checks ownership based on the hostname type:

1. **Cryptographic names** (`npub1xxx.nodns.shop`): The npub in the username must match the npub in the hostname. This is the direct ownership case.

2. **Delegated names** (`alice.nodns.shop`): The npub in the username must have an active delegation for that name. See [09-custom-names.md](09-custom-names.md) for delegation details.

The bot verifies:
- The nsec signature is valid
- The derived npub matches the username
- The npub owns the hostname (directly or via delegation)
- The hostname is in a managed zone (e.g. `nodns.shop`)

### Configuration

Enabled by default in `nodns-bot-rs/config.toml`:

```toml
[server]
bind = "127.0.0.1:9090"

[dyndns]
enabled = true
```

The server listens on the same port as the main API (default 9090). Caddy or another reverse proxy should handle TLS and forward requests.

### Client Setup Guide

**ddclient** (`/etc/ddclient/ddclient.conf`):

```conf
protocol = dyndns2
use = web, web=http://ipv4.nsupdate.info/myip
server = nodns.shop
login = npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr
password = nsec1...
hostname = npub1hw6am.nodns.shop
```

**ddclient with delegation**:

```conf
protocol = dyndns2
use = web, web=http://ipv4.nsupdate.info/myip
server = nodns.shop
login = npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr
password = nsec1...
hostname = alice.nodns.shop
```

**Home router** (generic UI fields):

- Service: Custom
- Server: `nodns.shop`
- Username: Your npub
- Password: Your nsec
- Hostname: Your FQDN

**Test manually**:

```bash
# Test the endpoint
curl -v -u "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr:nsec1..." \
  "https://nodns.shop/nic/update?hostname=npub1hw6am.nodns.shop"

# Verify the DNS record propagated
dig @8.8.8.8 npub1hw6am.nodns.shop A +short
```

### Limitations & Notes

- Only supports A and AAAA records (IP address updates)
- No wildcard support
- The nsec is sent in the HTTP password field, which is sent over HTTPS in production
- Credentials are validated on each request (no session or token)
- IP changes are published as Nostr events and typically propagate within seconds
- Prototype quality: not hardened against abuse or brute force attacks

---

## Protocol 2: acme-dns

### Overview

acme-dns is a REST API for handling ACME DNS-01 challenges. It's used by Let's Encrypt and ZeroSSL to verify domain ownership for certificate issuance.

NoDNS implements the standard acme-dns REST API, compatible with the acme-dns Go project. Users register to get credentials, create a CNAME record, and certbot/lego use those credentials to set the TXT record with the ACME challenge token.

**Compatible with**: certbot with acme-dns-certbot-joohoi, lego with DNS-01 provider, Caddy with ACME DNS challenge, acme.sh.

### Authentication

acme-dns uses two-step authentication:

1. **Registration**: Optional Nostr identity can be linked to the registration by sending `{"acmedns":{"npub":"npub1abc..."}}` in the POST body. This associates the acme-dns account with a Nostr pubkey.

2. **Update**: Uses credentials from the registration response:
   - `X-Api-User`: username (UUID)
   - `X-Api-Key`: password (base64-encoded)

These headers authenticate the TXT record update request.

### Request Format

#### POST /register

Register a new acme-dns account. Returns credentials for future updates.

**Request Body** (optional):

```json
{
  "acmedns": {
    "npub": "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr"
  }
}
```

**Example**:

```bash
curl -X POST https://nodns.shop/register \
  -H "Content-Type: application/json" \
  -d '{"acmedns":{"npub":"npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr"}}'
```

**Response** (201 Created):

```json
{
  "fulldomain": "a1b2c3d4-e5f6-7890-abcd-ef1234567890.acme.nodns.shop",
  "username": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "password": "base64-encoded-secret",
  "subdomain": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "allowfrom": []
}
```

The `fulldomain` is the CNAME target for your domain. Create a CNAME record:

```
_acme-challenge.yourdomain.nodns.shop → a1b2c3d4-e5f6-7890-abcd-ef1234567890.acme.nodns.shop
```

#### POST /update

Update the TXT record with the ACME challenge token.

**Headers**:

- `X-Api-User`: username from registration (UUID)
- `X-Api-Key`: password from registration (base64)

**Request Body**:

```json
{
  "subdomain": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "txt": "_d8h2k9j3l8m4n7p6q5r4s3t2u1v0w9x8y7z6"
}
```

**Example**:

```bash
curl -X POST https://nodns.shop/update \
  -H "Content-Type: application/json" \
  -H "X-Api-User: a1b2c3d4-e5f6-7890-abcd-ef1234567890" \
  -H "X-Api-Key: base64-encoded-secret" \
  -d '{"subdomain":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","txt":"_d8h2k9j3l8m4n7p6q5r4s3t2u1v0w9x8y7z6"}'
```

### Response Format

**Successful update** (200 OK):

```json
{
  "txt": "_d8h2k9j3l8m4n7p6q5r4s3t2u1v0w9x8y7z6"
}
```

**Error responses**:

- 401 Unauthorized: `{"error":"invalid credentials"}` or `{"error":"X-Api-User/X-Api-Key header required"}`
- 403 Forbidden: `{"error":"subdomain mismatch"}`

### Rolling TXT

NoDNS implements a rolling TXT mechanism to handle concurrent certificate requests. Each TXT record update stores both the current and previous values:

- When updating from `old` to `new`: the TXT response includes both `old` and `new`
- ACME clients can validate either value
- Subsequent updates roll forward: `new` becomes current, `old` is replaced

This prevents race conditions when multiple certificates are requested simultaneously (e.g., wildcard + root domain).

### Configuration

Enabled by default in `nodns-bot-rs/config.toml`:

```toml
[server]
bind = "127.0.0.1:9090"

[acmedns]
enabled = true
```

The server listens on the same port as the main API. Caddy or another reverse proxy should handle TLS and forward requests.

### Client Setup Guide

**certbot with acme-dns-certbot-joohoi**:

1. Install the certbot hook:
   ```bash
   pip install certbot-dns-acmedns
   ```

2. Register with NoDNS:
   ```bash
   curl -X POST https://nodns.shop/register \
     -H "Content-Type: application/json" \
     -d '{}' | jq
   ```

3. Save credentials to `/etc/letsencrypt/acmedns.json`:
   ```json
   {
     "nodns.shop": {
       "fulldomain": "uuid.acme.nodns.shop",
       "username": "uuid",
       "password": "base64-secret"
     }
   }
   ```

4. Create CNAME record in your NoDNS zone:
   ```
   _acme-challenge.yourdomain.nodns.shop → uuid.acme.nodns.shop
   ```

5. Run certbot:
   ```bash
   certbot certonly --dns-acmedns --dns-acmedns-credentials /etc/letsencrypt/acmedns.conf \
     -d yourdomain.nodns.shop
   ```

**lego (for automatic renewal)**:

```bash
# Register
curl -X POST https://nodns.shop/register \
  -H "Content-Type: application/json" \
  -d '{}' > /tmp/acmedns.json

# Use lego
lego --email you@example.com --dns acmedns \
  --dns.acmedns.credentials-file /tmp/acmedns.json \
  --domains yourdomain.nodns.shop run
```

**Verify the challenge**:

```bash
# Check that the TXT record is set
dig @8.8.8.8 _acme-challenge.yourdomain.nodns.shop TXT +short
```

### Limitations & Notes

- Only supports TXT records (for ACME challenges)
- Credentials are generated per-registration and cannot be changed
- The `allowfrom` field in the registration response is ignored (no IP restriction)
- CNAME must be created manually in the NoDNS zone before certificate issuance
- Rolling TXT keeps only two values (current + previous)
- Prototype quality: not hardened against abuse or rate limiting

See [17-acme-dns01-trust-analysis.md](17-acme-dns01-trust-analysis.md) for security analysis of the ACME DNS-01 challenge with NoDNS.

---

## Protocol 3: RFC 2136 DNS UPDATE

### Overview

RFC 2136 defines the standard DNS dynamic update protocol. It's used by `nsupdate`, BIND, PowerDNS, and other DNS tools to make authenticated changes to zone data.

NoDNS implements RFC 2136 §2.5 with TSIG authentication via HMAC-SHA256. Updates are received over UDP, validated, then translated into Nostr events for authorization and persistence.

The bot runs a separate UDP listener on a configurable port (default 5353, not 53) to avoid conflicts with Knot DNS.

**Compatible with**: nsupdate (BIND tools), knotnsupdate, any RFC 2136-compliant DNS client.

### Authentication

TSIG (Transaction SIG) with HMAC-SHA256:

- Single shared key configured in `[dns_update]` section of `config.toml`
- Key name: human-readable identifier (e.g. `nodns-update`)
- Key secret: base64-encoded HMAC-SHA256 secret
- Algorithm: `hmac-sha256`

All UPDATE messages must include a TSIG record in the additional section. The bot verifies the TSIG signature before processing the update.

### Request Format

Transport: UDP on configurable port (default 5353).

**Supported operations** (RFC 2136 §2.5):

| Operation | Class | TTL | Meaning |
|-----------|-------|-----|---------|
| ADD | IN | > 0 | Add or replace a record |
| DELETE RRSET | ANY | 0 | Delete all records of a type |
| DELETE RR | NONE | 0 | Delete a specific record |

**Supported record types**: A, AAAA, CNAME, TXT, MX, SRV

**Request structure** (RFC 2136 §2.3):

```
Header  | ID, flags
Zone    | zone name, class IN, type SOA
Prereq  | (optional) prerequisites
Update  | one or more updates
Additional | TSIG record (required)
```

### Response Format

Standard DNS response codes:

| Code | Name | Meaning |
|------|------|---------|
| NOERROR | 0 | Update successful |
| REFUSED | 5 | Not authorized or zone not managed |
| NOTZONE | 9 | Zone name not found |
| SERVFAIL | 2 | Server error (e.g. Nostr relay failure) |
| FORMERR | 1 | Malformed request |
| NOTIMP | 4 | Operation not supported (e.g. unsupported record type) |

### Authorization

Authorization checks:

1. **Zone validation**: The zone section must match a managed NoDNS zone (e.g. `nodns.shop`)
2. **Ownership check**: The FQDN being updated must have existing records in the NoDNS store, owned by a Nostr identity
3. **No "create from nothing"**: Unlike traditional DNS UPDATE, you cannot create a subdomain that you don't already own. See [09-custom-names.md](09-custom-names.md) for name registration.

This is a key difference from standard DNS UPDATE. In NoDNS, you must first claim a subdomain via Nostr (by publishing a kind 11111 event or receiving a delegation) before you can update it via RFC 2136.

### Configuration

Enable and configure in `nodns-bot-rs/config.toml`:

```toml
[dns_update]
enabled = true
listen = "0.0.0.0:5353"
tsig_key_name = "nodns-update"
tsig_key_secret = "base64-encoded-hmac-sha256-secret"
```

**Generate a TSIG key**:

```bash
# Using knot (if installed)
knot keygen -t hmac-sha256 nodns-update

# Or generate base64 secret manually
openssl rand -base64 24
```

Update `tsig_key_secret` with the base64 output.

**Knot DNS configuration** (if Knot is also using TSIG for the bot's DDNS updates):

```knot
key "nodns-bot" {
    algorithm hmac-sha256;
    secret "base64-secret-from-config";
}

acl "bot-allowed" {
    key "nodns-bot";
}

zone "nodns.shop" {
    file "/var/lib/knot/zones/nodns.shop.zone";
    dnssec-signing on;
    acl "bot-allowed";
}
```

### Client Setup Guide

**nsupdate with TSIG key file** (`/tmp/nsupdate.key`):

```
key nodns-update base64-encoded-secret
```

**nsupdate script** (`/tmp/update.ns`):

```
key nodns-update base64-encoded-secret
server localhost 5353
zone nodns.shop
update delete alice.nodns.shop A
update add alice.nodns.shop 3600 IN A 1.2.3.4
send
```

**Run nsupdate**:

```bash
nsupdate -k /tmp/nsupdate.key -v /tmp/update.ns
```

**Interactive nsupdate**:

```bash
nsupdate -k /tmp/nsupdate.key
> server localhost 5353
> zone nodns.shop
> update add alice.nodns.shop 3600 IN A 1.2.3.4
> send
> quit
```

**knotnsupdate** (alternative to BIND nsupdate):

```bash
knotnsupdate -k nodns-update:base64-secret -s localhost:5353 << EOF
zone nodns.shop
update add alice.nodns.shop 3600 IN A 1.2.3.4
send
EOF
```

**Verify the update**:

```bash
# Check that the DNS record propagated
dig @127.0.0.1 alice.nodns.shop A +short
```

### Limitations & Notes

- Only supports UDP (not TCP) for DNS UPDATE
- Port 5353 is non-standard to avoid conflicts with Knot DNS on port 53
- Cannot create new subdomains (must own them via Nostr first)
- Single shared TSIG key for all updates (not per-user keys)
- No wildcard support in UPDATE operations
- Zone must be pre-configured in the bot
- Prototype quality: not hardened against abuse or replay attacks
- The bot does not implement prerequisite checks (RFC 2136 §2.4.4) for simplicity

---

## Quick Reference

| Protocol | Purpose | Transport | Auth | Use Cases |
|----------|---------|-----------|------|-----------|
| **DynDNS v2** | Dynamic IP updates | HTTP (GET/POST) | HTTP Basic (npub/nsec) | Home routers, ddclient |
| **acme-dns** | ACME DNS-01 challenges | HTTP (POST) | X-Api-User/X-Api-Key | Let's Encrypt, ZeroSSL |
| **RFC 2136** | Standard DNS updates | UDP | TSIG (HMAC-SHA256) | nsupdate, BIND tools |

| Protocol | Record Types | Ownership | Config Section |
|----------|--------------|-----------|----------------|
| **DynDNS v2** | A, AAAA | Direct (npub) or delegated | `[dyndns]` |
| **acme-dns** | TXT (ACME) | Per-registration UUID | `[acmedns]` |
| **RFC 2136** | A, AAAA, CNAME, TXT, MX, SRV | Pre-existing Nostr ownership | `[dns_update]` |

| Protocol | Default Port | TLS | Endpoint |
|----------|--------------|-----|----------|
| **DynDNS v2** | 9090 (via reverse proxy) | Yes (via Caddy) | `/nic/update` |
| **acme-dns** | 9090 (via reverse proxy) | Yes (via Caddy) | `/register`, `/update` |
| **RFC 2136** | 5353 | No (UDP) | None (DNS protocol) |

---

## Implementation Notes

All three protocols are implemented in `nodns-bot-rs` as part of the main HTTP/UDP server:

- **DynDNS v2**: Handler at `/nic/update` in the web server
- **acme-dns**: Handlers at `/register` and `/update` in the web server
- **RFC 2136**: UDP listener spawned on the configured port

The translation flow is the same for all three:

1. Receive request (HTTP or UDP)
2. Validate authentication (Basic, API headers, or TSIG)
3. Check authorization (ownership, delegation, zone)
4. Publish Nostr event (kind 11111) with the update
5. Push DDNS UPDATE to Knot DNS via TSIG
6. Return protocol-specific response

This ensures that all record changes go through the same validation and persistence layer, whether they come from Nostr events or backwards-compatible APIs.

---

## Security Considerations

These are prototype/demo quality implementations, not production-hardened:

- No rate limiting on incoming requests
- No brute force protection for authentication
- Credentials sent over HTTP (use TLS in production via Caddy)
- No audit logging of API requests
- Single shared TSIG key for RFC 2136 (compromise affects all users)
- No IP restrictions or geo-blocking

In production deployment, consider adding:
- Rate limiting per client or per user
- Request logging and audit trails
- Per-user TSIG keys for RFC 2136
- IP allowlists for acme-dns updates
- Abuse detection and automatic blocking

For DNS security and DNSSEC considerations, see the Security & DNSSEC section in the doc index.