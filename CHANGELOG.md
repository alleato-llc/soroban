# Changelog — cross-cutting

Soroban is a two-ecosystem monorepo, and each ecosystem keeps its own changelog
under its own release track:

- **[swift/CHANGELOG.md](swift/CHANGELOG.md)** — the macOS app, engine, and CLI.
  Released as `vX.Y.Z` (signed, notarized `Soroban.dmg`) by salpa when `swift/**`
  or `spec/**` changes.
- **[rust/CHANGELOG.md](rust/CHANGELOG.md)** — the `anzan`/`soroban-engine`/`cli`
  crates and the `rust/gui` iced app. Released as `rust-vX.Y.Z` (portable
  Linux/Windows/macOS binaries) when `rust/**` or `spec/**` changes.

**This file** records only changes that span **both** ecosystems or the repo as
a whole — shared `spec/**` behavior, the monorepo layout, cross-ecosystem
interchange, and CI/release infrastructure common to both. A change that touches
only one ecosystem belongs in that ecosystem's changelog, not here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
A `spec/**` change is shared language behavior and releases both tracks; note it
here **and** in each ecosystem's changelog as it lands. `[skip ci]` on a commit
still suppresses all release workflows regardless of the paths it touches — see
[docs/RELEASING.md](docs/RELEASING.md).

## [Unreleased]

### Added

- **Scientific mode — the standard calculator trio (normal / scientific /
  programmer) — and the `°` degree literal** (`spec/anzan/modes.feature`,
  `spec/anzan/mathematics.feature`; Swift, Rust, and the ts/wasm/site
  surfaces all land together — Swift 579 / Rust 579 / ts 279 scenarios).
  Scientific keeps Normal's grammar and changes only how a plain NUMERIC
  result echoes: scientific notation at the value's own significant digits
  (`123456 * 2` → `2.46912e5`, never rounded), with an ENG style
  (`:mode scientific eng` → `246.912e3`, exponent snapped to a multiple
  of 3). Value-carried display (Money, grouping) wins over the sci echo.
  The new postfix `°` converts degrees to radians in every mode
  (`x × π/180`, 50-digit π): `sin(90°)` is `1` and `90° == pi / 2` holds.

### Changed

- **CI consolidation: one always-running `ci.yml` with always-reporting
  checks.** The four CI workflows (Swift `ci.yml`, `rust-ci.yml`, `ts-ci.yml`,
  `site-ci.yml`) merged into a single workflow whose jobs all start on every
  PR and push to `main`; a `dorny/paths-filter` `changes` job (the dorado
  pattern, with a shared `spec` YAML anchor merged into every engine filter)
  decides real-work-vs-skip per job, and a skipped job satisfies branch
  protection — so the six required checks always resolve and merges no
  longer need an admin bypass when a PR misses a workflow's paths. The six
  required check names are unchanged byte-for-byte; each job's steps,
  caching, and matrix carried over faithfully. Release workflows
  (`release.yml`, `release-rust.yml`, `deploy-site.yml`) are untouched.

- **Finance mode is retired; its literals are now core grammar.** Currency
  (`$10`, `€10` — sugar for `Money(v, "CODE")`) and thousands grouping
  (`138,561`) lex in EVERY mode: the `$`-before-a-letter cell pin and the
  `,`-is-the-argument-separator-first rule already made them collision-free
  (`$A:1` and `max(138,561)` are unchanged). `:mode finance` errors through
  the ordinary unknown-mode path with a hint that currency now works
  everywhere. The finance-only pin scenarios are deleted from the spec; the
  currency/grouping scenarios now run in the default dialect.

- **The site gains Playwright UI automation for the live REPL island**
  (`site/tests/repl.spec.ts`, new `site-ci.yml` on `site/**` changes).
  Chromium drives the hero's "Live · try it" build against the real static
  build (`npm run test:ui` — build + preview via the config's `webServer`):
  hydration + an exactness smoke (which catches a stale vendored wasm), the
  welcome lines, the mode-badge cycle and its snap-to-normal on examples
  (the bitXor regression), the Examples ▾ menu, the ENV/? panels, Open…/
  Save As… round-trips (including script halting), and the carousel's
  slide-less live build. Island **wiring only** — language behavior stays
  with the three spec runners.

- **Anzan in the browser and on npm: the TS/WASM target** (`ts/`,
  `rust/wasm`, the site's live REPL — modeled on the sibling dorado repo's
  pattern). The verified Rust engine compiles to WebAssembly via a thin,
  workspace-excluded binding crate (`anzan-wasm`) — *a binding, never a third
  implementation* — wrapped by the npm-ready `@alleato/anzan` TypeScript
  package (typed `Calculator` API, a fourth `anzan` CLI, vendored wasm so
  installs need no Rust; unpublished until the name/org call). The shared
  spec gains a THIRD runner — cucumber-js over `spec/anzan` — guarding the
  JS binding layer, and the landing page gains a **live REPL** ("Try it,
  right here"): the real engine, mode chips, click-to-run examples, session
  state (`ans * 2` after the finance example answers `$21,500.00`).
  `ts/wasm/` and `site/src/wasm/` are vendored builds — regenerate with
  `cd ts && npm run build:wasm` after `rust/anzan` changes. New `ts-ci.yml`
  builds fresh wasm and runs typecheck/vitest/spec on `ts/**`,
  `rust/anzan/**`, `rust/wasm/**`, and `spec/**` changes.

- **Anzan scripts: `.anzan` files, statement-aware pipes, and a REPL
  continuation prompt** (`spec/anzan/scripting.feature`, both engines). A
  statement now ends at a newline *unless* a `( [ {` is still open — following
  lines join into ONE logical line, so pretty-formatted `namespace { … }`
  blocks pipe, run from files, and paste into the REPL (`… ` continuation
  prompt). `soroban change.anzan` runs a script file — halting at the first
  error with `at file:line` and exit 1 — and mixes with expression arguments
  in one session (`soroban lib.anzan "changeFor(0.95)"`). A
  `#!/usr/bin/env soroban` shebang line is an ordinary `#` comment, so
  `chmod +x` makes scripts directly executable. The splitter is a public
  engine primitive (`StatementAccumulator`, Swift + Rust) — the SDK piece
  embedders and (later) the apps' multi-line paste share. The first line's
  trailing comment survives the join, so multi-line definitions stay
  documented; an unclosed block at end of input is a loud "unterminated"
  error. See [docs/ANZAN.md](docs/ANZAN.md) §1 and each ecosystem's CLI doc.

### Changed

- **The build/CI helper scripts are now Python.** The three `scripts/generate-*.sh`
  shell scripts — the perf report, the living-spec + engine reports, and the Rust
  app screenshots — are rewritten as `scripts/generate_*.py` (joining the existing
  `generate_icon.py`). Behaviour is identical; `deploy-site.yml` / `screenshots.yml`
  invoke them via `python3`.

### Added

- **The Living Specification is now engine-neutral (Rust-rendered).** `spec.html`
  — the behavior prose at `/spec.html` — was generated only by Swift/PickleKit; it
  is now rendered by a Rust generator (`rust/engine/tests/living_spec.rs`) that
  parses the shared `spec/anzan/*.feature` files directly (via the `gherkin`
  parser `cucumber` re-exports) and reproduces the same page structure + design
  tokens (verified: identical 14 features · 522 behaviors · 1404 steps). It now
  cross-links **both** engine reports — `report.html` (native Swift) and
  `rust-report.html` (Rust) — as equal proofs. `scripts/generate_living_spec.py`
  is now the single command that regenerates all three pages; the front door no
  longer belongs to one ecosystem.
- **The landing page now shows the cross-platform (Rust) app, not just the
  native one.** The hero carousel gained a native/cross **build toggle** beside
  the existing dark/light theme split, with a matched set of real Rust-app
  screenshots (log · log+inspector · grid · grid+inspector, both themes). The
  Rust shots are GENERATED by `scripts/generate_screenshots.py` (driving the
  permanent `rust/gui/src/shot.rs` harness) and regenerated in CI by a new
  `screenshots.yml` workflow — headless Linux with Xvfb + software Vulkan
  (lavapipe, GL/llvmpipe fallback) — which bot-commits the PNGs so the `site/**`
  change auto-deploys.
- **The cross-platform (Rust) engine now has its own live test report.** The
  landing page already published a "Living Specification" + test report from the
  native Swift engine (PickleKit); the Rust engine now gets a matching report
  from the **same** `spec/anzan` features. The gherkin runner emits Cucumber
  JSON when `SOROBAN_REPORT` is set (env-gated, so plain `cargo test` is
  unchanged), and `scripts/rust-report.mjs` converts it to a themed
  `rust-report.html` reusing the exact Solarized/Dracula tokens as `spec.html`.
  `deploy-site.yml` regenerates it on every deploy; the landing page's
  "continuously-verified" line now links **both** engines' reports. Verification
  parity: one shared spec, proven by two independent engines.
- **Stable, version-free download names for the Rust track.** `release-rust.yml`
  now attaches each `rust-v*` release under fixed public names via a
  `gh release upload --clobber` step: `Soroban-cross.dmg` (the signed, notarized
  universal macOS DMG) and `soroban-<os>-<arch>[.exe]` (the portable
  Linux/Windows binaries, `-gui` infix dropped). These names never change across
  releases, so the landing page can link a fixed URL per platform — resolving
  each track's newest tag (`v*` / `rust-v*`) via the GitHub Releases API at build
  time, since GitHub's one repo-wide "latest" can't be trusted per track. This
  is the release-side half of the platform-aware download experience.
- **Cross-ecosystem `.soroban` interchange is now proven both ways.** A new
  Rust-authored fixture `examples/interchange.soroban` (regenerate with `cargo
  run -p soroban-engine --example author_interchange`) is opened and computed by
  *both* ecosystems' suites (`rust/engine/tests/interchange.rs` + Swift's
  `InterchangeTests`), mirroring how the Swift-authored `examples/mortgage.
  soroban` is read by both — so a workbook written by either side is a permanent
  regression guard on the other. The fixture exercises what mortgage doesn't: a
  log variable, a user function, a `data`-type record, a named cell, and a saved
  bit-format variable — all restore and compute identically across Swift ⇄ Rust.

### Changed

- **Documentation reorganized ecosystem-first.** The shared language/format spec
  and a new [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (the common design) stay
  top-level in `docs/`; each ecosystem gained its own authored docs
  ([swift/README.md](swift/README.md) + `swift/docs/`,
  [rust/README.md](rust/README.md) + `rust/docs/`) and a nested `CLAUDE.md`, and
  the root `README.md`/`CLAUDE.md` were slimmed to a monorepo overview + router
  (the Swift app tour and the deep per-subsystem architecture moved into the
  Swift ecosystem docs). New [CONTRIBUTING.md](CONTRIBUTING.md) and
  [spec/README.md](spec/README.md). No behavior or code change;
  `docs/ANZAN.md` (the site-rendered spec) is unchanged.
- **Two independent release tracks, split by ecosystem.** `swift/**` (or
  `spec/**`) changes cut a **macOS release** (`release.yml` → salpa → signed,
  notarized `Soroban.dmg`, tagged `vX.Y.Z`). `rust/**` (or `spec/**`) changes cut
  a **Rust release** (`release-rust.yml` → portable Linux/Windows/macOS binaries,
  tagged `rust-vX.Y.Z` on its own version sequence). Each track is path-gated on
  push to `main` and also runs on manual `workflow_dispatch`; a `spec/**` change
  releases both. Previously *every* push to `main` cut a single macOS release
  (with the Rust binaries attached to it); the Rust binaries now have their own
  versioned track. Because GitHub exposes only one repo-wide "latest" release,
  `releases/latest/download/...` is not a reliable per-track link — stable
  per-track download URLs come from fixed asset names + build-time tag resolution
  on the landing page (see the "stable download names" entry above).
- **CI actions bumped to Node 24.** `actions/upload-artifact@v4` (Node 20, which
  GitHub was force-migrating) → `@v7` (Node 24) across the CI/release workflows,
  clearing the deprecation warning.
- **Per-ecosystem changelogs.** Split the single `CHANGELOG.md` into this
  cross-cutting file plus `swift/CHANGELOG.md` (the dated `v*` history) and
  `rust/CHANGELOG.md` (the Rust port). Ecosystem-specific bugfixes and feature
  parity are recorded in their own file; shared changes stay here.
- Restructured into an ecosystem-first monorepo (Phase 0 of
  [docs/MIGRATION.md](docs/MIGRATION.md)): everything Apple moved under
  `swift/` (`Engine/`, `App/`, `Kit/`, `project.yml`, the app's `salpa.yaml`);
  the Gherkin feature files moved to a shared top-level `spec/`
  (`spec/anzan/`, `spec/session/`), symlinked into the test targets, to serve
  as the cross-ecosystem parity oracle for the Rust port. The repo-root
  `salpa.yaml` now holds only the site deploy.

## [v1.4.13] · [rust-v0.1.9] — 2026-07-22

A `spec/**` change — shared language behavior, released on both tracks
(macOS `v1.4.13`, Rust `rust-v0.1.9`).

### Added

- **Finance mode gains a first-class currency type and thousands grouping**
  (`spec/anzan/modes.feature`, both engines). Finance is no longer grammatically
  identical to Normal. **Currency** is a genuine tagged type — a peer of
  `Int32(…)`/`Decimal(…)` — with a curated set (USD/EUR/GBP/JPY/CNY/INR/KRW/RUB/
  CHF/BTC) and a mode-agnostic constructor `Money(value, "USD")` whose call is
  the canonical form; `$10` / `€10` literals are sugar for it, and an unsupported
  currency glyph is a lex error. The currency propagates through arithmetic —
  `$10 * 5%` is `$0.50`, `$10,000 + ($15,000 * 5%)` is `$10,750.00` — and
  survives all four operators (`$10 * $2` is `$20.00`; a display contract, not a
  unit system). Mixing two currencies errors; `%` on a currency errors. Money
  renders grouped at 2 decimals, symbol outside the sign (`-$1,234.50`);
  `Money(v,"CODE")` is what recalls. **Thousands grouping** (`138,561`) is a
  *separate*, presentation-only value with no arithmetic rules: it canonicalizes
  to the plain number but echoes through a calculation (`138,561 * 9%` →
  `12,470.49`). Both literal forms are **Finance-only**, so nothing existing
  changes meaning: `$` before a letter is still the cell-column pin (`$A:1`), and
  `,` is still the argument separator — grouping is suppressed inside a call's
  argument list and inside `[…]`/`{…}`, so `max(138,561)` is unchanged. The
  constructor works in any mode. See [docs/MODES.md](docs/MODES.md).

[v1.4.13]: https://github.com/alleato-llc/soroban/releases/tag/v1.4.13
[rust-v0.1.9]: https://github.com/alleato-llc/soroban/releases/tag/rust-v0.1.9
