# Migration: modular monorepo + Rust/Iced port

**Decision record (2026-07-03).** Soroban becomes a modular monorepo hosting *two
long-term implementations* — the existing Swift/SwiftUI app and a new Rust app
built on [iced](https://iced.rs) via **rime** (`~/Development/rime`, our component
kit). The layout is **ecosystem-first**; the Gherkin feature files move to a shared
top-level `spec/` and become the cross-ecosystem parity oracle. rime stays its own
repo (it serves other apps) and is extended with the generic widgets Soroban needs.

---

## Status (2026-07-04)

Phases **0–3b are done**; both implementations pass the shared `spec/`. The rest
of this document is the original plan (2026-07-03), kept for rationale — where it
reads as future tense, cross-check against this section.

- **0 — monorepo restructure**: done. Swift green under `swift/`, `spec/` shared.
- **1 — `anzan` language crate**: done. All language features green against the
  cucumber oracle.
- **2a — `soroban` CLI**: done.
- **2b — `engine` crate** (sheet + persistence, workbook interchange): done —
  **522/522** shared Gherkin scenarios green in both ecosystems. A follow-on
  Phase 2c ported the engine remainder the shared suite doesn't exercise on its
  own (`ReferenceRewriter`, `NamedCells`, journal/package/`DataStore`/CSV, the
  log-only mutation commands, structural edits, `History` reflection, the binary
  bit-editor model, docs assembly). **`.soroban` interchange is proven both
  ways**: `examples/mortgage.soroban` (Swift-authored) and `examples/
  interchange.soroban` (Rust-authored) are each opened + computed by BOTH
  ecosystems' test suites — a workbook from either side round-trips through the
  other (cells, formulas, user functions, log variables, `data` records, named
  cells, saved bit-formats).
- **3a — rime extensions**: done as needed — `grid` (virtualized; per-cell
  overlays, double-click activation, per-column widths + resize-drag), `bit_grid`,
  `menu_bar`/`menu_bar_with_trailing`, plus the small gaps. **`autocomplete_field`
  is not yet built** (the gui has no autocomplete — see gaps).
- **3b — `gui` slices ①–⑥**: all landed, each demoed. Beyond the slice plan the
  gui also gained inline cell editing, in-cell controls, keyboard navigation
  (arrows / shift-arrows / Enter / type-to-edit / Esc), copy·cut·paste as TSV,
  column-width resize, a File/Edit/View **menu bar** with corner icons
  (replacing an earlier auto-hiding button strip), full **point mode** (Excel
  reference insertion incl. re-click-replace and shift-click-extend-to-range),
  and the **binary bit-editor at parity with the AppKit app** — a width picker,
  the preset catalog, per-field editors (enum pickers, base-aware numeric
  inputs, flag chips, reserved/unused gaps), and a visual **Build / Edit / Save**
  for custom formats that persist in the workbook. The bit-editor and session
  behavior are pinned by a **Rust-only headless cucumber suite** driving the
  UI-free `Session` (`rust/gui/tests/session.feature`, ~90% `session.rs`
  coverage) — the fast counterpart to Swift's `SorobanSessionTests`; it's not a
  cross-ecosystem oracle (that stays `spec/anzan`), just the port's own net.

**Remaining `rust/gui` gaps** (fidelity, not blockers): multiple sheets (the `+`
tab is decorative), row-height resize, structural edits (insert/delete rows &
columns), Fill Down/Right, relative-reference paste adjustment, and autocomplete.

**Next — Phase 4**: fold `rust/gui` into CI (settle the rime path-dep) and
dual-asset releases (§6). The §8 grid-performance risk is **resolved** — the
virtualized grid shipped and performs.

## 1. Target layout

```
soroban/
├── spec/                      # THE shared behavior spec — single source of truth
│   ├── anzan/                 # language scenarios (from Engine/Tests/…/Features)
│   │   ├── anzan.feature      # grammar facts (companion to docs/ANZAN.md)
│   │   ├── calculation.feature, functions.feature, spreadsheet.feature, …
│   └── session/               # session-layer scenarios (from App/Tests/Session)
│       └── session.feature
├── swift/                     # everything Apple, one SwiftPM+xcodegen world
│   ├── Engine/                # ← Engine/ (Anzan, SorobanEngine, SorobanCLI)
│   ├── App/                   # ← App/
│   ├── Kit/                   # ← Kit/ (BinaryEditorKit — Tama shared kit)
│   ├── project.yml            # ← project.yml (paths updated)
│   └── Soroban.xcodeproj      # generated, gitignored (unchanged rule)
├── rust/                      # one cargo workspace
│   ├── Cargo.toml             # members = anzan, cli, engine; gui is EXCLUDED (below)
│   ├── anzan/                 # the language crate (no grid/file knowledge)
│   ├── engine/                # hosting layer: sheet + persistence (pub use anzan)
│   ├── cli/                   # `soroban` binary (depends on anzan only)
│   └── gui/                   # iced + rime app — workspace-EXCLUDED (rime path-dep
│                              # + iced/wgpu); builds standalone, folded in at Phase 4
│                              # (`kit/` was planned but never split out — the bit
│                              #  editor lives in gui on rime's bit_grid)
├── site/                      # Astro landing page (unchanged)
├── docs/                      # shared language/format docs (ANZAN.md, FORMAT.md, …)
├── salpa.yaml                 # per-ecosystem build config (see §6)
└── .github/workflows/
```

Rules the layout encodes:

- **`spec/` is owned by neither ecosystem.** A language/behavior change lands as a
  feature-file edit **plus** both implementations, in that order. Until an
  implementation catches up, tag the scenario (`@swift-only` / `@rust-pending`) so
  each runner can skip-with-visibility rather than fail. CI enforces both runners
  green on untagged scenarios.
- **`docs/` stays shared.** ANZAN.md, FORMAT.md, DECIMAL.md, FIXED-WIDTH.md,
  MODES.md describe the *language and formats*, not an implementation. The
  "change the spec → change both" rule in CLAUDE.md now spans three artifacts:
  docs + spec + (two) implementations.
- **Interchange contracts** both apps must honor, forever: the `.soroban` package
  format (FORMAT.md — versioned JSON + `data.sqlite`), CSV encode/parse
  (`parse(encode(rows)) == rows`), and canonical `.normal`-mode source text (what's
  stored/replayed must stay byte-identical across ecosystems). A workbook saved in
  either app opens in the other. Add a `spec/` round-trip scenario per format
  change.

## 2. Phase 0 — restructure (pure moves, Swift stays green)

One PR, no behavior change, releasable:

1. `git mv Engine swift/Engine`, `git mv App swift/App`, `git mv Kit swift/Kit`,
   `git mv project.yml swift/project.yml` (fix its relative paths; the
   entitlements/Info blocks move with it).
2. Move `Engine/Tests/SorobanEngineTests/Features/*.feature` → `spec/anzan/` and
   `App/Tests/Session/session.feature` → `spec/session/`. Point the test targets at
   them: SwiftPM `resources: [.copy("../../../spec/anzan")]` doesn't allow paths
   outside the target, so use a **symlink inside the test target dir** to
   `spec/anzan` (git tracks symlinks; PickleKit reads via Bundle as today), or a
   pre-test copy step in CI + a local `make sync-spec`. Prefer the symlink — zero
   drift.
3. Update `ci.yml` / `release.yml` / `deploy-site.yml` working dirs and path
   filters (`Engine/**` → `swift/Engine/**` etc.); `salpa.yaml` paths likewise.
4. Update CLAUDE.md command blocks. Verify: `cd swift/Engine && swift test`,
   xcodegen + xcodebuild test, a release dry-run (`salpa build --explain`).

Everything after this is additive — the Swift app keeps shipping from `swift/`
throughout.

## 3. Phase 1 — `rust/anzan` (the language) + the parity harness

Port order mirrors the Swift module: **Number → Lexer → Parser → Eval → Functions
→ Calculator facade**. Design mappings:

| Swift | Rust |
| --- | --- |
| `BigDecimal` (BigInt significand × 10^exp, normalized) | own crate-internal type over `num-bigint` — **port digit-for-digit**, do NOT adopt the `bigdecimal` crate (different rounding/context semantics; our feature files pin full-precision values) |
| `PrecisionContext.current` (50 sig digits, banker's) | explicit context param or scoped thread-local (evaluation is single-threaded by discipline in both worlds) |
| `BigDecimal.viaDouble(...)` seam | `via_double(...)` seam — same single confinement rule. Use the pure-Rust `libm` crate, not platform libm: cross-OS determinism matters now that two implementations must agree to ~15 digits |
| `Value` enum + `EngineError` with positions | Rust enums; `Result<EvalOutcome, EngineError>` is already the Swift API shape — ports 1:1 |
| Evaluator frame-size / stack segmentation (`continueOnFreshStack`) | `stacker::maybe_grow` — same idea, one line; keep `maxTailIterations` / tail-call loop / `maxCallDepth` identical |
| resolver closures (`cellResolver`, `hostValueResolver`, …) | fields of `Box<dyn Fn…>` on the Calculator, or a `Host` trait with default-nil methods (prefer the trait — one seam instead of seven closures) |
| `HostObject` protocol | `trait HostObject` + `Value::Host(Rc<dyn HostObject>)` |
| `LanguageMode` parser/renderer parameterization | same — `parse(src, mode)`, `source_text(mode)`; `.normal` stays the only stored form |

**Parity harness**: `rust/anzan/tests/gherkin.rs` runs `spec/anzan/*.feature` with
the [`cucumber`](https://crates.io/crates/cucumber) crate — one static-world-per-
scenario, same as the PickleKit pattern. Steps port from `SorobanSteps.swift`
nearly mechanically. This harness is written **first** (against a stub calculator)
so every ported feature flips scenarios from red to green — the port has a
progress bar. What scenarios can't express stays as Rust unit tests, ported from
the Swift unit suites: typed-error *equality including positions*, codec round
trips, dependency-graph invalidation, the recursion canary.

Known parity risks to manage here, not discover later:

- **Transcendentals**: Swift routes through macOS libm; Rust must match to the
  precision the features state. Using the `libm` crate on both… isn't possible on
  the Swift side — so where last-ulp differences surface, loosen the specific
  scenario to stated-precision tolerance (the Excel-provenance rule already does
  this for finance) rather than chasing bit-equality. Audit once, early: run both
  lexers of `exp/ln/trig` scenarios and diff.
- **Gherkin dialect**: PickleKit and cucumber-rs both speak standard Gherkin
  (outlines, tables, docstrings), but verify the 14 files parse under cucumber-rs
  in week one, before writing steps.

## 4. Phase 2 — `rust/cli`, then `rust/engine`

- **CLI first** (small: Swift's is 209 lines): `rustyline` for the REPL (history at
  `~/.soroban_history` — same file, both CLIs share it; completion + gray signature
  hints via `completions(for_prefix)`/`FunctionDoc`, both engine-side like today).
  Mode by shape (args / piped stdin / tty) ports directly. Shipping a working
  `soroban` binary early validates the language crate's public API ergonomics.
- **Engine crate**: `Spreadsheet`, `SheetStore`, `Cell` classification (static in
  `Cell::new`, dynamic in recalc — keep the split), `ResolutionContext` dependency
  graph, `ReferenceRewriter`, `NamedCells`, `Controls`, definitions index; then
  Persistence: Workbook codec (serde against FORMAT.md — **the JSON is the
  contract, test by decoding fixture files saved by the Swift app**), journal
  (WAL replay idempotence test), `WorkbookPackage`, `DataStore` on `rusqlite`.
  `@_exported import Anzan` ≙ `pub use anzan::*`. The engine crate's cucumber
  runner picks up the spreadsheet/reflection/library features that need resolvers
  wired.

## 5. Phase 3 — rime extensions, then `rust/gui`

**Grow rime first, against `rime-demo`, domain-free** (the rime rule: components
hold no state, know nothing of the domain). Soroban needs, roughly in order:

1. **`grid`** — the big one. A virtualized spreadsheet grid: row/column headers,
   frozen headers, cell selection anchor+extent rectangles, per-cell `Element`
   content, resize-drag with preview guide lines, double-click hooks. iced 0.14
   has no virtualized table — this is a custom `Widget` (advanced API; see
   rime/ICED.md). Design it generic (a `fn(row, col) -> Element` cell factory +
   a viewport) so tty/fed-class apps can reuse it. Bake in the render-perf
   invariants CLAUDE.md records for SwiftUI — they translate: cell content must be
   cheap and diffable, selection changes must not rebuild all cells, judge on
   `--release`.
2. **`autocomplete_field`** — text input + suggestion popup + ↑/↓ dual role
   (suggestions when open, history otherwise) + programmatic-write suppression.
   Generalizes rime's `text_field`; fed/tty want this too.
3. **`bit_grid`** — the macOS-Calculator-style bit editor (labeled bit buttons,
   field bands with owned labels). This is also **Tama's** core. *(As built: the
   bit editor lives directly in `rust/gui` on rime's `bit_grid`; the per-field
   editors — enum pickers, numeric inputs, flag chips — and the visual format
   builder are gui/`Session` code, not rime widgets. `bit_grid` draws the bit
   register + colored `BitBand`s (labels are owned `String`s, so a host can pass
   a per-render decode like `owner rwx`). The once-planned `rust/kit` model crate
   mirroring `swift/Kit` was not split out; if Rust Tama happens, extract it
   then.)*
4. Small gaps as found: `log_list` (selectable text + context menu rows),
   caret-under-column error rendering (monospace + offset — trivial), cell
   format menus (rime `menu_bar`/`context_menu` already exist).

**Then `rust/gui`**: iced's Elm architecture replaces `@Observable` cleanly —
`CalculatorSession`/`SheetModel` become a `State` + `Message` enum + `update()`;
the undo stack (`SheetEdit`, grouped `[CellChange]`, cap 100) ports as data.
Themes: write a converter from `App/Resources/Themes/*.json` (10 built-ins) to
rime `Palette` + a Soroban `NamedTheme` (rime's `ThemeRegistry` handles user
themes/TOML for free — the site palette-sync rule now reads "themes are canonical
in one place, exported to swift/, rust/, and site/"; add a `scripts/` generator
rather than three hand-synced copies). Persistence targets the same App Support
paths? **No** — the Rust app is cross-platform; use `directories-rs` conventions,
but read/write the *same file formats* (`log.json`, scratch `Workbook`+journal,
workbook packages). The session-layer `spec/session/session.feature` runs against
the Rust session model with cucumber, same no-UI-automation stance as the Swift
`SorobanSessionTests`.

Port the GUI in vertical slices, each ending in a runnable app: ① log view + input
bar + history (a working calculator), ② grid read-only + ⌘\ toggle, ③ editing +
undo + point mode, ④ controls/formats/named cells, ⑤ inspector + binary editor +
reference window, ⑥ workbook manager (open/save/dirty/quit-prompt).

## 6. Phase 4 — CI & releases

- **ci.yml** gains a Rust job matrix: `cargo fmt --check`, `clippy`, `cargo test
  --workspace` (includes both cucumber runners) on macOS + Linux (+ Windows when
  gui compiles there). Path-filter so `swift/**`-only pushes don't run cargo and
  vice versa; `spec/**` and `docs/**` changes run **both**.
- **release.yml**: salpa is parameterized by ecosystem (`salpa.yaml: ecosystem
  apple`) — teach salpa a `rust` ecosystem (cargo build → bundle → sign/notarize
  the mac .app, plus Linux tarball/AppImage) or, pragmatically, keep salpa for the
  Swift dmg and add a `cargo-dist`-based job for Rust artifacts under the same
  auto-tag. Either way one tag → one GitHub Release carrying both apps' assets;
  the versionless `Soroban.dmg` keeps pointing at the Swift build until the Rust
  app is announced. Version is shared: one tag stream, both apps stamp it.
- Living spec/report generation (`deploy-site.yml`) keys off `spec/**` now and can
  render one report with per-ecosystem status columns — the public artifact of the
  parity contract.

## 7. Sequencing & effort (relative)

| Phase | What ships | Rough size |
| --- | --- | --- |
| 0 | monorepo restructure, Swift green | days |
| 1 | `anzan` crate + cucumber harness, all language features green | the long pole of the *engine* work (~8.4k loc Swift source, but mechanical with the oracle) |
| 2a | Rust `soroban` CLI | small |
| 2b | `engine` crate: sheet + persistence, workbook interchange proven | medium |
| 3a | rime: grid, autocomplete, bit_grid | the long pole of the *GUI* work; grid is the risk item — prototype it first |
| 3b | `gui` slices ①–⑥ | large, but each slice demos |
| 4 | CI matrix + dual-asset releases | days, amortized alongside |

Order phases 1→2 strictly (each layer's tests gate the next); 3a can start in
parallel with 2b since rime work is domain-free.

## 8. Risks

- **Grid performance/feel in iced** — ~~the one thing with no existing proof~~
  **RESOLVED**: the virtualized `grid` shipped (rime, custom `Widget`) through gui
  slice ⑥ + column resize and performs on a 1000×26 sheet; the canvas fallback
  wasn't needed.
- **Numeric divergence** — bounded by porting BigDecimal by hand + the
  full-precision feature values; transcendental ulp drift handled per §3.
- **Spec drift under dual maintenance** — the standing cost of "both live
  long-term." Mitigation is structural (spec/ gate in CI, tag discipline), but a
  feature added in one app under deadline pressure will happen; the living-spec
  report with per-ecosystem columns makes the debt visible instead of silent.
- **macOS-isms in the session layer** — NSPasteboard, responder-chain copy/paste,
  sandbox bookmarks, Finder UTIs have no direct iced analogue; the Rust app uses
  `arboard`/native dialogs (`rfd`) and forgoes sandboxing. Keep these host-side
  and thin, as the Swift app already does.
