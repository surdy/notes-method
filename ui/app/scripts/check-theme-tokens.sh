#!/bin/bash
# Lint: ensure components use semantic tokens instead of hard-coded colors.
# Scans .svelte and scoped .css files for hex color patterns.
# Ignores: styles/ directory (token definitions), test files, node_modules.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$APP_DIR/src"

# Directories to scan
SCAN_DIRS=(
  "$SRC_DIR/lib"
  "$SRC_DIR/routes"
)

# Find hex colors (#rgb, #rrggbb, #rrggbbaa) in component files
# Exclude: styles/ directory, test files, node_modules
VIOLATIONS=""

for dir in "${SCAN_DIRS[@]}"; do
  if [ -d "$dir" ]; then
    # Match #xxx, #xxxx, #xxxxxx, #xxxxxxxx (3,4,6,8 hex digits)
    # But NOT inside a theme-lint-ignore comment
    FOUND=$(grep -rn --include='*.svelte' --include='*.css' \
      -E '#[0-9a-fA-F]{3,8}\b' "$dir" \
      | grep -v 'theme-lint-ignore' \
      | grep -v '\.test\.' \
      | grep -v '/styles/' \
      || true)
    if [ -n "$FOUND" ]; then
      VIOLATIONS="${VIOLATIONS}${FOUND}\n"
    fi
  fi
done

if [ -n "$VIOLATIONS" ]; then
  echo "❌ Hard-coded hex colors found in component code:"
  echo ""
  echo -e "$VIOLATIONS"
  echo ""
  echo "Fix: Use semantic tokens (--text-default, --bg-surface, etc.) instead."
  echo "If intentional, add /* theme-lint-ignore: reason */ on the same line."
  exit 1
fi

echo "✅ No hard-coded colors found in components."
exit 0
