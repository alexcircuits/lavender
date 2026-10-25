#!/usr/bin/env bash
# Rebuilds the committed Zig WASM kernel artifact from source (LB-130, ADR 0009).
#
# The artifact at apps/web/public/wasm/lavender_kernels.wasm is committed so the
# web app and CI never need a Zig toolchain. This script is how it is rebuilt:
# it pins the toolchain, runs the Zig test suite, builds the freestanding
# ReleaseSmall module and copies it into place. Run it after changing anything
# under kernels/zig and commit the refreshed artifact together with the source.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
artifact="$root/apps/web/public/wasm/lavender_kernels.wasm"

# The Zig version the committed artifact is built with. 0.16-dev moves fast;
# a mismatch fails loudly instead of producing a subtly different binary.
pinned_zig="0.16.0"
version="$(zig version 2>/dev/null || true)"
if [ "$version" != "$pinned_zig" ]; then
  echo "error: zig ${pinned_zig} is required (found: ${version:-none on PATH})." >&2
  echo "       install with: zigup ${pinned_zig}" >&2
  exit 1
fi

cd "$root/kernels/zig"
echo "== zig build test"
zig build test
echo "== zig build wasm (wasm32-freestanding, ReleaseSmall)"
zig build -Doptimize=ReleaseSmall -Dtarget=wasm32-freestanding wasm

built="zig-out/wasm/lavender_kernels.wasm"
if [ ! -f "$built" ]; then
  echo "error: expected build output at kernels/zig/${built}" >&2
  exit 1
fi
cp "$built" "$artifact"
echo "== installed $(basename "$artifact")"
shasum -a 256 "$artifact"
