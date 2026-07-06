#!/usr/bin/env node
// Convert the Rust gherkin runner's Cucumber JSON (emitted when SOROBAN_REPORT
// is set — see rust/engine/tests/gherkin.rs) into a themed HTML report that
// matches the Swift PickleKit spec.html/report.html: the same Solarized Light /
// Dracula design tokens, the same pre-paint theme script and toggle, so the two
// engines' reports read as one family on the site.
//
//   node scripts/rust-report.mjs <input.json> <output.html>
//   node scripts/rust-report.mjs            # defaults: /tmp/rust-cucumber.json → site/public/rust-report.html
//
// The report is the CROSS-PLATFORM (Rust) engine's proof against the SAME
// shared spec/anzan features the native (Swift) engine runs — verification
// parity. Any lookup/parse failure exits non-zero so the deploy step fails loud.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const inPath = process.argv[2] ?? "/tmp/rust-cucumber.json";
const outPath = process.argv[3] ?? resolve(here, "../site/public/rust-report.html");

const esc = (s) =>
  String(s)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

// A scenario's status is the worst of its steps (failed > undefined > pending >
// skipped > passed). Background steps fold into the owning scenario visually.
const RANK = { failed: 4, undefined: 3, pending: 2, skipped: 1, passed: 0 };
const worst = (statuses) =>
  statuses.reduce((acc, s) => (RANK[s] > RANK[acc] ? s : acc), "passed");

const features = JSON.parse(readFileSync(inPath, "utf8"));

let scenarioCount = 0;
let scenarioPass = 0;
let stepCount = 0;
const stepTally = { passed: 0, failed: 0, skipped: 0, undefined: 0, pending: 0 };

const featureHtml = features
  .map((feature) => {
    const scenarios = (feature.elements ?? []).filter((e) => e.type === "scenario");
    const rows = scenarios
      .map((scenario) => {
        const steps = scenario.steps ?? [];
        const statuses = steps.map((s) => s.result?.status ?? "undefined");
        for (const st of statuses) {
          stepCount += 1;
          stepTally[st] = (stepTally[st] ?? 0) + 1;
        }
        const status = worst(statuses);
        scenarioCount += 1;
        if (status === "passed") scenarioPass += 1;
        const nanos = steps.reduce((sum, s) => sum + (s.result?.duration ?? 0), 0);
        const ms = (nanos / 1e6).toFixed(nanos >= 1e6 ? 0 : 1);
        const glyph = status === "passed" ? "✔" : status === "failed" ? "✗" : "•";
        const stepList = steps
          .map((s) => {
            const st = s.result?.status ?? "undefined";
            return `<li class="step step-${st}"><span class="kw">${esc(
              s.keyword.trim(),
            )}</span> ${esc(s.name)}${
              s.result?.error_message
                ? `<pre class="err">${esc(s.result.error_message)}</pre>`
                : ""
            }</li>`;
          })
          .join("");
        return `<details class="scenario scenario-${status}"${
          status === "passed" ? "" : " open"
        }>
  <summary><span class="pill pill-${status}">${glyph}</span> ${esc(
    scenario.name,
  )} <span class="dur">${ms} ms</span></summary>
  <ul class="steps">${stepList}</ul>
</details>`;
      })
      .join("\n");
    const tags = (feature.tags ?? [])
      .map((t) => `<span class="tag">${esc(t.name ?? t)}</span>`)
      .join(" ");
    const id = esc((feature.name ?? "feature").toLowerCase().replace(/[^a-z0-9]+/g, "-"));
    return `<section class="feature" id="${id}">
  <h2>${esc(feature.name)} ${tags}</h2>
  <div class="feature-count">${scenarios.length} scenario${
    scenarios.length === 1 ? "" : "s"
  }</div>
  ${rows}
</section>`;
  })
  .join("\n");

const allPass = scenarioPass === scenarioCount;
const outline = features
  .map((f) => {
    const id = esc((f.name ?? "feature").toLowerCase().replace(/[^a-z0-9]+/g, "-"));
    const n = (f.elements ?? []).filter((e) => e.type === "scenario").length;
    return `<a href="#${id}">${esc(f.name)} <span class="rail-n">${n}</span></a>`;
  })
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Anzan — Rust Engine Report</title>
<script>
  // Pre-paint theme resolution, shared with the rest of the site
  // (same localStorage key as the landing page's Layout.astro).
  (() => {
    const saved = localStorage.getItem("soroban-theme");
    const dark = matchMedia("(prefers-color-scheme: dark)").matches;
    document.documentElement.dataset.theme = saved ?? (dark ? "dark" : "light");
  })();
</script>
<style>
/* Palette: Solarized Light / Dracula — identical tokens to spec.html so the
   native (Swift) and cross-platform (Rust) reports read as one family. */
:root, :root[data-theme="light"] {
  --bg: #fdf6e3; --surface: #eee8d5; --text: #073642;
  --muted: #657b83; --faint: #93a1a1; --accent: #268bd2;
  --error: #dc322f; --border: rgba(7,54,66,0.12); --shadow: rgba(7,54,66,0.06);
  --passed: #2aa198; --failed: #dc322f; --skipped: #93a1a1; --undefined: #b58900;
}
:root[data-theme="dark"] {
  --bg: #282a36; --surface: #343746; --text: #f8f8f2;
  --muted: #bd93f9; --faint: #6272a4; --accent: #ff79c6;
  --error: #ff5555; --border: rgba(248,248,242,0.1); --shadow: rgba(0,0,0,0.3);
  --passed: #50fa7b; --failed: #ff5555; --skipped: #6272a4; --undefined: #ffb86c;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body { font: 16px/1.6 system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; background: var(--bg); color: var(--text); padding: 20px; -webkit-font-smoothing: antialiased; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
.page-width { max-width: 1040px; margin: 0 auto; }
.page-header { max-width: 1040px; margin: 0 auto 1.25rem; padding-bottom: 1rem; border-bottom: 1px solid var(--border); }
.page-header-row { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.page-header h1 { font-size: 1.7rem; letter-spacing: -0.015em; }
.page-sub { margin-top: 0.5rem; color: var(--faint); font-size: 0.9rem; }
.page-sub .check { color: var(--passed); font-weight: 700; }
.page-sub strong { color: var(--text); }
.cross-link { margin-top: 0.4rem; font-size: 0.9rem; }
.icon-btn { border: 1px solid var(--border); background: var(--bg); color: var(--muted); border-radius: 8px; width: 2.2rem; height: 2.2rem; font-size: 1.05rem; cursor: pointer; line-height: 1; flex: none; }
.icon-btn:hover { color: var(--text); }
.tag { display: inline-block; background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); padding: 2px 8px; border-radius: 999px; font-size: 11px; margin-left: 4px; vertical-align: middle; }
.layout { display: grid; grid-template-columns: 220px 1fr; gap: 2rem; max-width: 1040px; margin: 0 auto; }
.rail { position: sticky; top: 20px; align-self: start; font-size: 0.85rem; max-height: calc(100vh - 40px); overflow-y: auto; display: flex; flex-direction: column; gap: 0.15rem; }
.rail a { color: var(--muted); padding: 0.15rem 0; display: flex; justify-content: space-between; gap: 0.5rem; }
.rail a:hover { color: var(--text); text-decoration: none; }
.rail-n { color: var(--faint); }
.feature { margin-bottom: 2rem; }
.feature h2 { font-size: 1.2rem; scroll-margin-top: 20px; }
.feature-count { color: var(--faint); font-size: 0.8rem; margin: 0.15rem 0 0.6rem; }
.scenario { border: 1px solid var(--border); border-radius: 8px; margin-bottom: 0.4rem; background: var(--surface); overflow: hidden; }
.scenario summary { cursor: pointer; padding: 0.5rem 0.7rem; list-style: none; display: flex; align-items: center; gap: 0.5rem; }
.scenario summary::-webkit-details-marker { display: none; }
.dur { margin-left: auto; color: var(--faint); font-size: 0.78rem; }
.pill { display: inline-flex; align-items: center; justify-content: center; width: 1.3rem; height: 1.3rem; border-radius: 999px; font-size: 0.8rem; color: var(--bg); flex: none; }
.pill-passed { background: var(--passed); }
.pill-failed { background: var(--failed); }
.pill-skipped, .pill-undefined, .pill-pending { background: var(--undefined); }
.steps { list-style: none; padding: 0.2rem 0.7rem 0.6rem 2.1rem; font-size: 0.9rem; }
.step { color: var(--muted); padding: 0.05rem 0; }
.step .kw { color: var(--accent); font-weight: 600; }
.step-failed { color: var(--failed); }
.step-skipped, .step-undefined, .step-pending { color: var(--faint); }
.err { color: var(--failed); background: color-mix(in srgb, var(--failed) 10%, transparent); padding: 0.4rem 0.6rem; border-radius: 6px; margin-top: 0.3rem; white-space: pre-wrap; font-size: 0.82rem; }
@media (max-width: 800px) { .layout { grid-template-columns: 1fr; } .rail { position: static; max-height: none; } }
</style>
</head>
<body>
<header class="page-header">
  <div class="page-header-row">
    <h1>Anzan — Rust Engine Report</h1>
    <button class="icon-btn" id="theme" aria-label="Toggle theme" title="Toggle theme">◐</button>
  </div>
  <p class="page-sub">
    The <strong>cross-platform (Rust) engine</strong> against the shared
    <strong>spec/anzan</strong> features —
    <span class="check">${allPass ? "✔ all passing" : `${scenarioPass}/${scenarioCount} passing`}</span>:
    <strong>${features.length}</strong> features,
    <strong>${scenarioCount}</strong> scenarios,
    <strong>${stepCount}</strong> steps.
  </p>
  <p class="cross-link">
    The same spec verified by the native engine: <a href="/report.html">Swift report</a> ·
    <a href="/spec.html">Living Specification</a>
  </p>
</header>
<div class="layout page-width">
  <nav class="rail" aria-label="Features">
${outline}
  </nav>
  <main>
${featureHtml}
  </main>
</div>
<script>
  document.getElementById("theme").addEventListener("click", () => {
    const next = document.documentElement.dataset.theme === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = next;
    localStorage.setItem("soroban-theme", next);
  });
</script>
</body>
</html>
`;

writeFileSync(outPath, html);
console.log(
  `Wrote ${outPath} — ${features.length} features, ${scenarioCount} scenarios (${scenarioPass} passing), ${stepCount} steps.`,
);
if (stepTally.failed > 0 || stepTally.undefined > 0) {
  console.warn(
    `[rust-report] ${stepTally.failed} failed / ${stepTally.undefined} undefined steps — report still written.`,
  );
}
