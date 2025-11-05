#!/usr/bin/env bash
set -euo pipefail

# Coverage helper for homemetrics
# - Requires cargo-tarpaulin (https://github.com/xd009642/tarpaulin)
# - Generates HTML and XML coverage reports into ./coverage/

# Recommend running from repo root
REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO_ROOT"

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
  echo "cargo-tarpaulin not found. Install it with:"
  echo "  cargo install cargo-tarpaulin --locked"
  echo
  echo "On CI (GitHub Actions) we install it automatically. To continue locally, install first."
  exit 1
fi

# Ensure output dir
OUT_DIR=coverage
mkdir -p "$OUT_DIR"

# Run tarpaulin producing XML (cobertura) and HTML outputs.
# Note: tarpaulin may require nightly or specific environment on some platforms.
# Try XML first (good for CI/codecov), then HTML for local browsing.

echo "Running cargo tarpaulin (XML)..."
cargo tarpaulin --out Xml --output-dir "$OUT_DIR"

echo "Running cargo tarpaulin (HTML)..."
# Some versions need separate run for HTML
cargo tarpaulin --out Html --output-dir "$OUT_DIR"

echo "Coverage reports generated in $OUT_DIR/."
echo "Open $OUT_DIR/index.html in a browser to view the HTML report."