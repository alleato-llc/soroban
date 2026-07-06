#!/usr/bin/env bash
#
# Regenerates the cross-engine performance report into the landing page's public/
# directory:
#
#   perf.html   Rust engine vs. Swift engine, evaluating the same Anzan workloads
#               under the Gota protocol (bench/). A same-box/same-run RELATIVE
#               comparison — absolute evals/sec are trend-only (see bench/README.md).
#
# Kept SEPARATE from generate-living-spec.sh: this builds both engines in release
# (cargo + swift) and runs a measure phase, so it's the heavy step. Needs a Rust
# toolchain, Swift, and Python 3. Run from anywhere. Output is committed as a static
# snapshot until CI regenerates it.
#
#   scripts/generate-perf.sh
#
# Env: GOTA_MEASURE / GOTA_WARMUP / GOTA_BUF tune the run (see bench/run.py).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUBLIC="$ROOT/site/public"
mkdir -p "$PUBLIC"

# Build both runners, measure, write bench/results.json + bench/RESULTS.md.
python3 "$ROOT/bench/run.py"

# Render the standalone, self-contained HTML report into the site's public dir.
python3 "$ROOT/bench/report.py" "$ROOT/bench/results.json" \
  -o "$PUBLIC/perf.html" \
  --title "Soroban engine performance — Rust vs Swift"

echo "Wrote $PUBLIC/perf.html"
