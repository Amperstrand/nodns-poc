#!/usr/bin/env bash
set -euo pipefail

PURGE_LOG="/var/log/nodns-purge.log"
DRY_RUN="${1:-}"

log() { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$PURGE_LOG"; }

WHITELIST_PATTERNS=(
  "*.utxo"
  "utxo*"
  "*bitcoin*chainstate*"
  "*.key"
  "*.pem"
  "config.toml"
  "records.db"
  "knot.conf"
)

is_whitelisted() {
  local file="$1"
  for pattern in "${WHITELIST_PATTERNS[@]}"; do
    if [[ "$file" == $pattern ]]; then return 0; fi
  done
  return 1
}

purge_dir() {
  local dir="$1" max_age_days="$2" label="$3"
  if [ ! -d "$dir" ]; then return; fi
  local count
  count=$(find "$dir" -type f -mtime +"$max_age_days" ! -name "*.lock" 2>/dev/null | wc -l)
  if [ "$count" -eq 0 ]; then return; fi
  log "$label: found $count files older than ${max_age_days}d"
  if [ "$DRY_RUN" = "--dry" ]; then
    log "  (dry run — skipping)"
    find "$dir" -type f -mtime +"$max_age_days" ! -name "*.lock" -exec ls -lh {} \; 2>/dev/null | head -10
    return
  fi
  find "$dir" -type f -mtime +"$max_age_days" ! -name "*.lock" -delete 2>/dev/null
  log "  purged"
}

purge_pattern() {
  local base="$1" pattern="$2" max_age_days="$3" label="$4"
  local count
  count=$(find "$base" -name "$pattern" -type f -mtime +"$max_age_days" 2>/dev/null | wc -l)
  if [ "$count" -eq 0 ]; then return; fi
  log "$label: found $count files matching '$pattern' older than ${max_age_days}d"
  if [ "$DRY_RUN" = "--dry" ]; then
    log "  (dry run — skipping)"
    find "$base" -name "$pattern" -type f -mtime +"$max_age_days" -exec ls -lh {} \; 2>/dev/null | head -10
    return
  fi
  find "$base" -name "$pattern" -type f -mtime +"$max_age_days" -delete 2>/dev/null
  log "  purged"
}

log "=== nodns VPS purge started ==="

purge_dir "/tmp" 7 "Temp files"
purge_dir "/var/log" 30 "Old logs"
purge_pattern "/opt/nodns-bot" "*.bak*" 7 "Old bot binaries"
purge_pattern "/root" "fio*" 1 "fio benchmarks"
purge_pattern "/root" "seq-*" 1 "Disk test files"
purge_pattern "/root" "rand-*" 1 "Disk test files"

log "Checking rustup toolchains..."
if command -v rustup &>/dev/null; then
  INSTALLED=$(rustup toolchain list 2>/dev/null)
  STABLE=$(echo "$INSTALLED" | grep "stable" | head -1 | awk '{print $1}')
  for tc in $(echo "$INSTALLED" | grep -v "stable" | grep -v "nightly" | awk '{print $1}'); do
    log "  removing old toolchain: $tc"
    if [ "$DRY_RUN" != "--dry" ]; then
      rustup toolchain remove "$tc" 2>/dev/null || true
    fi
  done
fi

log "Checking npm cache..."
if [ -d "/root/.npm/_cacache" ]; then
  SIZE=$(du -sh /root/.npm/_cacache 2>/dev/null | awk '{print $1}')
  log "  npm cache: $SIZE"
fi

log "Checking cargo registry cache..."
if [ -d "/root/.cargo/registry/cache" ]; then
  SIZE=$(du -sh /root/.cargo/registry/cache 2>/dev/null | awk '{print $1}')
  log "  cargo cache: $SIZE"
fi

log "Vacuuming journal..."
if [ "$DRY_RUN" != "--dry" ]; then
  journalctl --vacuum-time=7d 2>&1 | tee -a "$PURGE_LOG"
fi

log "=== purge complete ==="
log "Disk usage: $(df -h / | tail -1 | awk '{print $3 " used / " $4 " available (" $5 ")"}')"
