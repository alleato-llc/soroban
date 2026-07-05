# Changelog

All notable changes to Soroban are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Versions are tagged `vX.Y.Z` and cut automatically by salpa on merge to `main`
(patch by default; `#minor` / `#major` in the merge commit bumps that part — see
[docs/RELEASING.md](docs/RELEASING.md)). The GitHub Release for each tag is the
point of truth for downloads.

## [Unreleased]

### Added

- Rust ecosystem, Phase 1 (docs/MIGRATION.md): the `rust/` cargo workspace
  with the `anzan` crate — the full language ported from Swift (BigDecimal
  number core, lexer, parser, evaluator with tail calls + stack segmentation,
  the complete builtin function library, JSON, documentation) plus a
  cucumber-rs harness running the shared `spec/anzan` Gherkin suite:
  the `soroban-engine` crate (Phase 2b: spreadsheet grid with dependency-
  graph recalc and cycle detection, sheet-scoped definitions, named cells,
  controls, cell formats, Workbook reflection, and the workbook JSON codec —
  `examples/mortgage.soroban`, written by the Swift app, decodes and restores
  in Rust), and the `soroban` CLI. The shared Gherkin suite passes 522/522
  in both ecosystems. New `rust-ci.yml` workflow (fmt/clippy/tests on
  Linux + macOS). No app behavior change.
- Rust ecosystem, Phase 2c (docs/MIGRATION.md): the engine-remainder ports that
  the shared Gherkin suite doesn't exercise on its own — token-precise
  reference rewriting (`ReferenceRewriter`: structural shifts, relative
  fill/paste adjustment, sheet-rename rewriting) and named-cell rewriting
  (`NamedCells`); the scratch journal (`WorkbookJournal`), document package
  reader/writer (`WorkbookPackage`), SQLite-backed data store (`DataStore`/
  `DataSheet`), and CSV codec; the log-only Workbook mutation commands
  (`updateCell`/`addWorksheet`/`renameWorksheet`/`deleteWorksheet`) with the
  in-cell refusal, and the structural-edit engine (`StructuralChange` insert/
  delete rows & columns with exact-inverse undo); the `History` reflection
  port (real `LogSource`, replacing the stub); the binary bit-editor model
  (`BinaryView`, bit-field formats, the visual `FormatBuilder`, and the
  `Bits::BitFormat` presets); and the reference-window documentation assembly.
  Ported with parity unit/integration suites (typed-error equality, recursion,
  cross-sheet invalidation, the mortgage workbook end-to-end); Gherkin stays
  522/522. No app behavior change.
- Rust ecosystem, Phase 3b slice ① (docs/MIGRATION.md): `rust/gui` — the first
  cut of the Rust/iced Soroban app, a working **log-view calculator** over the
  Anzan engine. Type an expression, press Enter, and the engine evaluates it
  into a newest-first log (values at full 50-digit precision, `λ`/`𝑫`
  definitions, comments, and errors with an aligned caret); ↑/↓ recall the
  input history; a rime-styled card + theme toggle. The engine/history logic
  lives in a UI-free `Session` (the Rust counterpart to the Swift
  `CalculatorSession`). The crate is **excluded** from the cargo workspace for
  now — it depends on the sibling `rime` kit by path and pulls in iced, so a
  workspace/CI build must not touch it; build it standalone with
  `cd rust/gui && cargo build`. Phase 4 moves it into a dedicated CI job. No
  change to the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ② (docs/MIGRATION.md): a **read-only
  spreadsheet grid** in `rust/gui`, sharing the log's engine session — ⌘\
  toggles between the log and the grid. Cells computed by the engine render
  through a new rime `grid` widget (numbers right-aligned, labels left,
  `#ERR`/`λ`/`𝑫`/notes styled from the theme palette), scroll virtualized over
  the full sheet, with click / shift-click selection. Because the log and grid
  share one `Calculator` + `SheetStore`, a `updateCell(cell("A",1), …)` typed
  into the log populates the grid and cell formulas recompute through the
  dependency graph. Editing lands in a later slice. No change to the existing
  crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ③ (docs/MIGRATION.md): **cell editing** in
  `rust/gui`. A formula/edit bar over the grid shows the selected cell's
  address and raw content; Enter commits, Escape cancels. Edits are
  **undoable** (⌘Z / ⇧⌘Z, grouped and capped like the Swift `SheetModel`), and
  navigating away commits an in-progress edit (Excel behavior). **Point mode**:
  clicking a cell while editing an operand-expecting draft inserts its `A:1`
  reference and refocuses the bar (gated on the engine's
  `Calculator.expectsOperand`) instead of moving the selection. No change to
  the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ④ (docs/MIGRATION.md): **interactive controls**
  in `rust/gui`. Selecting a control cell (slider / stepper / checkbox /
  dropdown) shows a control strip above the grid that drives it — dragging the
  slider, stepping ±, toggling, or picking an option rewrites the cell's stored
  literal in place via `Control::rewriting` and commits it as one undoable edit
  (so control changes join the ⌘Z history). Slider values snap to the step
  lattice exactly in `BigDecimal`. The grid renders each control's live value
  (a dropdown's string value shows as a label), and control cells feed the
  dependency graph like any other — a `= rate * 1000` formula recomputes as the
  slider moves. Uses rime's `slider`/`stepper`/`toggle`/`select`. No change to
  the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ④, part 2 (docs/MIGRATION.md): **cell formats**
  in `rust/gui`. A format bar over the grid sets the active cell's **number
  format** (general / number / currency / percent / date / hex / binary,
  rendered through `NumberFormat::rendered` — exact string/BigInt math, no
  float, so `1200` shows `$1,200.00` and `0.0825` shows `8.25%`), **alignment**,
  and **text / fill color** (semantic palette colors). Format edits are
  display-only and **undoable** — the undo model now carries cell-content *and*
  format steps (the Swift `SheetEdit.Kind.cells` / `.formats` split). Text
  styles (bold / italic / underline) are deferred pending a rime `GridCell`
  draw change. No change to the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ④, part 3 (docs/MIGRATION.md): **named cells**
  in `rust/gui`. An Excel-style name box (left of the formula bar) names the
  selected cell's location; a `'Rate'` reference in any formula then resolves
  through the name (dependency edges and cycle detection ride the ordinary
  cell-read path). Renaming rewrites every `'Old'` reference to `'New'` across
  the sheet token-precisely (`NamedCells::rewriting`) and clearing removes the
  name — both as one undoable step (the undo model gains a name edit alongside
  cells and formats). A duplicate/illegal name is rejected by the engine and
  the box reverts. No change to the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ⑤, part 1 (docs/MIGRATION.md): the **names
  inspector** in `rust/gui`. A "Names" toggle opens a sidebar listing every
  live name from both the log and the active sheet — variables (with values),
  named cells (address + value), functions (signatures), and data types —
  grouped and sorted, read-only. The Rust port of the Swift `InspectorView`'s
  two-source (log + sheet) merge. No change to the existing crates or the
  shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ⑤, part 2 (docs/MIGRATION.md): the **reference
  window** in `rust/gui`. A "Reference" toggle opens a searchable docs sidebar
  from `Calculator::documentation()` — the user's own functions and data types
  first (with their `# comment` docs), then Special Forms and every registry
  category, each entry showing signature + summary. A search field filters by
  signature/summary live. No change to the existing crates or the shared
  Gherkin suite.
- Rust ecosystem, Phase 3b slice ⑤, part 3 (docs/MIGRATION.md): the **binary
  bit-editor** in `rust/gui`. A "Bits" toggle opens a strip bound to the last
  result (`ans`): a plain integer edits as an unsigned register, an `Int…`/
  `UInt…` in two's-complement. Clicking a bit flips it (`BinaryView::flipping_bit`)
  and "Use in input" drops the current value into the log line to fold into an
  expression (the SpeedCrunch flow). A decimal / negative / too-wide value shows
  why it can't be edited. Uses rime's `bit_grid`. This completes slice ⑤. No
  change to the existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b slice ⑥ (docs/MIGRATION.md): the **workbook manager**
  in `rust/gui` — **New / Open / Save** in the top bar (⌘N / ⌘O / ⌘S), backed by
  native `rfd` file dialogs. Save writes a real `.soroban` document package (the
  engine `Workbook` codec — cells, names, and log-defined variables / functions /
  data types / namespaces via `soroban_engine::package`), remembering the file so
  a re-save skips the panel; Open restores through `restore_session` (types →
  functions → variables) and rebuilds the grid; New starts a fresh session. The
  title subtitle names the open document and shows a `•` when the live revision
  has moved past the last save (a session `revision` counter bumped on every
  submit / edit / undo / redo). This completes **Phase 3b** — the Rust/iced app
  now covers the log calculator, the editable grid, controls, formats, named
  cells, the inspector, the reference window, the binary editor, and workbook
  save/open. `rust/gui` stays out of the cargo workspace (path-dep on `rime` +
  iced; build standalone) until Phase 4 wires it into CI. No change to the
  existing crates or the shared Gherkin suite.
- Rust ecosystem, Phase 3b — `rust/gui` **chrome pass** to match the AppKit
  original's minimalist REPL feel: the log's input bar is pinned to the **bottom**
  behind a `›` prompt with the log flowing oldest→newest (freshest result just
  above the input), the expression echo is inked in the accent color and its
  result in plain ink (matching the original), and the window is edge-to-edge —
  the wordmark and card frame are gone, with the document name + unsaved-changes
  `•` moved to the **window title** (`Soroban・算盤 — Untitled`). The action
  buttons (New / Open / Save / Bits / Names / Reference / theme / view-toggle)
  become a slim, **left-aligned strip that auto-hides** like `fed`'s chrome —
  revealed only while the pointer is at the top edge (a `chrome_revealed` flag
  driven by pointer-Y with hysteresis). No engine or session behavior change.
- Rust ecosystem, dev tooling — a permanent, env-gated **review-screenshot
  harness** in `rust/gui` (`src/shot.rs`). iced captures its own window via wgpu
  readback (headless, no screen-recording permission); the harness is inert
  unless `SOROBAN_SHOT=<path>` is set and is fully parameterized by environment
  (`SOROBAN_SHOT_SEED` = a file of log inputs to run first, `SOROBAN_SHOT_VIEW`,
  `SOROBAN_SHOT_SELECT`, `SOROBAN_SHOT_CHROME`, `SOROBAN_SHOT_PANEL`) so a slice
  can be captured for review without editing code. Adds a `png` dependency
  (gui-only).
- Rust ecosystem, Phase 3b — `rust/gui` **fidelity batch** closing the visible
  gap to the AppKit original: the log's prompt is `>` (was `›`); the empty state
  reads "Type an expression below — or click one:" with three clickable sample
  expressions that insert themselves into the input; the two signature corner
  icons (docs 📖 / grid ▦) sit at the input's right, always visible. The
  **inspector** is rebuilt to match — an `Environment` header over small-caps
  `VARIABLES` / `FUNCTIONS` / `DATA TYPES` sections (named cells fold into
  Variables), each row tagged with its provenance: a muted `log`, or a clickable
  `B:2 ↗` that jumps to and selects the cell. The grid gains a `Sheet 1 +`
  tab strip at the bottom-left (replacing the "Grid — Sheet N" label). Cosmetic
  only — no engine or session behavior change.
- Rust ecosystem, Phase 3b — **inline cell editing** in `rust/gui`, the biggest
  remaining fidelity item. Double-clicking a cell (or point-mode reference
  insertion) now opens a text editor **inside the cell** (mirroring the formula
  bar), the way the AppKit app edits — instead of only the top formula bar. Built
  on a new rime `grid` capability (`.editor(row, col, element)` + `.on_activate`),
  the grid hosts the editor over the active cell and forwards it events + focus;
  Enter commits, Esc cancels, and clicking another cell mid-edit still inserts its
  reference (point mode) and refocuses the inline editor. Requires rime with the
  grid inline-editor support (path dependency). No engine or session behavior
  change.
- Rust ecosystem, Phase 3b — **inline controls** in `rust/gui`: slider / stepper /
  checkbox / dropdown cells now render their interactive widget **inside the cell**
  (the AppKit behavior), driven directly there, instead of in a control strip
  above the grid. Built on rime's generalized grid overlays (`.overlay(row, col,
  element)`), the app hosts a compact control over each control cell (the cell
  address rides each message, since many are live at once) and the control strip
  is gone. A new `Session::control_cells()` enumerates them by scanning only the
  sheet's occupied cells. Requires rime with grid overlay support (path dep). No
  engine or session behavior change.
- Rust ecosystem, Phase 3b — **grid usability gaps** in `rust/gui`, closing the
  last interaction differences from the AppKit grid:
  - **Keyboard navigation.** Arrow keys move the selection (Shift-arrow extends
    it), Enter/type-to-edit opens the inline editor (a printable key seeds it),
    Enter commits and advances down (Excel-style), Esc cancels. Grid-only keys
    are gated on `mode == Grid && !editing` so a focused editor keeps its own
    keys (iced gives no focus query); the clamping lives in a pure
    `next_selection` (unit-tested).
  - **Copy / cut / paste.** ⌘C/⌘X copy the selection as TSV (Excel/Numbers
    interop) via `Session::selection_tsv`; ⌘V pastes clipboard TSV from the
    anchor, clipped to the grid, as one undoable edit (`paste_tsv`); cut also
    clears the source range.
  - **Column-width resize.** Drag a column's right border in the header strip to
    resize it (↔ cursor, 24px minimum); widths persist per sheet in the workbook.
    Built on rime's new `grid` per-column widths + `.on_resize_column`.
  Requires rime with the per-column-width grid + public `Selection::bounds()`
  (path dependency). No engine behavior change.
- Rust ecosystem, Phase 3b — **menu-bar chrome** in `rust/gui`, matching the
  AppKit app's window chrome. The auto-hiding row of ghost text buttons is
  replaced by a **File / Edit / View menu bar** (rime's `menu_bar`, with
  ⌘-shortcut hints): File → New/Open/Save, Edit → Undo/Redo/Copy/Cut/Paste,
  View → Show Grid·Log / Names / Reference / Bits / theme. A **sidebar-toggle
  icon pins to the bar's right** (like the AppKit title bar's toolbar item) and
  a **log/grid view-toggle icon sits bottom-right**, mirroring the original's
  corner affordances. Works identically in calculator (log) and grid modes.
  Built on rime's new `menu_bar_with_trailing`; drops the pointer-tracked
  auto-hide plumbing. No engine behavior change.
- Rust ecosystem, Phase 3b — **headless session-scenario suite** for `rust/gui`
  (`tests/session.feature` + a cucumber-rs runner), the Rust counterpart to the
  Swift `SorobanSessionTests` but a **fast `cargo test`** (~0.15s for 16
  scenarios — it drives the UI-free `Session` directly, no iced, no rendering,
  no `xcodebuild`). Covers the calculator (log values, `ans`, function defs,
  errors, comments) and the sheet (cell values / formulas / labels / errors,
  shared log↔grid variables, undo·redo, **point mode** (Excel-style reference
  insertion — clicking a cell mid-formula splices its reference into the draft),
  checkbox·slider commits, TSV copy/paste, named cells, column-width round-trip,
  workbook save/reopen). The gui crate gains a `[lib]` target exposing `Session`
  so the suite can link it without iced. Rust-only by design (the cross-ecosystem
  parity oracle stays `spec/anzan`, run by the engine's gherkin suite). No engine
  behavior change.
- Rust ecosystem, Phase 3b — **point mode inserts a cell's *name*** (`'Rate'`),
  not just its `A:1` address, when the clicked cell is named — matching the
  AppKit app; the logic is centralized in a tested `Session::point_click`
  (Excel's "expecting an operand → insert reference, else commit" rule) that the
  grid click handler now calls.

### Changed

- Restructured into an ecosystem-first monorepo (Phase 0 of
  [docs/MIGRATION.md](docs/MIGRATION.md)): everything Apple moved under
  `swift/` (`Engine/`, `App/`, `Kit/`, `project.yml`, the app's `salpa.yaml`);
  the Gherkin feature files moved to a shared top-level `spec/`
  (`spec/anzan/`, `spec/session/`), symlinked into the test targets, to serve
  as the cross-ecosystem parity oracle for the planned Rust port. The repo-root
  `salpa.yaml` now holds only the site deploy. No app behavior change.
- Docs: documented the `[skip ci]` convention for docs / CHANGELOG / test-only
  commits (salpa tags every push to `main` but never edits this file, so
  `[Unreleased]` entries are promoted to a dated `[vX.Y.Z]` by hand).

## [1.4.5] — 2026-06-17

### Changed

- Docs: `docs/PROGRAMMER.md`, `docs/MODULES.md`, `README.md`, and `CLAUDE.md`
  now cover the expanded preset catalog, all **five** field kinds (numeric /
  flags / enum / **reserved** / **unused**), the `base` field in the `Bits`
  schema, and the `formatValue` encoder.

## [1.4.4] — 2026-06-17

### Changed

- Internal only: real-world decode tests for the new bit-formats (IEEE 754,
  `st_mode`, EFLAGS, RGBA8888) — no user-facing change.

## [1.4.3] — 2026-06-17

### Added

- **14 new built-in bit-formats** across four groups: floating point (IEEE 754
  float/double/half, bfloat16), color (RGBA8888, ARGB1555, RGBA4444),
  networking (DNS header flags, VLAN 802.1Q tag, IPv4 DSCP/ECN), and systems
  (x86 EFLAGS, Unix `st_mode`, FAT date, FAT time). The richer ones mix
  enum, flag, and reserved sub-fields in one layout — backed by a new
  `BinaryView.formatValue([FieldSpec])` encoder that round-trips enum /
  reserved / unused fields (and duplicate field names like EFLAGS's repeated
  `reserved`/`flags`).

## [1.4.2] — 2026-06-16

### Changed

- A bit-field **format now fixes the register width** to its own size — IPv4 is
  32 bits, MAC 48, IPv6 128; applying one snaps the width (up or down) and the
  width selector locks to it. No more widening a format into meaningless unused
  high bits. (Applies to built-in and saved formats alike.)

## [1.4.1] — 2026-06-16

### Fixed

- Bit-editor builder: a single claim can now span **all** the free bits, not
  just 32 — so you can reserve e.g. 47 of 48 bits in one field instead of being
  left with a chunk still "available". (The open cells wrap.)

## [1.4.0] — 2026-06-16

### Fixed

- Bit editor: a wide **field band** — a Reserved/Unused gap, or any field
  wider than the card — now **wraps** into rows instead of overflowing the
  editor (matches the unused high band). Previously a wide reserved span ran
  off-screen.

### Added

- The **calculator** now manages its saved bit-formats in the editor too —
  **rename/delete** from the Format menu (previously only the standalone Tama
  app could; the calculator's were managed via the log). Backed by new
  off-log primitives `Calculator.setUserVariable` / `removeUserVariable`.

## [1.3.0] — 2026-06-16

### Added

- **Binary bit-editor parity** (shared `BinaryEditorKit`, so both the calculator
  and the standalone Tama app get it):
  - A **48-bit** register width (8/16/32/48/64/128/256) — a MAC fits exactly.
  - **Reserved** (locked, must-be-zero) and **Unused** (don't-care, editable)
    bit-field kinds in the builder, persisted in the typed `Bits::BitFormat`
    (`kind: "reserved"` / `"unused"`).
  - **Build new…** vs **Edit current…** — the builder no longer silently edits
    the active format; the Format menu is disabled mid-build.
  - Out-of-format ("unused") bits are grayed and locked until a deliberate
    double-click enables editing (one confirm); the unused high band wraps so a
    wide span (e.g. IPv6 in a 256-bit register) doesn't overflow.
  - Hosts that own their format store (Tama) gain in-editor **rename/delete** of
    saved formats (`BinaryEditorHost.canManageSavedFormats`).

## [1.2.0] — 2026-06-15

### Added

- **Language modes** — a default dialect and a **Programmer** dialect where the
  overloaded glyphs read as C operators (`^ & | << >> ~`, `%` as modulo); a
  display dialect only, the stored formula stays canonical.
- **Fixed-width integers** — `Int8…Int256` / `UInt8…UInt256` and parameterized
  `Int(v, bits)` / `UInt(v, bits)` (8–256 bits, signed/unsigned), exact and
  *checked*: overflow is an error, never a silent wraparound.
- **Fixed-precision decimals** — a SQL-style `Decimal(p, s)` money type that
  rounds to scale and errors on precision overflow.
- **Unix-style `man` pages** for built-ins, with every documented example
  evaluated by the test suite.
- **Programmer-mode binary bit-editor** — a clickable bit grid bound to the last
  result; flip bits to build a value, then **Use** it (or double-click the
  decimal/hex) to drop it into the expression. SpeedCrunch-style `ans`-prefix:
  a leading operator on an empty line continues the last result.
- **Bit-field formats** — label a register's bit ranges as **numeric**,
  per-bit **flags** (`r-x`), or **enum** fields. Built-in presets (Unix
  permissions, TCP flags, RGB565) plus networking presets (IPv4, IPv6, MAC) with
  per-field hex. Formats persist as typed `Bits::BitFormat` records, and a
  **visual builder** carves one by claiming bits, naming, and coloring them.
- **Module system** — namespaces (`Geo::Point`, `namespace` blocks, nested),
  `import` with loud conflict reporting, generic/typed `data` records (list,
  nested-list, and map fields), namespaced functions and types, constants, and
  built-ins reachable as `Module::name` behind the global prelude.
- **Site** — a nine-feature information architecture, a docs refresh, and the
  standard-library note.

### Changed

- Extracted the binary bit-editor UI into a shared **`BinaryEditorKit`** Swift
  package, so the calculator and the standalone [Tama](https://github.com/alleato-llc/soroban)
  app share one component instead of duplicating it.

### Removed

- The text-spec **"Custom…"** format entry in the bit editor — the visual
  builder is a strict superset (it also authors flag/enum fields, colors, and
  per-field bases), so the redundant `owner:3 group:3` text path was dropped.

## [1.1.9] — 2026-06-11

### Fixed

- Anzan: a number can no longer directly follow another value — this is now an
  error rather than a silent multiplication.

## [1.1.8] — 2026-06-11

### Fixed

- Restored the stable download asset after the infrastructure-scrub history
  reset.

## [1.1.7] — 2026-06-11

### Added

- First public release of Soroban — an exact-arithmetic spreadsheet calculator
  for macOS, built on a 50-digit exact decimal engine with the Anzan language.

[Unreleased]: https://github.com/alleato-llc/soroban/compare/v1.4.5...HEAD
[1.4.5]: https://github.com/alleato-llc/soroban/compare/v1.4.4...v1.4.5
[1.4.4]: https://github.com/alleato-llc/soroban/compare/v1.4.3...v1.4.4
[1.4.3]: https://github.com/alleato-llc/soroban/compare/v1.4.2...v1.4.3
[1.4.2]: https://github.com/alleato-llc/soroban/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/alleato-llc/soroban/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/alleato-llc/soroban/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/alleato-llc/soroban/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/alleato-llc/soroban/compare/v1.1.9...v1.2.0
[1.1.9]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.9
[1.1.8]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.8
[1.1.7]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.7
