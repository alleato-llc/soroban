#!/usr/bin/env bash
#
# Generates the Living Specification + interactive test report into the
# landing page's public/ directory, straight from the engine's Gherkin
# suite (PickleKit's ReportSuite). The site build serves them as
# /spec.html and /report.html; the spec is the front door, the report the
# drill-down, and they cross-link.
#
# Run from anywhere. The output is committed as a static snapshot until CI
# regenerates it on release.
#
#   scripts/generate-living-spec.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUBLIC="$ROOT/site/public"
mkdir -p "$PUBLIC"

cd "$ROOT/Engine"
PICKLE_REPORT=1 \
  PICKLE_REPORT_PATH="$PUBLIC/report.html" \
  PICKLE_SPEC_PATH="$PUBLIC/spec.html" \
  PICKLE_SPEC_TITLE="Anzan — Living Specification" \
  swift test --filter GherkinTests

echo "Wrote $PUBLIC/spec.html and $PUBLIC/report.html"
