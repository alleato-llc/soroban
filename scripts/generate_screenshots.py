#!/usr/bin/env python3
"""Generate the cross-platform (Rust/iced) app's landing-page screenshot set,
matching the native (Swift) app's carousel scenes — log / log+inspector /
grid / grid+inspector, each in the site's two themes (Dracula dark, Solarized
Light). Output lands in site/public/screenshots/ as rust-<scene>-<theme>.png,
committed as static assets alongside the Swift shots.

Driven entirely by the permanent env-gated shot harness (rust/gui/src/shot.rs)
— this script only seeds content and loops scenes x themes. Needs a GPU:
locally on macOS (real GPU), or headless Linux via
.github/workflows/screenshots.yml (software Vulkan/lavapipe + xvfb).

    scripts/generate_screenshots.py [output_dir]

Env:
  SHOT_PROFILE=debug   build/run the debug binary instead of --release
                       (faster to iterate locally; pixels are identical)
"""

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Seeds are inline so the script is self-contained and reproducible.

# Log scene — a short exact-arithmetic tour (each line submitted to the log):
# the 0.1+0.2 headline, a finance function, a variable, the money + fixed-width
# types, and a recursive function definition + call. Also populates the
# environment inspector (rate, fib, Decimal…).
LOG_SEED = """\
0.1 + 0.2
pmt(0.0825 / 12, 360, -350000)
rate = 0.0825
Decimal(10.50, 5, 2)
Int32(255) + 1
fib(n) = if(n < 2, n, fib(n-1) + fib(n-2))  # classic
fib(20)
"""

# Grid scene — a mortgage what-if model built via the log's updateCell
# reflection (updateCell takes a cell HANDLE + a value): labels, inputs, a live
# slider on the rate, and rounded pmt / total-interest formulas. A couple of log
# variables so the inspector has environment content in the grid view too.
GRID_SEED = """\
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
"""

# Themes: the two the site itself wears (see site/src/styles/global.css) and the
# Swift shots are matched to — one dark, one light.
THEME_DARK = "Dracula"
THEME_LIGHT = "Solarized Light"


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "site" / "public" / "screenshots"
    gui = ROOT / "rust" / "gui"
    out.mkdir(parents=True, exist_ok=True)

    profile = os.environ.get("SHOT_PROFILE", "release")
    cargo_flags = ["--release"] if profile == "release" else []

    seed_dir = Path(tempfile.mkdtemp())
    try:
        (seed_dir / "log.txt").write_text(LOG_SEED)
        (seed_dir / "grid.txt").write_text(GRID_SEED)

        def shot(name: str, view: str, theme: str, seed: str, **extra_env: str) -> None:
            """One capture: a FRESH data dir (so no persisted log/grid state
            bleeds in), a scene, a theme, and any extra SOROBAN_SHOT_* knobs."""
            data = Path(tempfile.mkdtemp())
            try:
                env = {
                    **os.environ,
                    "SOROBAN_DATA_DIR": str(data),
                    "SOROBAN_SHOT": str(out / f"{name}.png"),
                    "SOROBAN_SHOT_SEED": str(seed_dir / seed),
                    "SOROBAN_SHOT_VIEW": view,
                    "SOROBAN_SHOT_THEME": theme,
                    **extra_env,
                }
                subprocess.run(["cargo", "run", "-q", *cargo_flags], cwd=gui, check=True, env=env)
            finally:
                shutil.rmtree(data, ignore_errors=True)
            print(f"  → {name}.png")

        print(f"Generating Rust app screenshots into {out} (profile: {profile})…")

        # log
        shot("rust-log-dark", "log", THEME_DARK, "log.txt")
        shot("rust-log-light", "log", THEME_LIGHT, "log.txt")
        # log + environment inspector
        shot("rust-log-inspector-dark", "log", THEME_DARK, "log.txt", SOROBAN_SHOT_PANEL="inspector")
        shot("rust-log-inspector-light", "log", THEME_LIGHT, "log.txt", SOROBAN_SHOT_PANEL="inspector")
        # grid (rate slider selected, showing the formula bar)
        shot("rust-grid-dark", "grid", THEME_DARK, "grid.txt", SOROBAN_SHOT_SELECT="B2")
        shot("rust-grid-light", "grid", THEME_LIGHT, "grid.txt", SOROBAN_SHOT_SELECT="B2")
        # grid + environment inspector
        shot("rust-grid-inspector-dark", "grid", THEME_DARK, "grid.txt", SOROBAN_SHOT_PANEL="inspector")
        shot("rust-grid-inspector-light", "grid", THEME_LIGHT, "grid.txt", SOROBAN_SHOT_PANEL="inspector")

        print(f"Done — 8 screenshots in {out}.")
    finally:
        shutil.rmtree(seed_dir, ignore_errors=True)


if __name__ == "__main__":
    main()
