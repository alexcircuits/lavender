#!/usr/bin/env bash
# Verify the Rust backend and built web app without desktop system dependencies.
# Set LAVENDER_CHECK_DESKTOP=1 to include the Tauri crate on a prepared machine.
set -euo pipefail
lavender_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$lavender_root"

rust_scope=(--workspace --exclude lavender-desktop)
if [[ "${LAVENDER_CHECK_DESKTOP:-0}" == "1" ]]; then
  rust_scope=(--workspace)
fi

cargo fmt --all --check
cargo clippy "${rust_scope[@]}" --all-targets --locked -- -D warnings
cargo test "${rust_scope[@]}" --locked
cargo build --locked -p lavender-server
cargo deny --locked check
./scripts/check-contracts.sh
pnpm --dir apps/web run typecheck
pnpm --dir apps/web run test
pnpm --dir apps/web run lint
pnpm --dir apps/web run build
