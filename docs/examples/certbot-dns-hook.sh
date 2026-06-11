#!/bin/bash
#
# NoDNS Certbot Manual DNS Hook Script
# =====================================
#
# This script implements the acme-dns protocol for Let's Encrypt / Certbot
# DNS-01 challenge validation using NoDNS's backwards-compatible acme-dns
# endpoint.
#
# USAGE:
#   certbot certonly --manual --preferred-challenges dns \
#     --manual-auth-hook /path/to/certbot-dns-hook.sh \
#     --manual-cleanup-hook /path/to/certbot-dns-hook.sh \
#     --manual-public-ip-logging-ok \
#     -d example.com -d *.example.com
#
# REQUIREMENTS:
# - curl (for HTTP requests)
# - jq (for JSON parsing, optional - fallback to grep/sed)
#
# HOW IT WORKS:
# 1. On first run, registers with NoDNS acme-dns endpoint
# 2. Stores credentials (fulldomain, username, password) in a config file
# 3. On subsequent runs, uses stored credentials to update TXT records
# 4. Certbot validates the challenge via DNS
# 5. Cleanup removes the TXT record (optional, acme-dns keeps rolling values)
#
# CREDENTIAL STORAGE:
# By default, stores credentials in /tmp/nodns-acmedns-<domain>.json
# Change CREDENTIALS_DIR to a persistent location for production use.
#
# NoDNS acme-dns Endpoints:
# - Register: POST https://nodns.shop/register
# - Update:   POST https://nodns.shop/update
# - Dev:      http://localhost:9090/register, http://localhost:9090/update

set -euo pipefail

# Configuration
# ==============

# NoDNS server endpoint (use http://localhost:9090 for development)
NODNS_SERVER="${NODNS_SERVER:-https://nodns.shop}"

# Directory to store acme-dns credentials
# Use a persistent location like /var/lib/nodns-acmedns for production
CREDENTIALS_DIR="${CREDENTIALS_DIR:-/tmp}"

# Your Nostr npub (optional - can be provided during registration)
# If set, the registered fulldomain will be associated with this npub
NODNS_NPUB="${NODNS_NPUB:-}"

# DNS propagation delay in seconds
PROPAGATION_DELAY="${PROPAGATION_DELAY:-30}"

# Enable curl verbose output
VERBOSE="${VERBOSE:-0}"

# Logging
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

# Error handler
error_exit() {
    log "ERROR: $1"
    exit 1
}

# Debug logging
debug() {
    if [ "$VERBOSE" -eq 1 ]; then
        log "DEBUG: $*"
    fi
}

# Get certificate domain from certbot
# ====================================
# Certbot passes the domain as CERTBOT_DOMAIN environment variable
# Example: example.com or _acme-challenge.example.com

DOMAIN="${CERTBOT_DOMAIN:-}"
if [ -z "$DOMAIN" ]; then
    error_exit "CERTBOT_DOMAIN environment variable not set"
fi

# For wildcard certificates, certbot passes the base domain
# (e.g., CERTBOT_DOMAIN=example.com for *.example.com)
BASE_DOMAIN="$DOMAIN"

# Credential file path
# ====================
# Use a unique filename per domain to support multiple domains
CRED_FILE="${CREDENTIALS_DIR}/nodns-acmedns-${BASE_DOMAIN}.json"

# Register with acme-dns
# =======================
register_acmedns() {
    log "Registering new acme-dns account for domain: $BASE_DOMAIN"

    # Build registration payload
    # Optional: include npub to associate with your Nostr identity
    local payload
    if [ -n "$NODNS_NPUB" ]; then
        payload="{\"acmedns\":{\"npub\":\"${NODNS_NPUB}\"}}"
    else
        payload='{}'
    fi

    debug "Registration payload: $payload"

    # Make registration request
    local response
    response=$(curl -s -X POST \
        "${NODNS_SERVER}/register" \
        -H "Content-Type: application/json" \
        -d "$payload" 2>&1)

    debug "Registration response: $response"

    # Parse response
    local fulldomain username password subdomain
    fulldomain=$(echo "$response" | grep -o '"fulldomain":"[^"]*"' | cut -d'"' -f4)
    username=$(echo "$response" | grep -o '"username":"[^"]*"' | cut -d'"' -f4)
    password=$(echo "$response" | grep -o '"password":"[^"]*"' | cut -d'"' -f4)
    subdomain=$(echo "$response" | grep -o '"subdomain":"[^"]*"' | cut -d'"' -f4)

    if [ -z "$fulldomain" ] || [ -z "$username" ] || [ -z "$password" ]; then
        error_exit "Invalid registration response: $response"
    fi

    log "Registered successfully:"
    log "  Full domain: $fulldomain"
    log "  Username: $username"
    log "  Subdomain: $subdomain"

    # Save credentials
    local cred_dir
    cred_dir=$(dirname "$CRED_FILE")
    if [ ! -d "$cred_dir" ]; then
        mkdir -p "$cred_dir" || error_exit "Failed to create credentials directory: $cred_dir"
    fi

    cat > "$CRED_FILE" <<EOF
{
  "fulldomain": "$fulldomain",
  "username": "$username",
  "password": "$password",
  "subdomain": "$subdomain",
  "base_domain": "$BASE_DOMAIN"
}
EOF

    # Set restrictive permissions (readable only by owner)
    chmod 600 "$CRED_FILE"

    log "Credentials saved to: $CRED_FILE"

    # Provide CNAME instructions
    log ""
    log "IMPORTANT: To complete the setup, add a CNAME record:"
    log "  _acme-challenge.$BASE_DOMAIN  CNAME  $fulldomain"
    log ""
    log "This tells Let's Encrypt to validate at $fulldomain instead of"
    log "_acme-challenge.$BASE_DOMAIN."
}

# Load existing credentials
# ==========================
load_credentials() {
    if [ ! -f "$CRED_FILE" ]; then
        return 1
    fi

    source "$CRED_FILE" 2>/dev/null || true

    # If credentials are in JSON format (from registration)
    if command -v jq &> /dev/null; then
        fulldomain=$(jq -r '.fulldomain' "$CRED_FILE" 2>/dev/null)
        username=$(jq -r '.username' "$CRED_FILE" 2>/dev/null)
        password=$(jq -r '.password' "$CRED_FILE" 2>/dev/null)
        subdomain=$(jq -r '.subdomain' "$CRED_FILE" 2>/dev/null)
    else
        # Fallback to grep/sed if jq is not available
        fulldomain=$(grep -o '"fulldomain":"[^"]*"' "$CRED_FILE" 2>/dev/null | cut -d'"' -f4)
        username=$(grep -o '"username":"[^"]*"' "$CRED_FILE" 2>/dev/null | cut -d'"' -f4)
        password=$(grep -o '"password":"[^"]*"' "$CRED_FILE" 2>/dev/null | cut -d'"' -f4)
        subdomain=$(grep -o '"subdomain":"[^"]*"' "$CRED_FILE" 2>/dev/null | cut -d'"' -f4)
    fi

    if [ -z "$fulldomain" ] || [ -z "$username" ] || [ -z "$password" ]; then
        error_exit "Invalid credentials file: $CRED_FILE"
    fi

    debug "Loaded credentials from: $CRED_FILE"
    debug "  Full domain: $fulldomain"
    debug "  Username: $username"

    return 0
}

# Update TXT record
# =================
update_txt_record() {
    local txt_value="$1"

    log "Updating TXT record for $fulldomain"

    # Build update payload
    local payload
    payload="{\"subdomain\":\"${subdomain}\",\"txt\":\"${txt_value}\"}"

    debug "Update payload: $payload"

    # Make update request with authentication headers
    local response
    response=$(curl -s -X POST \
        "${NODNS_SERVER}/update" \
        -H "Content-Type: application/json" \
        -H "X-Api-User: ${username}" \
        -H "X-Api-Key: ${password}" \
        -d "$payload" 2>&1)

    debug "Update response: $response"

    # Verify response
    local response_txt
    response_txt=$(echo "$response" | grep -o '"txt":"[^"]*"' | cut -d'"' -f4)

    if [ "$response_txt" != "$txt_value" ]; then
        error_exit "Failed to update TXT record. Response: $response"
    fi

    log "TXT record updated successfully: $txt_value"
}

# Wait for DNS propagation
# =========================
wait_for_propagation() {
    local txt_value="$1"
    local max_attempts=30
    local attempt=0

    log "Waiting for DNS propagation (max ${PROPAGATION_DELAY}s)..."

    sleep "$PROPAGATION_DELAY"

    # Optional: Verify the TXT record is visible
    if command -v dig &> /dev/null; then
        log "Verifying TXT record visibility..."
        while [ $attempt -lt $max_attempts ]; do
            local lookup
            lookup=$(dig +short TXT "$fulldomain" 2>/dev/null || echo "")

            if echo "$lookup" | grep -q "\"${txt_value}\""; then
                log "TXT record is now visible in DNS!"
                return 0
            fi

            attempt=$((attempt + 1))
            if [ $attempt -lt $max_attempts ]; then
                sleep 2
            fi
        done

        log "WARNING: TXT record not verified in DNS, but continuing..."
    fi
}

# Delete TXT record (cleanup)
# ============================
delete_txt_record() {
    log "Cleanup: Deleting TXT record for $fulldomain"
    log "Note: acme-dns keeps rolling values, so this is optional"

    # Send empty txt value or leave as-is
    # Most acme-dns implementations keep old values
    # We'll just log that cleanup is done

    log "Cleanup complete (record may remain until next update)"
}

# Main entry point
# ================
main() {
    # Determine operation mode (auth or cleanup) from environment
    # Certbot sets CERTBOT_VALIDATION for auth, and calls script again for cleanup

    if [ -n "${CERTBOT_VALIDATION:-}" ]; then
        # Auth mode - add TXT record
        log "=== Auth Mode: Adding DNS-01 TXT record ==="

        # Load or register credentials
        if ! load_credentials; then
            register_acmedns
            load_credentials || error_exit "Failed to load credentials after registration"
        fi

        local txt_value="${CERTBOT_VALIDATION}"
        log "Challenge value: $txt_value"

        # Update the TXT record
        update_txt_record "$txt_value"

        # Wait for DNS propagation
        wait_for_propagation "$txt_value"

        log "=== Auth complete. Certbot will now validate ==="

    elif [ "${1:-}" = "--cleanup" ] || [ -n "${CERTBOT_ALL_DOMAINS:-}" ]; then
        # Cleanup mode - remove TXT record
        log "=== Cleanup Mode: Removing DNS-01 TXT record ==="

        if load_credentials; then
            delete_txt_record
        else
            log "WARNING: No credentials found, skipping cleanup"
        fi

    else
        error_exit "Invalid invocation. This script should be called by certbot."
    fi
}

# Run main function
main "$@"