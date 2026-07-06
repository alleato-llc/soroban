#!/usr/bin/env python3
"""Soroban cross-engine benchmark orchestrator.

Builds both calculator engines' runners (the Rust `anzan` crate and the Swift
`SorobanEngine` library), runs each under identical parameters, and writes
results.json + RESULTS.md. The generic engine is in harness.py (copied from Gota,
github.com/alleato-llc/gota — copy-not-depend); this file is the Soroban-specific
config: which runners, the workloads, the framing.

The op measured is one `Calculator.evaluate(line)` — the fair, symmetric operation
BOTH engines expose (Swift's parser is package-scoped, so full parse+eval is the only
externally reachable seam, and the honest one to compare). The metric is
OP_THROUGHPUT: evaluations per second, peak of batches. A single evaluate() is faster
than the clock, so the harness batches it, which auto-calibrates across the ~1000×
spread between `arith` and `fib` (the classic Gota trick) — no shared buffer to fill.

    python3 run.py
    GOTA_MEASURE=3.0 python3 run.py          # override params

HONESTY: this is a SAME-BOX, SAME-RUN comparison of the two engines — that relative
ratio is the deliverable and is valid on any machine. Absolute evals/sec are hardware-
and toolchain-specific (and noisy on shared CI runners), so treat them as a TREND, not
a headline number. See PROTOCOL.md's "Honesty" section.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import harness
from harness import RunnerSpec

HERE = Path(__file__).resolve().parent
os.chdir(HERE)

# buf_bytes is NOMINAL for an op_throughput run (the op is one evaluate(), not a buffer
# sweep). Fixing it at 1e6 makes the runner's raw `mbps` read numerically as
# evaluations/sec, so RESULTS.md and the report's ops/sec headline agree.
BUF = int(os.environ.get("GOTA_BUF", 1_000_000))
WARMUP = float(os.environ.get("GOTA_WARMUP", 0.5))
MEASURE = float(os.environ.get("GOTA_MEASURE", 2.0))


def _build(cmd: list[str], cwd: str) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


def prep_rust_engine():
    if not harness.which("cargo"):
        return None
    _build(["cargo", "build", "--release"], cwd="rust")
    return ["rust/target/release/runner"]


def prep_swift_engine():
    if not harness.which("swift"):
        return None
    _build(["swift", "build", "-c", "release"], cwd="swift")
    return ["swift/.build/release/runner"]


SPECS = [
    RunnerSpec("rust-engine", prep_rust_engine),
    RunnerSpec("swift-engine", prep_swift_engine),
]

# Version probes recorded into the results' provenance — a number is only reproducible
# alongside the compiler that produced it.
TOOLCHAINS = {
    "rust": ["rustc", "--version"],
    "swift": ["swift", "--version"],
}

IMPL_ORDER = ["rust-engine", "swift-engine"]
IMPL_LABELS = {"rust-engine": "Rust engine", "swift-engine": "Swift engine"}

BENCH_ORDER = ["arith", "fib", "reduce", "transcendental", "finance"]
BENCH_LABELS = {
    "arith": "Arithmetic",
    "fib": "Fibonacci",
    "reduce": "Reduction (∑)",
    "transcendental": "Transcendental",
    "finance": "Finance (pmt)",
}


def intro(meta: dict) -> str:
    return f"""\
Soroban's two calculator engines — the Rust `anzan` crate and the Swift `SorobanEngine`
library — evaluating the SAME five Anzan workloads under one protocol. The op is one
`Calculator.evaluate(line)`; the number is peak **evaluations/sec** (op throughput), so
higher is better. results.json also records each run's median rate (mbps_median) as a
stability signal.

Both engines implement the same language against one shared spec, so this is an honest,
apples-to-apples comparison of the two implementations on identical inputs. It is a
**same-box, same-run** measurement: the Rust-vs-Swift RATIO is the deliverable and holds
on any machine, but the absolute evals/sec are hardware/toolchain-specific (and noisy on
shared CI) — read them as a trend, not a fixed number.

Workloads: `arith` exercises BigDecimal division at working precision; `fib` the
interpreter's dispatch + user-function recursion (`fib(20)`); `reduce` the indexed ∑
loop (`sigma_i=1^1000(i^2)`); `transcendental` the f64 libm/Double seam
(`sin+cos+tan+atan2`); `finance` the exact `pmt` (power + div).

Machine: {meta['machine']} | {meta['os']} | {meta['date']} | commit {meta['git_commit']}.
"""


def main() -> None:
    harness.log(f"params: buf={BUF} warmup={WARMUP} measure={MEASURE}")
    rows = harness.run_all(SPECS, BUF, WARMUP, MEASURE)
    meta = harness.gather_metadata(toolchains=TOOLCHAINS)
    params = {"buffer_bytes": BUF, "warmup_s": WARMUP, "measure_s": MEASURE}
    harness.write_results(
        rows,
        "results.json",
        "RESULTS.md",
        params=params,
        meta=meta,
        metric=harness.Metric.OP_THROUGHPUT,
        units="evaluations/sec (peak of batches)",
        impl_order=IMPL_ORDER,
        impl_labels=IMPL_LABELS,
        bench_order=BENCH_ORDER,
        bench_labels=BENCH_LABELS,
        intro=intro(meta),
    )


if __name__ == "__main__":
    main()
