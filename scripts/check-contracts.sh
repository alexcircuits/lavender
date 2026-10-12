#!/usr/bin/env bash
# Regenerates the TypeScript contract from lavender-domain into a temporary directory and
# fails if it differs from the committed apps/web/src/contracts/generated (ADR 0007).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
(cd "$root" && TS_RS_EXPORT_DIR="$tmp" cargo test -q -p lavender-domain export_bindings >/dev/null)
if ! diff -ru "$root/apps/web/src/contracts/generated" "$tmp"; then
  echo "TypeScript contracts are out of date: run 'cargo test -p lavender-domain' and commit the result." >&2
  exit 1
fi
echo "TypeScript contracts are up to date."
