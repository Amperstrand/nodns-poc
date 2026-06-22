#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="${SCRIPT_DIR}"

echo "Building nodns-registrar..."
cd "$PROJECT_DIR"
npm run build

if [ ! -d "out" ]; then
  echo "ERROR: Build output 'out/' not found"
  exit 1
fi

echo "Deploying to Cloudflare Pages..."
if [ "${1:-}" = "--deploy" ]; then
  npx wrangler pages deploy out --project-name nodns-registrar
else
  echo "Built successfully. Deploy with: ./deploy.sh --deploy"
  echo "Or connect this repo to Cloudflare Pages for automatic deploys."
fi
