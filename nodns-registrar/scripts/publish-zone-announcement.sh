#!/usr/bin/env bash
set -euo pipefail

# Publish a kind:31990 zone announcement event to relay.cashu.email
#
# Usage:
#   ZONE_NSEC=nsec1... ./publish-zone-announcement.sh             # testing mode
#   ZONE_NSEC=nsec1... ./publish-zone-announcement.sh --production # production mode
#
# Requires: nak (npm i -g @noble/secp256k1 or brew install nak)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR/.."

SEC="${ZONE_NSEC:-${NOSTR_SECRET_KEY:-}}"
if [ -z "$SEC" ]; then
  echo "ERROR: Set ZONE_NSEC or NOSTR_SECRET_KEY env var"
  exit 1
fi

if ! command -v nak &>/dev/null; then
  echo "ERROR: nak not found. Install: go install github.com/fiatjaf/nak@latest"
  exit 1
fi

MODE="${1:-testing}"
RELAY="wss://relay.cashu.email"

ZONE="nodns.shop"
DNSKEY_HASH="aca4c1968ae4ecda5f2eaf245207b7a3a36a55a620d631573abaddd3c7449d01"
WEB_URL="https://nodns-registrar.pages.dev/"
CONTENT='{"name":"nodns.shop","about":"Decentralized DNS zone operator","website":"https://nodns.shop","nip05":"_nodns@nodns.shop"}'

TAGS=(
  -t "d=nodns-registrar"
  -t "k=11111"
  -t "zone=${ZONE}"
  -t "dnskey_hash=${DNSKEY_HASH}"
  -t "dnskey_alg=ECDSAP256SHA256"
  -t "pricing=create=2;update=0;delete=0"
  -t "mint=testnut.cashu.space"
  -t "web=${WEB_URL}"
)

if [ "$MODE" = "--production" ]; then
  TAGS+=(-t "status=production;Live and operational")
  echo "Publishing PRODUCTION zone announcement to ${RELAY}..."
else
  TAGS+=(-t "testnet=")
  TAGS+=(-t "status=testing;Best-effort pilot - not yet fully implemented")
  echo "Publishing TESTING zone announcement to ${RELAY}..."
fi

nak event \
  --sec "$SEC" \
  -k 31990 \
  -c "$CONTENT" \
  "${TAGS[@]}" \
  "$RELAY"

echo ""
echo "Done. Verify with:"
echo "  nak event -k 31990 --relay ${RELAY} | jq '.tags'"
echo ""
if [ "$MODE" = "--production" ]; then
  echo "Zone is now announced as PRODUCTION."
else
  echo "Zone is announced as TESTING. To go production:"
  echo "  ZONE_NSEC=$SEC $0 --production"
fi
