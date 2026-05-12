#!/usr/bin/env bash
# Repacks each profile in profiles/src/<name>/ into profiles/<name>.import
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="$SCRIPT_DIR/src"

for profile_dir in "$SRC_DIR"/*/; do
  name="$(basename "$profile_dir")"
  out="$SCRIPT_DIR/$name.import"

  echo "Building $name.import..."
  rm -f "$out"
  (cd "$profile_dir" && zip -r --quiet "$out" . \
      --exclude "test-files/*" \
      --exclude "*.DS_Store")
  echo "  → $out"
done

echo "Done."
