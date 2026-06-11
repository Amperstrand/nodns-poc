#!/bin/bash
#
# NoDNS RFC 2136 DNS UPDATE Examples
# ===================================
#
# This script demonstrates how to use the nsupdate tool with TSIG
# authentication to manage DNS records on NoDNS via the RFC 2136
# DNS UPDATE protocol.
#
# NoDNS provides a UDP listener for DNS UPDATE requests on port 5353
# (configurable via dns_update.udp_port in server config).
#
# TSIG Authentication:
# - Uses HMAC-SHA256 algorithm
# - Single shared key configured on the NoDNS server
# - Key name: dns_update.tsig_key_name
# - Key secret: dns_update.tsig_key_secret
#
# SETUP:
# 1. Obtain the TSIG key from your NoDNS server configuration
# 2. Create a BIND-format key file (see generate_key_file() function)
# 3. Run this script or use nsupdate directly with the key file
#
# KEY FILE FORMAT (BIND):
# ------------------------
# key "nodns-key" {
#     algorithm hmac-sha256;
#     secret "BASE64_ENCODED_KEY";
# };
#
# The key file can be generated using the dnssec-keygen tool or manually.
# See the generate_key_file() function below for an example.
#
# NSUPDATE USAGE:
# ---------------
# nsupdate -k /path/to/key-file -v << EOF
# server nodns.shop 5353
# update add example.nodns.shop 300 A 1.2.3.4
# send
# EOF
#
# SUPPORTED RECORD TYPES:
# - A (IPv4 address)
# - AAAA (IPv6 address)
# - CNAME (canonical name)
# - TXT (text record)
# - MX (mail exchange)
# - SRV (service record)
#
# ZONES:
# Must use one of NoDNS's managed zones (e.g., nodns.shop)

set -euo pipefail

# Configuration
# ==============

# NoDNS server and UDP port for DNS UPDATE
NODNS_SERVER="${NODNS_SERVER:-nodns.shop}"
NODNS_PORT="${NODNS_PORT:-5353}"

# TSIG key configuration (obtain from your NoDNS server)
# These should match dns_update.tsig_key_name and dns_update.tsig_key_secret
TSIG_KEY_NAME="${TSIG_KEY_NAME:-nodns-key}"
TSIG_KEY_SECRET="${TSIG_KEY_SECRET:-}"

# Zone to update (must be a managed zone)
ZONE="${ZONE:-nodns.shop}"

# TTL for new records (in seconds)
TTL="${TTL:-300}"

# Temporary directory for key file
TMP_DIR="${TMP_DIR:-/tmp}"

# Logging
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

error_exit() {
    log "ERROR: $1"
    exit 1
}

# Check if required tools are available
check_requirements() {
    local missing_tools=()

    if ! command -v nsupdate &> /dev/null; then
        missing_tools+=("nsupdate")
    fi

    if [ ${#missing_tools[@]} -gt 0 ]; then
        error_exit "Missing required tools: ${missing_tools[*]}. Install: apt-get install dnsutils (Debian/Ubuntu) or brew install bind (macOS)"
    fi
}

# Generate BIND-format key file
# ==============================
# This creates a temporary key file that nsupdate can use for TSIG auth
generate_key_file() {
    local key_file="$1"

    if [ -z "$TSIG_KEY_SECRET" ]; then
        error_exit "TSIG_KEY_SECRET not set. Set it via environment variable or edit this script."
    fi

    log "Creating key file: $key_file"

    cat > "$key_file" <<EOF
key "${TSIG_KEY_NAME}" {
    algorithm hmac-sha256;
    secret "${TSIG_KEY_SECRET}";
};
EOF

    # Set restrictive permissions
    chmod 600 "$key_file"

    log "Key file created successfully"
}

# Run nsupdate with given commands
# =================================
run_nsupdate() {
    local commands="$1"
    local key_file="$2"

    log "Executing nsupdate commands..."

    nsupdate -k "$key_file" -v << EOF
server ${NODNS_SERVER} ${NODNS_PORT}
${commands}
send
EOF

    log "nsupdate completed"
}

# Example 1: Add an A record
# ===========================
example_add_a_record() {
    local name="${1:-}"
    local ip="${2:-}"

    if [ -z "$name" ]; then
        name="example"
    fi

    if [ -z "$ip" ]; then
        ip="1.2.3.4"
    fi

    log "=== Example 1: Add A record ==="
    log "  Record: ${name}.${ZONE} A ${ip}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    run_nsupdate "update add ${name}.${ZONE} ${TTL} A ${ip}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 2: Add an AAAA record
# ==============================
example_add_aaaa_record() {
    local name="${1:-}"
    local ip="${2:-}"

    if [ -z "$name" ]; then
        name="ipv6-example"
    fi

    if [ -z "$ip" ]; then
        ip="2001:db8::1"
    fi

    log "=== Example 2: Add AAAA record ==="
    log "  Record: ${name}.${ZONE} AAAA ${ip}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    run_nsupdate "update add ${name}.${ZONE} ${TTL} AAAA ${ip}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 3: Add a CNAME record
# ==============================
example_add_cname_record() {
    local alias="${1:-}"
    local target="${2:-}"

    if [ -z "$alias" ]; then
        alias="www"
    fi

    if [ -z "$target" ]; then
        target="example.${ZONE}"
    fi

    log "=== Example 3: Add CNAME record ==="
    log "  Record: ${alias}.${ZONE} CNAME ${target}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    run_nsupdate "update add ${alias}.${ZONE} ${TTL} CNAME ${target}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 4: Add a TXT record
# ============================
example_add_txt_record() {
    local name="${1:-}"
    local value="${2:-}"

    if [ -z "$name" ]; then
        name="_acme-challenge"
    fi

    if [ -z "$value" ]; then
        value="example_txt_record_value"
    fi

    log "=== Example 4: Add TXT record ==="
    log "  Record: ${name}.${ZONE} TXT \"${value}\""
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    # Note: TXT values need to be quoted
    run_nsupdate "update add ${name}.${ZONE} ${TTL} TXT \"${value}\"" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 5: Add an MX record
# ============================
example_add_mx_record() {
    local name="${1:-}"
    local priority="${2:-}"
    local mailserver="${3:-}"

    if [ -z "$name" ]; then
        name="mail"
    fi

    if [ -z "$priority" ]; then
        priority=10
    fi

    if [ -z "$mailserver" ]; then
        mailserver="mail.${ZONE}"
    fi

    log "=== Example 5: Add MX record ==="
    log "  Record: ${name}.${ZONE} MX ${priority} ${mailserver}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    run_nsupdate "update add ${name}.${ZONE} ${TTL} MX ${priority} ${mailserver}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 6: Add an SRV record
# =============================
example_add_srv_record() {
    local service="${1:-}"
    local proto="${2:-}"
    local name="${3:-}"
    local priority="${4:-}"
    local weight="${5:-}"
    local port="${6:-}"
    local target="${7:-}"

    if [ -z "$service" ]; then
        service="_sip"
    fi

    if [ -z "$proto" ]; then
        proto="_tcp"
    fi

    if [ -z "$name" ]; then
        name=""
    fi

    if [ -z "$priority" ]; then
        priority=10
    fi

    if [ -z "$weight" ]; then
        weight=60
    fi

    if [ -z "$port" ]; then
        port=5060
    fi

    if [ -z "$target" ]; then
        target="sipserver.${ZONE}"
    fi

    local fqdn="${service}.${proto}.${name}.${ZONE}"

    log "=== Example 6: Add SRV record ==="
    log "  Record: ${fqdn} SRV ${priority} ${weight} ${port} ${target}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    run_nsupdate "update add ${fqdn} ${TTL} SRV ${priority} ${weight} ${port} ${target}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 7: Update an existing record
# ======================================
example_update_record() {
    local name="${1:-}"
    local new_ip="${2:-}"

    if [ -z "$name" ]; then
        name="example"
    fi

    if [ -z "$new_ip" ]; then
        new_ip="5.6.7.8"
    fi

    log "=== Example 7: Update existing A record ==="
    log "  Record: ${name}.${ZONE} A ${new_ip}"
    log "  TTL: ${TTL}s"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    # First delete the old record, then add the new one
    run_nsupdate "update delete ${name}.${ZONE} A
update add ${name}.${ZONE} ${TTL} A ${new_ip}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 8: Delete all records for a name (class ANY)
# =====================================================
example_delete_all_records() {
    local name="${1:-}"

    if [ -z "$name" ]; then
        name="old-record"
    fi

    log "=== Example 8: Delete ALL records for a name (class ANY) ==="
    log "  Target: ${name}.${ZONE}"
    log "  This removes A, AAAA, CNAME, TXT, MX, SRV - everything"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    # Using class ANY deletes all records regardless of type
    run_nsupdate "update delete ${name}.${ZONE} ANY" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 9: Delete a specific RR (class NONE)
# =============================================
example_delete_specific_record() {
    local name="${1:-}"
    local type="${2:-}"
    local value="${3:-}"

    if [ -z "$name" ]; then
        name="example"
    fi

    if [ -z "$type" ]; then
        type="A"
    fi

    if [ -z "$value" ]; then
        value="1.2.3.4"
    fi

    log "=== Example 9: Delete a specific record (class NONE) ==="
    log "  Record: ${name}.${ZONE} ${type} ${value}"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    # Using class NONE deletes only the specific RRset
    run_nsupdate "update delete ${name}.${ZONE} ${type} ${value}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Example 10: Batch updates
# ==========================
example_batch_updates() {
    log "=== Example 10: Batch multiple updates ==="
    log "  Adding A, AAAA, and MX records in one transaction"

    local key_file="${TMP_DIR}/nodns-key-$$"
    generate_key_file "$key_file"

    # Multiple updates in a single transaction
    run_nsupdate "update add batch1.${ZONE} ${TTL} A 1.2.3.4
update add batch1.${ZONE} ${TTL} AAAA 2001:db8::1
update add batch2.${ZONE} ${TTL} A 5.6.7.8
update add mail.${ZONE} ${TTL} MX 10 mail.${ZONE}" "$key_file"

    rm -f "$key_file"
    log ""
}

# Show usage
show_usage() {
    cat << EOF
NoDNS RFC 2136 DNS UPDATE Examples
====================================

USAGE:
    $(basename "$0") <example-number>

    Or set environment variables:
    export TSIG_KEY_SECRET="your_base64_encoded_key"
    export TSIG_KEY_NAME="nodns-key"
    $(basename "$0") all

EXAMPLES:
    1   Add an A record
    2   Add an AAAA record
    3   Add a CNAME record
    4   Add a TXT record
    5   Add an MX record
    6   Add an SRV record
    7   Update an existing record
    8   Delete all records for a name (class ANY)
    9   Delete a specific record (class NONE)
    10  Batch multiple updates
    all Run all examples

ENVIRONMENT VARIABLES:
    NODNS_SERVER        NoDNS server hostname (default: nodns.shop)
    NODNS_PORT          DNS UPDATE UDP port (default: 5353)
    TSIG_KEY_NAME       TSIG key name (default: nodns-key)
    TSIG_KEY_SECRET     TSIG key secret (BASE64 encoded, REQUIRED)
    ZONE                Zone to update (default: nodns.shop)
    TTL                 TTL for new records (default: 300)
    TMP_DIR             Temporary directory for key file (default: /tmp)

OBTAINING TSIG KEYS:
    The TSIG key must match the configuration on your NoDNS server:
    - Key name: dns_update.tsig_key_name
    - Key secret: dns_update.tsig_key_secret

    Contact your NoDNS administrator to get these values.

DIRECT NSUPDATE USAGE:
    Create a key file first:
    cat > /tmp/nodns-key.conf << 'KEYEOF'
key "nodns-key" {
    algorithm hmac-sha256;
    secret "YOUR_BASE64_ENCODED_KEY";
};
KEYEOF

    Then use nsupdate directly:
    nsupdate -k /tmp/nodns-key.conf -v << 'EOF'
server nodns.shop 5353
update add example.nodns.shop 300 A 1.2.3.4
send
EOF
EOF
}

# Main
# ===
main() {
    check_requirements

    if [ $# -eq 0 ]; then
        show_usage
        exit 0
    fi

    local example="$1"

    case "$example" in
        1) example_add_a_record ;;
        2) example_add_aaaa_record ;;
        3) example_add_cname_record ;;
        4) example_add_txt_record ;;
        5) example_add_mx_record ;;
        6) example_add_srv_record ;;
        7) example_update_record ;;
        8) example_delete_all_records ;;
        9) example_delete_specific_record ;;
        10) example_batch_updates ;;
        all)
            example_add_a_record
            example_add_aaaa_record
            example_add_cname_record
            example_add_txt_record
            example_add_mx_record
            example_add_srv_record
            example_update_record
            example_batch_updates
            ;;
        *)
            show_usage
            error_exit "Unknown example: $example"
            ;;
    esac
}

main "$@"