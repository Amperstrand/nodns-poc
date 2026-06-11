#!/usr/bin/env bash
# Integration test scripts for NoDNS backwards-compatible DNS protocols
#
# PREREQUISITES:
# 1. NoDNS server running at BASE_URL (default http://localhost:9090)
#    - Development: Run locally with cargo run
#    - Production: Point to https://nodns.shop
#
# 2. For DynDNS v2 tests with npub names:
#    - Have a valid npub (bech32 encoded public key)
#    - Have the corresponding nsec (bech32 encoded secret key)
#    - Have an existing DNS record for the hostname being tested
#    - Fill in NPUB_VALID and NSEC_VALID below
#
# 3. For DynDNS v2 tests with delegated names:
#    - Have a delegated hostname owned by the npub
#    - Fill in DELEGATED_HOSTNAME below
#
# 4. For RFC 2136 tests:
#    - Knot DNS server running with dns_update.enabled=true in config
#    - nsupdate tool available (part of bind-utils or dnsutils package)
#    - Fill in TSIG_KEY_NAME and TSIG_KEY_SECRET below (from config)
#
# USAGE:
#   1. Fill in placeholder variables below
#   2. Run: ./integration-test.sh
#   3. Tests requiring pre-existing data will be skipped gracefully

set -euo pipefail

# ============================================================================
# CONFIGURATION
# ============================================================================

# NoDNS server endpoints
BASE_URL="${BASE_URL:-http://localhost:9090}"
DNS_UPDATE_PORT="${DNS_UPDATE_PORT:-5353}"

# ============================================================================
# PLACEHOLDER VARIABLES - FILL THESE IN FOR FULL TEST COVERAGE
# ============================================================================

# DynDNS v2 - Valid npub/nsec pair (for npub name updates)
# Format: npub1xxx... (bech32 encoded)
NPUB_VALID="${NPUB_VALID:-}"
# Format: nsec1xxx... (bech32 encoded)
NSEC_VALID="${NSEC_VALID:-}"

# DynDNS v2 - Delegated hostname (for delegated name updates)
# Format: alice.nodns.shop (must end in managed zone)
DELEGATED_HOSTNAME="${DELEGATED_HOSTNAME:-}"

# RFC 2136 - TSIG key configuration
# From config: dns_update.tsig_key_name
TSIG_KEY_NAME="${TSIG_KEY_NAME:-}"
# From config: dns_update.tsig_key_secret
TSIG_KEY_SECRET="${TSIG_KEY_SECRET:-}"

# Test IPs
IP_V4="${IP_V4:-192.0.2.1}"
IP_V4_ALT="${IP_V4_ALT:-192.0.2.2}"
IP_V6="${IP_V6:-2001:db8::1}"

# ============================================================================
# TEST COUNTERS
# ============================================================================

TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# ============================================================================
# UTILITY FUNCTIONS
# ============================================================================

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Print test result
print_result() {
    local test_name="$1"
    local result="$2"
    local message="${3:-}"

    case "$result" in
        pass)
            echo -e "${GREEN}✓ PASS${NC} $test_name"
            if [ -n "$message" ]; then
                echo -e "  ${GREEN}$message${NC}"
            fi
            ((TESTS_PASSED++))
            ;;
        fail)
            echo -e "${RED}✗ FAIL${NC} $test_name"
            if [ -n "$message" ]; then
                echo -e "  ${RED}$message${NC}"
            fi
            ((TESTS_FAILED++))
            ;;
        skip)
            echo -e "${YELLOW}⊘ SKIP${NC} $test_name"
            if [ -n "$message" ]; then
                echo -e "  ${YELLOW}$message${NC}"
            fi
            ((TESTS_SKIPPED++))
            ;;
    esac
}

# Check if required variable is set
check_var() {
    local var_name="$1"
    local var_value="${!var_name}"

    if [ -z "$var_value" ]; then
        return 1
    fi
    return 0
}

# ============================================================================
# DYNDNS V2 PROTOCOL TESTS
# ============================================================================

test_dyndns_a_record_update() {
    local test_name="DynDNS v2: Successful A record update"

    if ! check_var "NPUB_VALID" || ! check_var "NSEC_VALID"; then
        print_result "$test_name" skip "NPUB_VALID and NSEC_VALID not set"
        return
    fi

    local hostname="${NPUB_VALID}.nodns.shop"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ] && [[ "$body" == "good ${IP_V4}" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_aaaa_record_update() {
    local test_name="DynDNS v2: Successful AAAA record update"

    if ! check_var "NPUB_VALID" || ! check_var "NSEC_VALID"; then
        print_result "$test_name" skip "NPUB_VALID and NSEC_VALID not set"
        return
    fi

    local hostname="${NPUB_VALID}.nodns.shop"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V6}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ] && [[ "$body" == "good ${IP_V6}" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_nochg_same_ip() {
    local test_name="DynDNS v2: nochg when same IP sent twice"

    if ! check_var "NPUB_VALID" || ! check_var "NSEC_VALID"; then
        print_result "$test_name" skip "NPUB_VALID and NSEC_VALID not set"
        return
    fi

    local hostname="${NPUB_VALID}.nodns.shop"

    # First update
    curl -s -o /dev/null \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V4}"

    # Second update with same IP
    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ] && [[ "$body" == "nochg ${IP_V4}" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_badauth_wrong_nsec() {
    local test_name="DynDNS v2: badauth with wrong nsec"

    if ! check_var "NPUB_VALID"; then
        print_result "$test_name" skip "NPUB_VALID not set"
        return
    fi

    local hostname="${NPUB_VALID}.nodns.shop"
    local wrong_nsec="nsec1invalidkeyforpurposesoftesting"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${wrong_nsec}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "401" ] && [[ "$body" == "badauth" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_badauth_no_auth() {
    local test_name="DynDNS v2: badauth with no auth header"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        "${BASE_URL}/nic/update?hostname=test.nodns.shop&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "401" ] && [[ "$body" == "badauth" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_notfqdn_bare_hostname() {
    local test_name="DynDNS v2: notfqdn with bare hostname"

    if ! check_var "NPUB_VALID" || ! check_var "NSEC_VALID"; then
        print_result "$test_name" skip "NPUB_VALID and NSEC_VALID not set"
        return
    fi

    local bare_hostname="example"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${bare_hostname}&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ] && [[ "$body" == "notfqdn" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_dyndns_nohost_wrong_npub() {
    local test_name="DynDNS v2: nohost with hostname from different npub"

    if ! check_var "NPUB_VALID" || ! check_var "NSEC_VALID"; then
        print_result "$test_name" skip "NPUB_VALID and NSEC_VALID not set"
        return
    fi

    # Try to update a hostname that belongs to a different npub
    local hostname="other-npub.nodns.shop"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        --user "${NPUB_VALID}:${NSEC_VALID}" \
        "${BASE_URL}/nic/update?hostname=${hostname}&myip=${IP_V4}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ] && [[ "$body" == "nohost" ]]; then
        print_result "$test_name" pass "Response: $body"
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

# ============================================================================
# ACME-DNS PROTOCOL TESTS
# ============================================================================

test_acme_dns_register() {
    local test_name="acme-dns: Register new account"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        -X POST "${BASE_URL}/register" \
        -H "Content-Type: application/json" \
        -d '{"acmedns":{"npub":"npub1placeholderforregistration"}}')

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "201" ]; then
        # Parse and store credentials for subsequent tests
        ACME_USERNAME=$(echo "$body" | jq -r '.username // empty')
        ACME_PASSWORD=$(echo "$body" | jq -r '.password // empty')
        ACME_FULLDOMAIN=$(echo "$body" | jq -r '.fulldomain // empty')

        if [ -n "$ACME_USERNAME" ] && [ -n "$ACME_PASSWORD" ]; then
            print_result "$test_name" pass "Registered: $ACME_FULLDOMAIN"
        else
            print_result "$test_name" fail "Invalid response format: $body"
        fi
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_acme_dns_update() {
    local test_name="acme-dns: Update TXT record"

    if [ -z "$ACME_USERNAME" ] || [ -z "$ACME_PASSWORD" ]; then
        print_result "$test_name" skip "No credentials available (registration may have failed)"
        return
    fi

    local txt_value="_acme-challenge-test-value-$(date +%s)"
    local subdomain="$ACME_USERNAME"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        -X POST "${BASE_URL}/update" \
        -H "X-Api-User: $ACME_USERNAME" \
        -H "X-Api-Key: $ACME_PASSWORD" \
        -H "Content-Type: application/json" \
        -d "{\"subdomain\":\"$subdomain\",\"txt\":\"$txt_value\"}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "200" ]; then
        local returned_txt=$(echo "$body" | jq -r '.txt // empty')
        if [ "$returned_txt" = "$txt_value" ]; then
            print_result "$test_name" pass "TXT updated: $returned_txt"
        else
            print_result "$test_name" fail "TXT mismatch: expected '$txt_value', got '$returned_txt'"
        fi
    else
        print_result "$test_name" fail "HTTP $http_code, Response: $body"
    fi
}

test_acme_dns_update_wrong_credentials() {
    local test_name="acme-dns: Update with wrong credentials (expect 401)"

    if [ -z "$ACME_USERNAME" ]; then
        print_result "$test_name" skip "No username available (registration may have failed)"
        return
    fi

    local subdomain="$ACME_USERNAME"
    local txt_value="test-value"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        -X POST "${BASE_URL}/update" \
        -H "X-Api-User: $ACME_USERNAME" \
        -H "X-Api-Key: wrong-password" \
        -H "Content-Type: application/json" \
        -d "{\"subdomain\":\"$subdomain\",\"txt\":\"$txt_value\"}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "401" ]; then
        local error_msg=$(echo "$body" | jq -r '.error // empty')
        if [ "$error_msg" = "invalid credentials" ]; then
            print_result "$test_name" pass "Error: $error_msg"
        else
            print_result "$test_name" fail "Wrong error message: $error_msg"
        fi
    else
        print_result "$test_name" fail "Expected 401, got HTTP $http_code, Response: $body"
    fi
}

test_acme_dns_update_subdomain_mismatch() {
    local test_name="acme-dns: Update with subdomain mismatch (expect 403)"

    if [ -z "$ACME_USERNAME" ] || [ -z "$ACME_PASSWORD" ]; then
        print_result "$test_name" skip "No credentials available (registration may have failed)"
        return
    fi

    local wrong_subdomain="wrong-uuid-subdomain"
    local txt_value="test-value"

    local response
    response=$(curl -s -w "\n%{http_code}" \
        -X POST "${BASE_URL}/update" \
        -H "X-Api-User: $ACME_USERNAME" \
        -H "X-Api-Key: $ACME_PASSWORD" \
        -H "Content-Type: application/json" \
        -d "{\"subdomain\":\"$wrong_subdomain\",\"txt\":\"$txt_value\"}")

    local http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | head -n-1)

    if [ "$http_code" = "403" ]; then
        local error_msg=$(echo "$body" | jq -r '.error // empty')
        if [ "$error_msg" = "subdomain mismatch" ]; then
            print_result "$test_name" pass "Error: $error_msg"
        else
            print_result "$test_name" fail "Wrong error message: $error_msg"
        fi
    else
        print_result "$test_name" fail "Expected 403, got HTTP $http_code, Response: $body"
    fi
}

# ============================================================================
# RFC 2136 DNS UPDATE TESTS
# ============================================================================

test_dns_update_add_a_record() {
    local test_name="RFC 2136: Add A record via nsupdate"

    # Check if nsupdate is available
    if ! command -v nsupdate &> /dev/null; then
        print_result "$test_name" skip "nsupdate not available"
        return
    fi

    # Check if TSIG key configuration is available
    if ! check_var "TSIG_KEY_NAME" || ! check_var "TSIG_KEY_SECRET"; then
        print_result "$test_name" skip "TSIG_KEY_NAME and TSIG_KEY_SECRET not set"
        return
    fi

    local test_hostname="test-integration.${BASE_URL#http://}"
    local tsig_key_file="/tmp/nodns-tsig-$$-key"

    # Create TSIG key file
    cat > "$tsig_key_file" <<EOF
key "$TSIG_KEY_NAME" {
    algorithm hmac-sha256;
    secret "$TSIG_KEY_SECRET";
};
EOF

    # Run nsupdate
    local nsupdate_output
    nsupdate_output=$(nsupdate -v "$tsig_key_file" 2>&1 <<EOF
server localhost $DNS_UPDATE_PORT
update add $test_hostname 60 A $IP_V4
send
quit
EOF
)

    local nsupdate_exit_code=$?
    rm -f "$tsig_key_file"

    if [ "$nsupdate_exit_code" = "0" ]; then
        print_result "$test_name" pass "Added A record for $test_hostname"
    else
        print_result "$test_name" fail "nsupdate failed with exit code $nsupdate_exit_code, Output: $nsupdate_output"
    fi
}

test_dns_update_delete_a_record() {
    local test_name="RFC 2136: Delete A record via nsupdate"

    # Check if nsupdate is available
    if ! command -v nsupdate &> /dev/null; then
        print_result "$test_name" skip "nsupdate not available"
        return
    fi

    # Check if TSIG key configuration is available
    if ! check_var "TSIG_KEY_NAME" || ! check_var "TSIG_KEY_SECRET"; then
        print_result "$test_name" skip "TSIG_KEY_NAME and TSIG_KEY_SECRET not set"
        return
    fi

    local test_hostname="test-integration.${BASE_URL#http://}"
    local tsig_key_file="/tmp/nodns-tsig-$$-key"

    # Create TSIG key file
    cat > "$tsig_key_file" <<EOF
key "$TSIG_KEY_NAME" {
    algorithm hmac-sha256;
    secret "$TSIG_KEY_SECRET";
};
EOF

    # Run nsupdate
    local nsupdate_output
    nsupdate_output=$(nsupdate -v "$tsig_key_file" 2>&1 <<EOF
server localhost $DNS_UPDATE_PORT
update delete $test_hostname A
send
quit
EOF
)

    local nsupdate_exit_code=$?
    rm -f "$tsig_key_file"

    if [ "$nsupdate_exit_code" = "0" ]; then
        print_result "$test_name" pass "Deleted A record for $test_hostname"
    else
        print_result "$test_name" fail "nsupdate failed with exit code $nsupdate_exit_code, Output: $nsupdate_output"
    fi
}

test_dns_update_wrong_tsig_key() {
    local test_name="RFC 2136: Update with wrong TSIG key (expect failure)"

    # Check if nsupdate is available
    if ! command -v nsupdate &> /dev/null; then
        print_result "$test_name" skip "nsupdate not available"
        return
    fi

    if ! check_var "TSIG_KEY_NAME"; then
        print_result "$test_name" skip "TSIG_KEY_NAME not set"
        return
    fi

    local test_hostname="test-integration.${BASE_URL#http://}"
    local wrong_secret="invalid-secret-key-for-testing"
    local tsig_key_file="/tmp/nodns-tsig-$$-key"

    # Create TSIG key file with wrong secret
    cat > "$tsig_key_file" <<EOF
key "$TSIG_KEY_NAME" {
    algorithm hmac-sha256;
    secret "$wrong_secret";
};
EOF

    # Run nsupdate
    local nsupdate_output
    nsupdate_output=$(nsupdate -v "$tsig_key_file" 2>&1 <<EOF
server localhost $DNS_UPDATE_PORT
update add $test_hostname 60 A $IP_V4
send
quit
EOF
)

    local nsupdate_exit_code=$?
    rm -f "$tsig_key_file"

    # nsupdate should fail with wrong TSIG key
    if [ "$nsupdate_exit_code" != "0" ]; then
        print_result "$test_name" pass "nsupdate correctly rejected wrong TSIG key"
    else
        print_result "$test_name" fail "nsupdate should have failed with wrong TSIG key"
    fi
}

# ============================================================================
# MAIN TEST RUNNER
# ============================================================================

main() {
    echo "=================================="
    echo "NoDNS Integration Tests"
    echo "=================================="
    echo "Server: $BASE_URL"
    echo "DNS Update Port: $DNS_UPDATE_PORT"
    echo "=================================="
    echo

    # Check for required tools
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}Error: jq is required for JSON parsing${NC}"
        echo "Install with: brew install jq (macOS) or apt-get install jq (Linux)"
        exit 1
    fi

    # Run DynDNS v2 tests
    echo -e "\n${GREEN}=== DynDNS v2 Protocol Tests ===${NC}\n"
    test_dyndns_a_record_update
    test_dyndns_aaaa_record_update
    test_dyndns_nochg_same_ip
    test_dyndns_badauth_wrong_nsec
    test_dyndns_badauth_no_auth
    test_dyndns_notfqdn_bare_hostname
    test_dyndns_nohost_wrong_npub

    # Run acme-dns tests
    echo -e "\n${GREEN}=== acme-dns Protocol Tests ===${NC}\n"
    test_acme_dns_register
    test_acme_dns_update
    test_acme_dns_update_wrong_credentials
    test_acme_dns_update_subdomain_mismatch

    # Run RFC 2136 tests
    echo -e "\n${GREEN}=== RFC 2136 DNS UPDATE Tests ===${NC}\n"
    test_dns_update_add_a_record
    test_dns_update_delete_a_record
    test_dns_update_wrong_tsig_key

    # Print summary
    echo
    echo "=================================="
    echo "Test Summary"
    echo "=================================="
    echo -e "${GREEN}Passed:  $TESTS_PASSED${NC}"
    echo -e "${RED}Failed:  $TESTS_FAILED${NC}"
    echo -e "${YELLOW}Skipped: $TESTS_SKIPPED${NC}"
    echo "=================================="

    # Exit with failure if any tests failed
    if [ "$TESTS_FAILED" -gt 0 ]; then
        exit 1
    fi

    exit 0
}

# Run main
main