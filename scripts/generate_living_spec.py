#!/usr/bin/env python3
"""Regenerate all three verification pages into the landing page's public/
directory, from the SAME shared spec/anzan/*.feature files:

    spec.html         the Living Specification — the behavior prose, engine-
                      NEUTRAL, rendered by the Rust generator
                      (rust/engine/tests/living_spec.rs). The front door; it
                      cross-links both engine reports below.
    report.html       the native (Swift) engine's test report (PickleKit).
    rust-report.html  the cross-platform (Rust) engine's test report
                      (cucumber JSON -> scripts/rust-report.mjs).

Needs Swift (report.html), a Rust toolchain (spec.html + rust-report.html),
and Node (the rust-report converter). Run from anywhere. Output is committed
as a static snapshot until CI regenerates it.

    scripts/generate_living_spec.py
"""

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUBLIC = ROOT / "site" / "public"


def run(cmd: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> None:
    """Run `cmd`, inheriting the environment plus any `env` overrides, and fail
    loudly on a non-zero exit (the shell script's `set -euo pipefail`)."""
    subprocess.run(cmd, cwd=cwd, check=True, env={**os.environ, **(env or {})})


def main() -> None:
    PUBLIC.mkdir(parents=True, exist_ok=True)

    # 1. Native (Swift) engine -> report.html only. (The living spec is Rust-
    #    rendered now; dropping PICKLE_SPEC_PATH leaves PickleKit emitting just
    #    the report.)
    run(
        ["swift", "test", "--filter", "GherkinTests"],
        cwd=ROOT / "swift" / "Engine",
        env={"PICKLE_REPORT": "1", "PICKLE_REPORT_PATH": str(PUBLIC / "report.html")},
    )

    # 2. Living spec (engine-neutral) — the Rust generator parses spec/anzan and
    #    renders spec.html matching the report's design.
    run(
        ["cargo", "test", "-p", "soroban-engine", "--test", "living_spec"],
        cwd=ROOT / "rust",
        env={"SOROBAN_SPEC": str(PUBLIC / "spec.html")},
    )

    # 3. Cross-platform (Rust) engine -> rust-report.html, from the same features.
    cucumber_json = PUBLIC / "rust-cucumber.json"
    run(
        ["cargo", "test", "-p", "soroban-engine", "--test", "gherkin"],
        cwd=ROOT / "rust",
        env={"SOROBAN_REPORT": str(cucumber_json)},
    )
    run(
        [
            "node",
            str(ROOT / "scripts" / "rust-report.mjs"),
            str(cucumber_json),
            str(PUBLIC / "rust-report.html"),
        ],
        cwd=ROOT,
    )
    cucumber_json.unlink(missing_ok=True)

    print(f"Wrote {PUBLIC}/{{spec,report,rust-report}}.html")


if __name__ == "__main__":
    main()
