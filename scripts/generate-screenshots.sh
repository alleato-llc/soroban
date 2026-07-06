#!/usr/bin/env bash
#
# Generates the cross-platform (Rust/iced) app's landing-page screenshot set,
# matching the native (Swift) app's carousel scenes — log / log+inspector /
# grid / grid+inspector, each in the site's two themes (Dracula dark, Solarized
# Light). Output lands in site/public/screenshots/ as rust-<scene>-<theme>.png,
# committed as static assets alongside the Swift shots.
#
# Driven entirely by the permanent env-gated shot harness (rust/gui/src/shot.rs)
# — this script only seeds content and loops scenes × themes. Needs a GPU:
# locally on macOS (real GPU), or headless Linux via
# .github/workflows/screenshots.yml (software Vulkan/lavapipe + xvfb).
#
#   scripts/generate-screenshots.sh [output_dir]
#
# Env:
#   SHOT_PROFILE=debug   build/run the debug binary instead of --release
#                        (faster to iterate locally; pixels are identical)
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/site/public/screenshots}"
GUI="$ROOT/rust/gui"
mkdir -p "$OUT"

PROFILE="${SHOT_PROFILE:-release}"
CARGO_FLAG=""
[ "$PROFILE" = "release" ] && CARGO_FLAG="--release"

# Themes: the two the site itself wears (see site/src/styles/global.css) and the
# Swift shots are matched to — one dark, one light.
THEME_DARK="Dracula"
THEME_LIGHT="Solarized Light"

# Seeds are inline so the script is self-contained and reproducible.
SEED_DIR="$(mktemp -d)"
trap 'rm -rf "$SEED_DIR"' EXIT

# Log scene — a short exact-arithmetic tour (each line submitted to the log):
# the 0.1+0.2 headline, a finance function, a variable, the money + fixed-width
# types, and a recursive function definition + call. Also populates the
# environment inspector (rate, fib, Decimal…).
cat > "$SEED_DIR/log.txt" <<'SEED'
0.1 + 0.2
pmt(0.0825 / 12, 360, -350000)
rate = 0.0825
Decimal(10.50, 5, 2)
Int32(255) + 1
fib(n) = if(n < 2, n, fib(n-1) + fib(n-2))  # classic
fib(20)
SEED

# Grid scene — a mortgage what-if model built via the log's updateCell
# reflection (updateCell takes a cell HANDLE + a value): labels, inputs, a live
# slider on the rate, and rounded pmt / total-interest formulas. A couple of log
# variables so the inspector has environment content in the grid view too.
cat > "$SEED_DIR/grid.txt" <<'SEED'
rate = 0.0825
principal = 350000
updateCell(cell("A", 1), "Loan amount")
updateCell(cell("B", 1), 350000)
updateCell(cell("A", 2), "Annual rate")
updateCell(cell("B", 2), "=slider(0.0825, 0.02, 0.12, 0.0025)")
updateCell(cell("A", 3), "Term (mo)")
updateCell(cell("B", 3), 360)
updateCell(cell("A", 5), "Payment")
updateCell(cell("B", 5), "=round(pmt(B:2/12, B:3, -B:1), 2)")
updateCell(cell("A", 6), "Total interest")
updateCell(cell("B", 6), "=round(B:5 * B:3 - B:1, 2)")
SEED

# One capture: a FRESH data dir (so no persisted log/grid state bleeds in), a
# scene, a theme, and any extra SOROBAN_SHOT_* knobs passed as trailing KEY=VAL.
shot() { # <out-name> <view: log|grid> <theme> <seed-file> [EXTRA_ENV=…]
  local name="$1" view="$2" theme="$3" seed="$4"
  shift 4
  local data
  data="$(mktemp -d)"
  (
    cd "$GUI"
    env \
      SOROBAN_DATA_DIR="$data" \
      SOROBAN_SHOT="$OUT/$name.png" \
      SOROBAN_SHOT_SEED="$SEED_DIR/$seed" \
      SOROBAN_SHOT_VIEW="$view" \
      SOROBAN_SHOT_THEME="$theme" \
      "$@" \
      cargo run -q $CARGO_FLAG
  )
  rm -rf "$data"
  echo "  → $name.png"
}

echo "Generating Rust app screenshots into $OUT (profile: $PROFILE)…"

# log
shot "rust-log-dark"            log  "$THEME_DARK"  log.txt
shot "rust-log-light"           log  "$THEME_LIGHT" log.txt
# log + environment inspector
shot "rust-log-inspector-dark"  log  "$THEME_DARK"  log.txt  SOROBAN_SHOT_PANEL=inspector
shot "rust-log-inspector-light" log  "$THEME_LIGHT" log.txt  SOROBAN_SHOT_PANEL=inspector
# grid (rate slider selected, showing the formula bar)
shot "rust-grid-dark"           grid "$THEME_DARK"  grid.txt SOROBAN_SHOT_SELECT=B2
shot "rust-grid-light"          grid "$THEME_LIGHT" grid.txt SOROBAN_SHOT_SELECT=B2
# grid + environment inspector
shot "rust-grid-inspector-dark"  grid "$THEME_DARK"  grid.txt SOROBAN_SHOT_PANEL=inspector
shot "rust-grid-inspector-light" grid "$THEME_LIGHT" grid.txt SOROBAN_SHOT_PANEL=inspector

echo "Done — 8 screenshots in $OUT."
