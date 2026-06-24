#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="dist"
NSEC_FILE="${NSITE_NSEC_FILE:-$HOME/.config/nodns-registrar/nsite_nsec}"

if ! command -v nsyte &>/dev/null; then
  echo "ERROR: nsyte CLI not installed."
  echo "Install: npm i -g @nsyte/cli"
  echo "Or:      curl -L https://github.com/sandwichfarm/nsyte/releases/latest/download/nsyte-$(uname -s)-$(uname -m) -o /usr/local/bin/nsyte && chmod +x /usr/local/bin/nsyte"
  exit 1
fi

if [ ! -f "$NSEC_FILE" ]; then
  echo "ERROR: nsec file not found at $NSEC_FILE"
  echo "Generate one:"
  echo "  mkdir -p ~/.config/nodns-registrar && chmod 700 ~/.config/nodns-registrar"
  echo "  nsyte keygen > ~/.config/nodns-registrar/nsite_nsec"
  echo "  chmod 600 ~/.config/nodns-registrar/nsite_nsec"
  exit 1
fi

if [ ! -d "$DIST_DIR" ]; then
  echo "Building SPA..."
  npm run build
fi

echo "Deploying SPA to Nostr as nsite..."
nsyte deploy "$DIST_DIR" \
  --name nodns-registrar \
  --fallback /index.html \
  --sec "$(cat "$NSEC_FILE")" \
  -i

echo ""
echo "nsite deployed. Accessible at:"
echo "  https://<npub>.nsite.lol/"
echo ""
echo "Check status:"
echo "  nsyte status --name nodns-registrar --full"
