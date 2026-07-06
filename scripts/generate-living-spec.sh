#!/usr/bin/env bash
#
# Regenerates all three verification pages into the landing page's public/
# directory, from the SAME shared spec/anzan/*.feature files:
#
#   spec.html         the Living Specification — the behavior prose, engine-
#                     NEUTRAL, rendered by the Rust generator
#                     (rust/engine/tests/living_spec.rs). The front door; it
#                     cross-links both engine reports below.
#   report.html       the native (Swift) engine's test report (PickleKit).
#   rust-report.html  the cross-platform (Rust) engine's test report
#                     (cucumber JSON → scripts/rust-report.mjs).
#
# Needs Swift (report.html), a Rust toolchain (spec.html + rust-report.html),
# and Node (the rust-report converter). Run from anywhere. Output is committed
# as a static snapshot until CI regenerates it.
#
#   scripts/generate-living-spec.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUBLIC="$ROOT/site/public"
mkdir -p "$PUBLIC"

# 1. Native (Swift) engine → report.html only. (The living spec is Rust-rendered
#    now; dropping PICKLE_SPEC_PATH leaves PickleKit emitting just the report.)
( cd "$ROOT/swift/Engine"
  PICKLE_REPORT=1 \
    PICKLE_REPORT_PATH="$PUBLIC/report.html" \
    swift test --filter GherkinTests )

# 2. Living spec (engine-neutral) — the Rust generator parses spec/anzan and
#    renders spec.html matching the report's design.
( cd "$ROOT/rust"
  SOROBAN_SPEC="$PUBLIC/spec.html" cargo test -p soroban-engine --test living_spec )

# 3. Cross-platform (Rust) engine → rust-report.html, from the same features.
( cd "$ROOT/rust"
  SOROBAN_REPORT="$PUBLIC/rust-cucumber.json" cargo test -p soroban-engine --test gherkin )
node "$ROOT/scripts/rust-report.mjs" "$PUBLIC/rust-cucumber.json" "$PUBLIC/rust-report.html"
rm -f "$PUBLIC/rust-cucumber.json"

echo "Wrote $PUBLIC/{spec,report,rust-report}.html"
