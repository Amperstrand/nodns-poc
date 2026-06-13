#!/usr/bin/env bash
#
# deploy.sh — Build, verify, and deploy nodns-bot to the VPS
#
# This script ensures the deployed binary is always fresh by building from
# source. It also runs the expiry check to ensure operator_lease_expires
# matches the actual domain registration.
#
# Usage:
#   ./deploy/deploy.sh                  # Build + verify + show deploy commands
#   ./deploy/deploy.sh --push           # Build + verify + SCP + restart
#
# Prerequisites:
#   - cargo-zigbuild installed (cargo install cargo-zigbuild)
#   - zig installed (brew install zig)
#   - SSH access to root@46.224.104.12
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BOT_DIR="${REPO_ROOT}/nodns-bot-rs"
VPS_HOST="root@46.224.104.12"
REMOTE_PATH="/opt/nodns-bot"
TARGET="x86_64-unknown-linux-gnu"
PUSH=false

if [ "${1:-}" = "--push" ]; then
    PUSH=true
fi

echo "=========================================="
echo "  NoDNS Deploy"
echo "=========================================="

# --- Step 1: Check domain expiry ---
echo ""
echo "[1/4] Checking domain expiry..."
bash "${SCRIPT_DIR}/check-expiry.sh"

# --- Step 2: Run tests ---
echo ""
echo "[2/4] Running tests..."
(cd "$BOT_DIR" && cargo test --quiet 2>&1 | tail -3)

# --- Step 3: Build release binary ---
echo ""
echo "[3/4] Cross-compiling for Linux x86_64..."
echo "  (This takes ~10 minutes on first run, ~2 minutes incremental)"
(cd "$BOT_DIR" && cargo zigbuild --release --target "$TARGET")

BINARY="${BOT_DIR}/target/${TARGET}/release/nodns-bot"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at ${BINARY}"
    exit 1
fi

FILE_INFO=$(file "$BINARY")
echo "  Built: $(ls -lh "$BINARY" | awk '{print $5, $6, $7, $8}')"
echo "  Type: $(echo "$FILE_INFO" | cut -d, -f1)"

# --- Step 4: Deploy or show instructions ---
echo ""
echo "[4/4] Deploy..."

if [ "$PUSH" = true ]; then
    echo "  Uploading binary to VPS..."
    scp "$BINARY" "${VPS_HOST}:${REMOTE_PATH}/nodns-bot"
    echo "  Restarting service..."
    ssh "$VPS_HOST" "systemctl restart nodns-bot && sleep 2 && systemctl status nodns-bot --no-pager"
    echo ""
    echo "  Deployed! Verify with:"
    echo "    ssh ${VPS_HOST} 'curl -s http://127.0.0.1:9090/health'"
else
    echo "  Binary ready at: ${BINARY}"
    echo ""
    echo "  To deploy, run:"
    echo "    ./deploy/deploy.sh --push"
    echo ""
    echo "  Or manually:"
    echo "    scp ${BINARY} ${VPS_HOST}:${REMOTE_PATH}/nodns-bot"
    echo "    ssh ${VPS_HOST} 'systemctl restart nodns-bot'"
fi

echo ""
echo "=========================================="
echo "  Done"
echo "=========================================="
