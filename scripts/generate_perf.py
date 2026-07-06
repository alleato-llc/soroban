#!/usr/bin/env python3
"""Regenerate the cross-engine performance report into the landing page's public/
directory:

    perf.html   Rust engine vs. Swift engine, evaluating the same Anzan workloads
                under the Gota protocol (bench/). A same-box/same-run RELATIVE
                comparison — absolute evals/sec are trend-only (see bench/README.md).

Kept SEPARATE from generate_living_spec.py: this builds both engines in release
(cargo + swift) and runs a measure phase, so it's the heavy step. Needs a Rust
toolchain, Swift, and Python 3. Run from anywhere. Output is committed as a static
snapshot until CI regenerates it.

    scripts/generate_perf.py

Env: GOTA_MEASURE / GOTA_WARMUP / GOTA_BUF tune the run (see bench/run.py).
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUBLIC = ROOT / "site" / "public"


def main() -> None:
    PUBLIC.mkdir(parents=True, exist_ok=True)

    # Build both runners, measure, write bench/results.json + bench/RESULTS.md.
    subprocess.run([sys.executable, str(ROOT / "bench" / "run.py")], check=True)

    # Render the standalone, self-contained HTML report into the site's public dir.
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "bench" / "report.py"),
            str(ROOT / "bench" / "results.json"),
            "-o",
            str(PUBLIC / "perf.html"),
            "--title",
            "Soroban engine performance — Rust vs Swift",
        ],
        check=True,
    )

    print(f"Wrote {PUBLIC / 'perf.html'}")


if __name__ == "__main__":
    main()
