#!/usr/bin/env python3
"""Gota HTML report. Fills in report_template.html to produce a finished,
self-contained, format-aware viewer for the results.json that the harness writes.

The presentation lives in `report_template.html` (HTML/CSS/JS); this script only
substitutes three tokens into it (the title, the embedded data, the date) and writes
the result. To restyle the report, edit the template, not this file.

The output is one standalone .html file (no network, no build step) that:
  - has a file picker to load any results.json produced by Gota, and
  - if you pass a results.json on the command line, embeds it so the page renders
    immediately on open (you can still load a different file from the picker).

It is generic over the format, not over your project: the page shows only what the
results.json carries (the impl/bench/mbps rows plus the machine/date/params
provenance). Pass a title if you want one; otherwise it is generic.

    python3 report.py                       # empty viewer, load a file in-browser
    python3 report.py results.json          # embed data, write report.html
    python3 report.py results.json -o out.html --title "My project throughput"

Pass more than one file to compare runs against a baseline (the first file, or
whichever you name with --baseline). The HTML then opens in diff mode; --markdown
prints the delta table instead; --fail-on-regression turns it into a CI gate:

    python3 report.py base.json new.json                       # diff report.html
    python3 report.py base.json a.json b.json --baseline base.json
    python3 report.py base.json new.json --markdown            # delta table to stdout
    python3 report.py base.json new.json --fail-on-regression 5  # exit 1 if >5% slower
"""

from __future__ import annotations

import argparse
import datetime
import json
import sys
from pathlib import Path

import harness

TEMPLATE_PATH = Path(__file__).resolve().parent / "report_template.html"


def build_html(doc: dict | None, title: str, template: str | None = None) -> str:
    """Fill the template's three tokens. `template` defaults to report_template.html
    next to this script; pass your own string to use a different template."""
    if template is None:
        template = TEMPLATE_PATH.read_text()
    return (
        template.replace("__DATA__", json.dumps(doc) if doc is not None else "null")
        .replace("__TITLE__", title)
        .replace("__GENERATED__", datetime.date.today().isoformat())
    )


def _load(path: str) -> dict:
    """Load a results.json and tag it with a label (its filename stem) so comparisons
    and the viewer have a stable, human name for each run."""
    doc = json.loads(Path(path).read_text())
    doc.setdefault("label", Path(path).stem)
    return doc


def main() -> None:
    ap = argparse.ArgumentParser(description="Generate a standalone HTML viewer for a Gota results.json, or compare runs.")
    ap.add_argument("results", nargs="*", help="results.json file(s); pass two or more to compare against a baseline")
    ap.add_argument("-o", "--out", default="report.html", help="output HTML file (default: report.html)")
    ap.add_argument("--title", default="Gota benchmark report", help="report title")
    ap.add_argument("--template", help="custom HTML template (default: report_template.html beside this script)")
    ap.add_argument("--baseline", help="which results file is the baseline (default: the first one given)")
    ap.add_argument("--tolerance", type=float, default=2.0, help="noise band in percent; changes within it are not flagged (default: 2.0)")
    ap.add_argument("--markdown", action="store_true", help="print the comparison as a Markdown delta table to stdout instead of writing HTML")
    ap.add_argument("--fail-on-regression", type=float, metavar="PCT", help="exit non-zero if any cell is more than PCT%% slower than baseline (CI gate)")
    args = ap.parse_args()

    # 0 or 1 file: the original single-run viewer (empty if no file).
    if len(args.results) <= 1:
        if args.baseline or args.markdown or args.fail_on_regression is not None:
            ap.error("comparison needs at least two results files (a baseline and a candidate)")
        doc = _load(args.results[0]) if args.results else None
        template = Path(args.template).read_text() if args.template else None
        Path(args.out).write_text(build_html(doc, args.title, template))
        print(f"wrote {args.out} from {args.results[0] if args.results else '(empty viewer)'}", file=sys.stderr)
        return

    # 2+ files: compare candidates against a baseline.
    docs = [_load(p) for p in args.results]
    by_label = {d["label"]: d for d in docs}
    baseline = by_label.get(Path(args.baseline).stem) if args.baseline else docs[0]
    if baseline is None:
        ap.error(f"--baseline {args.baseline} is not among the given files")
    candidates = [d for d in docs if d is not baseline]

    comparison = harness.compare_runs(baseline, candidates, tolerance=args.tolerance / 100)

    if args.markdown:
        print(harness.render_comparison_markdown(comparison))
    else:
        # Embed all runs and the baseline label; the viewer computes deltas (and lets
        # the reader re-pick the baseline interactively).
        wrapper = {"runs": docs, "baseline": baseline["label"], "tolerance": args.tolerance / 100}
        template = Path(args.template).read_text() if args.template else None
        Path(args.out).write_text(build_html(wrapper, args.title, template))
        print(f"wrote {args.out}: {baseline['label']} (baseline) vs {', '.join(c['label'] for c in candidates)}", file=sys.stderr)

    if args.fail_on_regression is not None:
        regs = harness.regressions(comparison, threshold_pct=args.fail_on_regression)
        if regs:
            print(f"REGRESSION: {len(regs)} cell(s) >{args.fail_on_regression:g}% slower than baseline:", file=sys.stderr)
            for r in regs:
                print(f"  {r['label']}: {r['impl']}/{r['bench']} {r['pct']:+.1f}%", file=sys.stderr)
            sys.exit(1)


if __name__ == "__main__":
    main()
