#!/usr/bin/env bash
# Lavender performance harness (ENG-1000): scripts/perf.sh bench | load | browser | all
# Method and targets: docs/performance/specification.md. Usage notes: scripts/README.md.
#
#   bench    criterion benchmarks (normalisation, hub snapshot of 50 000 aircraft, bbox
#            filter, region keys, SGP4 batch)
#   load     release lavender-server against wiremock upstreams: HTTP latency, WebSocket
#            fan-out, upstream calls per key, RSS; then in fixture mode, worldwide snapshots
#            of the dense synthetic populations     (ports 7428 and 7429)
#   browser  production build in headless Chrome via Playwright + CDP, dense synthetic
#            fixtures: startup, frame rates, keyed updates, heap   (port 7428)
#   all      all three, in that order
#
# Results go to $LAVENDER_PERF_OUT (default ${TMPDIR:-/tmp}/lavender-perf/results/<UTC time>).
# Builds use $CARGO_TARGET_DIR when it is set.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

target="${CARGO_TARGET_DIR:-$root/target}"
out="${LAVENDER_PERF_OUT:-${TMPDIR:-/tmp}/lavender-perf/results/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$out"

require_free_port() {
  if command -v lsof >/dev/null && lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "perf: port $1 is in use; the harness only measures servers it starts itself." >&2
    exit 1
  fi
}

# Other work on the machine skews every number: record the load around each part.
note_load() {
  echo "load_average_$1: $(uptime | sed 's/.*load average[s]*: //') at $(date -u +%H:%M:%SZ)" >>"$out/machine.txt"
}

run_part() {
  note_load "before_$1"
  "$1"
  note_load "after_$1"
}

record_machine() {
  {
    echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "commit: $(git rev-parse HEAD 2>/dev/null || echo unknown) (uncommitted changes: $(git status --porcelain 2>/dev/null | wc -l | tr -d ' ') paths)"
    echo "kernel: $(uname -srm)"
    if command -v sw_vers >/dev/null; then
      echo "os: $(sw_vers -productName) $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
      echo "model: $(sysctl -n hw.model)"
      echo "cpu: $(sysctl -n machdep.cpu.brand_string), $(sysctl -n hw.physicalcpu) physical / $(sysctl -n hw.logicalcpu) logical cores"
      echo "memory_gib: $(($(sysctl -n hw.memsize) / 1073741824))"
      echo "power: $(pmset -g batt 2>/dev/null | head -1 | sed "s/.*'\(.*\)'.*/\1/")"
    else
      echo "os: $(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME")"
      echo "cpu: $(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2-)"
      echo "memory_gib: $(($(grep MemTotal /proc/meminfo | awk '{print $2}') / 1048576))"
    fi
    echo "rustc: $(rustc --version)"
    echo "node: $(node --version)"
  } >>"$out/machine.txt"
}

bench() {
  cargo bench --locked -p lavender-geo --bench geo 2>&1 | tee "$out/bench-geo.txt"
  cargo bench --locked -p lavender-core --bench normalise 2>&1 | tee "$out/bench-normalise.txt"
  cargo bench --locked -p lavender-core --bench snapshot 2>&1 | tee "$out/bench-snapshot.txt"
}

# Runs the TypeScript fixture generator on every Node the web package allows (>= 22.12).
fixtures() {
  node --experimental-strip-types apps/web/perf/fixtures.ts "$@"
}

load() {
  require_free_port 7428
  require_free_port 7429
  cargo build --release --locked -p lavender-server --bin lavender-server --example perf_load
  # The dense phase serves the browser scenarios' SYNTHETIC populations in fixture mode.
  local dense="${TMPDIR:-/tmp}/lavender-perf/load-fixtures"
  fixtures --out "$dense" --aircraft 20000
  "$target/release/examples/perf_load" --server "$target/release/lavender-server" \
    --fixtures "$dense" --out "$out/load.json" >/dev/null
  echo "perf: load results in $out/load.json"
}

browser() {
  require_free_port 7428
  cargo build --release --locked -p lavender-server --bin lavender-server
  # The production build also runs check-dist, the bundle budget gate.
  pnpm --dir apps/web run build
  (
    cd apps/web
    LAVENDER_PERF_OUT="$out" CARGO_TARGET_DIR="$target" \
      PLAYWRIGHT_CHANNEL="${PLAYWRIGHT_CHANNEL-chrome}" \
      pnpm exec playwright test -c perf/playwright.perf.config.ts
  )
}

case "${1:-}" in
  bench | load | browser)
    record_machine
    run_part "$1"
    ;;
  all)
    record_machine
    run_part bench
    run_part load
    run_part browser
    ;;
  *)
    echo "usage: scripts/perf.sh bench | load | browser | all" >&2
    exit 2
    ;;
esac
echo "perf: results in $out"
