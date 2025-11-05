#!/usr/bin/env bash
set -euo pipefail

# Show current code coverage percentage
# This script runs tarpaulin and extracts the coverage percentage

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO_ROOT"

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
  echo "❌ cargo-tarpaulin not found. Install it with:"
  echo "   cargo install cargo-tarpaulin --locked"
  exit 1
fi

echo "🔍 Calculating code coverage..."
echo

# Run tarpaulin and capture output
OUTPUT=$(cargo tarpaulin --out Xml --output-dir coverage 2>&1)

# Extract coverage percentage
COVERAGE=$(echo "$OUTPUT" | grep -oP '\d+\.\d+(?=% coverage)' | head -1)
LINES_COVERED=$(echo "$OUTPUT" | grep -oP '\d+(?=/\d+ lines covered)' | head -1)
LINES_TOTAL=$(echo "$OUTPUT" | grep -oP '(?<=\/)\d+(?= lines covered)' | head -1)

# Display results
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 Code Coverage Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "  Coverage: ${COVERAGE}%"
echo "  Lines:    ${LINES_COVERED}/${LINES_TOTAL}"
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

# Show color-coded badge
if (( $(echo "$COVERAGE >= 80" | bc -l) )); then
    echo "✅ Excellent coverage!"
elif (( $(echo "$COVERAGE >= 60" | bc -l) )); then
    echo "🟢 Good coverage"
elif (( $(echo "$COVERAGE >= 40" | bc -l) )); then
    echo "🟡 Moderate coverage"
elif (( $(echo "$COVERAGE >= 20" | bc -l) )); then
    echo "🟠 Low coverage"
else
    echo "🔴 Very low coverage"
fi

echo
echo "📈 Coverage by module:"
echo "$OUTPUT" | grep -A 20 "|| Tested/Total Lines:" | grep "src/" | sed 's/^|| /  /'

echo
echo "💡 To view detailed HTML report:"
echo "   open coverage/tarpaulin-report.html  # macOS"
echo "   xdg-open coverage/tarpaulin-report.html  # Linux"
