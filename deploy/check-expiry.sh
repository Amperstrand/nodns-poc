#!/usr/bin/env bash
#
# check-expiry.sh — Verify deploy/config-multi-zone.toml matches actual domain expiry via RDAP
#
# Usage:
#   ./deploy/check-expiry.sh                 # Check only
#   ./deploy/check-expiry.sh --update        # Update config if mismatch
#
# Exit codes:
#   0 — config matches RDAP (or was updated with --update)
#   1 — mismatch detected (run with --update to fix)
#   2 — RDAP query failed (network error, etc.)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/config-multi-zone.toml"
DOMAIN="nodns.shop"
RDAP_URL="https://rdap.org/domain/${DOMAIN}"

UPDATE=false
if [ "${1:-}" = "--update" ]; then
    UPDATE=true
fi

# --- Query RDAP for actual domain expiry ---
echo "[check-expiry] Querying RDAP for ${DOMAIN}..."

ACTUAL_EXPIRY=$(curl -sL "$RDAP_URL" 2>/dev/null | python3 -c "
import json, sys
data = json.load(sys.stdin)
for event in data.get('events', []):
    if event.get('eventAction') == 'expiration':
        print(event['eventDate'][:10])
        sys.exit(0)
print('ERROR: No expiration event in RDAP response', file=sys.stderr)
sys.exit(1)
" 2>/dev/null) || {
    echo "[check-expiry] FAILED: Could not query RDAP for ${DOMAIN}"
    echo "[check-expiry] URL: ${RDAP_URL}"
    exit 2
}

echo "[check-expiry] RDAP expiry:   ${ACTUAL_EXPIRY}"

# --- Extract configured expiry from config ---
CONFIG_EXPIRY=$(grep 'operator_lease_expires' "$CONFIG_FILE" 2>/dev/null | head -1 | sed 's/.*"\(.*\)".*/\1/' || echo "")

if [ -z "$CONFIG_EXPIRY" ]; then
    echo "[check-expiry] WARNING: No operator_lease_expires found in ${CONFIG_FILE}"
    if [ "$UPDATE" = true ]; then
        echo "[check-expiry] --update specified but no existing value to replace. Edit config manually."
        exit 1
    fi
    exit 1
fi

echo "[check-expiry] Config expiry: ${CONFIG_EXPIRY}"

# --- Compare ---
if [ "$ACTUAL_EXPIRY" = "$CONFIG_EXPIRY" ]; then
    echo "[check-expiry] OK — config matches RDAP."
    exit 0
fi

echo "[check-expiry] MISMATCH: config (${CONFIG_EXPIRY}) != RDAP (${ACTUAL_EXPIRY})"

if [ "$UPDATE" = true ]; then
    echo "[check-expiry] Updating ${CONFIG_FILE}..."
    # Use sed to replace the date in-place (macOS sed compatible)
    sed -i.bak "s/operator_lease_expires = \"${CONFIG_EXPIRY}\"/operator_lease_expires = \"${ACTUAL_EXPIRY}\"/" "$CONFIG_FILE"
    rm -f "${CONFIG_FILE}.bak"
    echo "[check-expiry] Updated operator_lease_expires to ${ACTUAL_EXPIRY}"
    exit 0
fi

echo "[check-expiry] Run with --update to sync config, or edit ${CONFIG_FILE} manually."
exit 1
