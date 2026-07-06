"""Gota: a generic cross-language micro-benchmark orchestrator.

This file is project-agnostic. It runs a list of `RunnerSpec`s (each builds itself
and emits JSON lines of {"impl","bench","mbps","iters"}), collects the rows, and
writes a results.json plus a Markdown table. The project-specific configuration
(which runners, the row/column labels, the framing text) lives in a `run.py` that
imports this and supplies those, so the engine here never changes.

Copy this file into your project as-is; write a `run.py` that declares your runners
and calls `run_all` + `write_results`. See README.md and PROTOCOL.md.
"""

from __future__ import annotations

import dataclasses
import enum
import json
import platform
import shutil
import subprocess
import sys
from typing import Callable, Optional


class Metric(str, enum.Enum):
    """The kind of quantity a run measures. It drives the report headline and whether two
    runs are comparable (you cannot compare a TPS run to an MB/s run). The display unit
    (e.g. "MB/s", "requests/sec") is a separate free-text `units` label, and many labels
    map to one Metric: "rows/sec" and "requests/sec" are both OP_THROUGHPUT.

    Selecting a Metric is the user's intent; which measurement *engine* runs (batched for
    ops faster than the clock, per-call for slower IO) is decided by the harness from the
    op's speed, not chosen here."""

    BYTE_THROUGHPUT = "byte_throughput"  # bytes moved per second; headline MB/s
    OP_THROUGHPUT = "op_throughput"      # operations per second (TPS); headline ops/sec
    LATENCY = "latency"                  # per-call time distribution; headline p50/p99/max


def which(cmd: str) -> bool:
    return shutil.which(cmd) is not None


def log(msg: str) -> None:
    print(msg, file=sys.stderr)


@dataclasses.dataclass
class RunnerSpec:
    """One benchmark runner.

    `prepare` builds/sets up the runner and returns the argv to invoke it (the three
    protocol parameters are appended by the harness), or None if the runner is
    unavailable (a missing toolchain) and should be skipped. It may raise on a build
    failure; the harness logs and skips.
    """

    name: str
    prepare: Callable[[], Optional[list[str]]]


def run_all(
    specs: list[RunnerSpec],
    buf: int,
    warmup: float,
    measure: float,
    *,
    timeout: float = 120.0,
) -> list[dict]:
    """Build and run each available runner with identical parameters; collect the
    JSON-line results. Each runner's stdout is captured separately, so there is no
    shared-output ordering or interleaving.

    `timeout` bounds each runner (seconds): a runner that hangs is killed and skipped
    rather than stalling the whole run. A malformed JSON line is logged and skipped, so
    one bad line never aborts the collection."""
    rows: list[dict] = []
    for spec in specs:
        try:
            argv = spec.prepare()
        except Exception as e:  # build failure, etc.
            log(f"  {spec.name}: prepare failed ({e}), skipping")
            continue
        if argv is None:
            log(f"  {spec.name}: unavailable, skipping")
            continue
        log(f"  {spec.name}: running")
        try:
            proc = subprocess.run(
                [*argv, str(buf), str(warmup), str(measure)],
                capture_output=True,
                text=True,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            log(f"  {spec.name}: timed out after {timeout:g}s, skipping")
            continue
        if proc.returncode != 0:
            log(f"  {spec.name}: runner exited {proc.returncode}; stderr:\n{proc.stderr.strip()}")
            continue
        for line in proc.stdout.splitlines():
            line = line.strip()
            if line.startswith("{"):
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError as e:
                    log(f"  {spec.name}: skipped malformed JSON line ({e}): {line[:120]}")
    return rows


def _first_line(argv: list[str]) -> Optional[str]:
    """Run a `--version`-style probe and return its first non-empty output line, or None
    if the tool is absent or errors. stderr is folded in because some toolchains (javac,
    older gcc) print their version there."""
    try:
        out = subprocess.run(argv, capture_output=True, text=True, timeout=10)
    except Exception:
        return None
    for line in ((out.stdout or "") + (out.stderr or "")).splitlines():
        if line.strip():
            return line.strip()
    return None


def gather_metadata(*, toolchains: Optional[dict[str, list[str]]] = None) -> dict:
    """Machine, OS, date, and git commit, for provenance in the results.

    Pass `toolchains` as {label: version_argv} (e.g. {"rust": ["rustc", "--version"]})
    to also record each compiler/runtime's version under a `toolchains` key; absent or
    failing tools are skipped. A throughput number is only reproducible alongside the
    toolchain that produced it, so recording versions is part of honest provenance."""
    machine = "unknown"
    try:
        if platform.system() == "Darwin":
            machine = subprocess.check_output(
                ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
            ).strip()
        else:
            with open("/proc/cpuinfo") as f:
                for line in f:
                    if line.startswith("model name"):
                        machine = line.split(":", 1)[1].strip()
                        break
    except Exception:
        pass
    commit = "unknown"
    try:
        commit = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        pass
    meta = {
        "machine": machine,
        "os": f"{platform.system()} {platform.machine()}",
        "date": subprocess.check_output(["date", "+%Y-%m-%d"], text=True).strip(),
        "git_commit": commit,
    }
    if toolchains:
        versions = {label: v for label, argv in toolchains.items() if (v := _first_line(argv))}
        if versions:
            meta["toolchains"] = versions
    return meta


def build_results_doc(
    rows: list[dict],
    *,
    params: dict,
    meta: dict,
    units: str,
    metric: "Metric | str" = Metric.BYTE_THROUGHPUT,
) -> dict:
    """The results.json document: provenance + params + the raw measurement rows.
    This is the canonical, tooling-friendly shape; feed it to your own pipeline (a
    dashboard, a database, the HTML viewer) instead of the Markdown if you prefer.

    `metric` is the kind of quantity measured (a `Metric`, or its string value); it is
    recorded so the report can headline the right number and a comparison can refuse to
    pit different kinds against each other. An unknown value raises. `units` stays the
    free-text display label (e.g. "MB/s", "requests/sec")."""
    return {**meta, "params": params, "metric": Metric(metric).value, "units": units, "results": rows}


def render_markdown(
    rows: list[dict],
    *,
    intro: str,
    impl_order: list[str],
    impl_labels: dict[str, str],
    bench_order: list[str],
    bench_labels: dict[str, str],
) -> str:
    """Render the results as a Markdown table and return it as a string (so callers can
    route it anywhere: a file, a PR comment, a docs page). Orderings and labels are
    supplied by the caller, so this stays generic."""
    by = {(r["impl"], r["bench"]): r["mbps"] for r in rows}
    present = [i for i in impl_order if any(r["impl"] == i for r in rows)]

    lines = ["# Benchmark results", "", intro.strip(), ""]
    lines.append("| Implementation | " + " | ".join(bench_labels[b] for b in bench_order) + " |")
    lines.append("| --- | " + " | ".join("---:" for _ in bench_order) + " |")
    for impl in present:
        cells = []
        for b in bench_order:
            v = by.get((impl, b))
            cells.append(f"{v:.1f}" if v is not None else "-")
        lines.append(f"| {impl_labels[impl]} | " + " | ".join(cells) + " |")
    lines.append("")
    return "\n".join(lines)


def _write(target, text: str) -> str:
    """Write `text` to a path (str/Path) or an already-open writable stream (anything
    with a `.write`, e.g. sys.stdout, an io.StringIO, an HTTP response body). Returns a
    display name for logging."""
    if hasattr(target, "write"):
        target.write(text)
        return getattr(target, "name", "<stream>")
    with open(target, "w") as f:
        f.write(text)
    return str(target)


def write_results(
    rows: list[dict],
    out_json,
    out_md,
    *,
    params: dict,
    meta: dict,
    units: str,
    metric: "Metric | str" = Metric.BYTE_THROUGHPUT,
    impl_order: list[str],
    impl_labels: dict[str, str],
    bench_order: list[str],
    bench_labels: dict[str, str],
    intro: str,
) -> None:
    """Convenience: write results.json (raw + provenance) and a Markdown table.

    `out_json` and `out_md` may each be a path or an open writable stream, so the
    output can go to files or straight into your own tooling. `metric` (a `Metric`)
    records what kind of quantity was measured; `units` is its free-text display label.
    To integrate more deeply, skip this and use `build_results_doc` / `render_markdown`
    directly, or just consume the list of row dicts that `run_all` returns."""
    json_name = _write(out_json, json.dumps(build_results_doc(rows, params=params, meta=meta, units=units, metric=metric), indent=2) + "\n")
    md_name = _write(
        out_md,
        render_markdown(
            rows,
            intro=intro,
            impl_order=impl_order,
            impl_labels=impl_labels,
            bench_order=bench_order,
            bench_labels=bench_labels,
        ),
    )
    present = len([i for i in impl_order if any(r["impl"] == i for r in rows)])
    log(f"wrote {json_name} and {md_name} ({len(rows)} measurements, {present} implementations)")


# --- Comparison: baseline vs one or more candidate runs ---------------------------
#
# A results.json is a single run. To track a baseline against later runs (regression
# gating, before/after) you compare runs *on top of* that format without changing it:
# match cells by (impl, bench) and report the ratio. Two honesty rules are built in
# because they are easy to get wrong: a change within a noise band is not a regression
# (peak-of-batches still has variance), and comparing across different machines or
# params is apples-to-oranges, so provenance mismatches are surfaced as warnings rather
# than silently turned into a meaningless delta.


def _index(doc: dict) -> dict:
    """Map (impl, bench) -> mbps for a results doc."""
    return {(r["impl"], r["bench"]): r["mbps"] for r in doc.get("results", [])}


def _union_order(docs: list[dict], key: str) -> list[str]:
    """Distinct `key` values across docs, baseline first, preserving first-seen order."""
    seen, order = set(), []
    for doc in docs:
        for r in doc.get("results", []):
            if r[key] not in seen:
                seen.add(r[key])
                order.append(r[key])
    return order


def _provenance_warnings(baseline: dict, cand: dict) -> list[str]:
    """Flag baseline/candidate differences that make a numeric delta misleading."""
    warnings = []
    for field, what in (("machine", "machine"), ("os", "OS")):
        if baseline.get(field) and cand.get(field) and baseline[field] != cand[field]:
            warnings.append(f"{what} differs ({baseline[field]} vs {cand[field]}) — deltas are not comparable")
    bp, cp = baseline.get("params", {}), cand.get("params", {})
    for k in sorted(set(bp) | set(cp)):
        if bp.get(k) != cp.get(k):
            warnings.append(f"param {k} differs ({bp.get(k)} vs {cp.get(k)}) — runs measure different work")
    # Comparability keys on the metric *kind*, not the free-text label: two OP_THROUGHPUT
    # runs labelled "requests/sec" and "rows/sec" are comparable, but a TPS run and an
    # MB/s run are not. Pre-metric results (no field) are assumed BYTE_THROUGHPUT.
    bm = baseline.get("metric", Metric.BYTE_THROUGHPUT.value)
    cm = cand.get("metric", Metric.BYTE_THROUGHPUT.value)
    if bm != cm:
        warnings.append(f"metric kind differs ({bm} vs {cm}) — runs measure different things, not comparable")
    return warnings


def compare_runs(baseline: dict, candidates: list[dict], *, tolerance: float = 0.02) -> dict:
    """Compare each candidate results doc against `baseline`, cell by (impl, bench).

    `tolerance` is the fractional noise band: a change within ±tolerance is reported as
    `same`, not faster/slower, so ordinary run-to-run jitter is not called a regression.
    Returns a dict with the union `impls`/`benches` (baseline first) and, per candidate,
    a flat list of cell deltas plus any provenance `warnings`. Each cell carries
    `base`, `cand`, `ratio` (cand/base), `pct` (percent change), and `status` (one of
    faster/slower/same/new/gone). Callers apply their own gate threshold to `pct`."""
    base_idx = _index(baseline)
    impls = _union_order([baseline, *candidates], "impl")
    benches = _union_order([baseline, *candidates], "bench")
    runs = []
    for cand in candidates:
        cand_idx = _index(cand)
        cells = []
        for impl in impls:
            for bench in benches:
                b, c = base_idx.get((impl, bench)), cand_idx.get((impl, bench))
                if b is None and c is None:
                    continue
                cell = {"impl": impl, "bench": bench, "base": b, "cand": c}
                if b is None:
                    cell.update(ratio=None, pct=None, status="new")
                elif c is None:
                    cell.update(ratio=None, pct=None, status="gone")
                else:
                    ratio = c / b if b else None
                    cell["ratio"] = ratio
                    cell["pct"] = (ratio - 1) * 100 if ratio is not None else None
                    cell["status"] = "faster" if ratio >= 1 + tolerance else "slower" if ratio <= 1 - tolerance else "same"
                cells.append(cell)
        runs.append({
            "label": cand.get("label") or cand.get("git_commit") or cand.get("date") or "candidate",
            "machine": cand.get("machine"),
            "git_commit": cand.get("git_commit"),
            "date": cand.get("date"),
            "warnings": _provenance_warnings(baseline, cand),
            "cells": cells,
        })
    return {
        "baseline": {
            "label": baseline.get("label") or baseline.get("git_commit") or baseline.get("date") or "baseline",
            "machine": baseline.get("machine"),
            "git_commit": baseline.get("git_commit"),
            "date": baseline.get("date"),
        },
        "tolerance": tolerance,
        "impls": impls,
        "benches": benches,
        "runs": runs,
    }


def regressions(comparison: dict, *, threshold_pct: float) -> list[dict]:
    """Cells that dropped more than `threshold_pct` percent below baseline, across all
    candidate runs (each annotated with its run `label`). Empty list means no run
    regressed past the threshold — use it for a CI gate's exit code."""
    out = []
    for run in comparison["runs"]:
        for cell in run["cells"]:
            if cell["pct"] is not None and cell["pct"] < -abs(threshold_pct):
                out.append({**cell, "label": run["label"]})
    return out


def render_comparison_markdown(comparison: dict) -> str:
    """Render a comparison (from `compare_runs`) as Markdown: one pivot table per
    candidate, each cell `value (±pct%)`, with a ⚠ on changes past the tolerance band
    and any provenance warnings called out above the table. Returns a string for a PR
    comment or a docs page."""
    benches = comparison["benches"]
    tol = comparison["tolerance"] * 100
    base = comparison["baseline"]
    base_bits = " · ".join(b for b in (base["machine"], base["git_commit"] and f"commit {base['git_commit']}", base["date"]) if b)
    lines = ["# Benchmark comparison", ""]
    lines.append(f"Baseline: **{base['label']}**" + (f" ({base_bits})" if base_bits else ""))
    lines.append(f"Tolerance: ±{tol:.1f}% (changes within the band are not flagged)")
    lines.append("")
    mark = {"faster": "▲", "slower": "▼ ⚠", "same": "", "new": "new", "gone": "gone"}
    for run in comparison["runs"]:
        lines.append(f"## {run['label']} vs baseline")
        for w in run["warnings"]:
            lines.append(f"> ⚠️ {w}")
        if run["warnings"]:
            lines.append("")
        cells = {(c["impl"], c["bench"]): c for c in run["cells"]}
        impls = [i for i in comparison["impls"] if any((i, b) in cells for b in benches)]
        lines.append("| Implementation | " + " | ".join(benches) + " |")
        lines.append("| --- | " + " | ".join("---:" for _ in benches) + " |")
        for impl in impls:
            out = []
            for bench in benches:
                c = cells.get((impl, bench))
                if c is None:
                    out.append("-")
                elif c["status"] == "new":
                    out.append(f"{c['cand']:.1f} (new)")
                elif c["status"] == "gone":
                    out.append("gone")
                else:
                    out.append(f"{c['cand']:.1f} ({c['pct']:+.1f}%) {mark[c['status']]}".strip())
            lines.append(f"| {impl} | " + " | ".join(out) + " |")
        lines.append("")
    return "\n".join(lines)
