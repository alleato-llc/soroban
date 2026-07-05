# Changelog — Rust ecosystem

Changes to the **Rust ecosystem**: the `anzan` language crate, `soroban-engine`,
the `soroban` CLI, and the `rust/gui` iced desktop app. Cross-ecosystem changes
(both Swift and Rust) live in the repo-root [CHANGELOG.md](../CHANGELOG.md); the
Swift ecosystem has its own [swift/CHANGELOG.md](../swift/CHANGELOG.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This track is versioned `rust-vX.Y.Z` (its own sequence, independent of the
Swift `vX.Y.Z` tags), cut on merge to `main` that touches `rust/**` or `spec/**`
(patch by default; `#minor` / `#major` in the merge commit bumps that part — see
[docs/RELEASING.md](../docs/RELEASING.md)). Each `rust-v*` GitHub Release ships
portable, unsigned Linux / Windows / macOS binaries.

## [Unreleased]

### Added

- Phase 1 (docs/MIGRATION.md): the `rust/` cargo workspace with the `anzan`
  crate — the full language ported from Swift (BigDecimal number core, lexer,
  parser, evaluator with tail calls + stack segmentation, the complete builtin
  function library, JSON, documentation) plus a cucumber-rs harness running the
  shared `spec/anzan` Gherkin suite: the `soroban-engine` crate (Phase 2b:
  spreadsheet grid with dependency-graph recalc and cycle detection,
  sheet-scoped definitions, named cells, controls, cell formats, Workbook
  reflection, and the workbook JSON codec — `examples/mortgage.soroban`, written
  by the Swift app, decodes and restores in Rust), and the `soroban` CLI. The
  shared Gherkin suite passes 522/522 in both ecosystems. New `rust-ci.yml`
  workflow (fmt/clippy/tests on Linux + macOS).
- Phase 2c (docs/MIGRATION.md): the engine-remainder ports that the shared
  Gherkin suite doesn't exercise on its own — token-precise reference rewriting
  (`ReferenceRewriter`: structural shifts, relative fill/paste adjustment,
  sheet-rename rewriting) and named-cell rewriting (`NamedCells`); the scratch
  journal (`WorkbookJournal`), document package reader/writer (`WorkbookPackage`),
  SQLite-backed data store (`DataStore`/`DataSheet`), and CSV codec; the log-only
  Workbook mutation commands (`updateCell`/`addWorksheet`/`renameWorksheet`/
  `deleteWorksheet`) with the in-cell refusal, and the structural-edit engine
  (`StructuralChange` insert/delete rows & columns with exact-inverse undo); the
  `History` reflection port (real `LogSource`, replacing the stub); the binary
  bit-editor model (`BinaryView`, bit-field formats, the visual `FormatBuilder`,
  and the `Bits::BitFormat` presets); and the reference-window documentation
  assembly. Ported with parity unit/integration suites (typed-error equality,
  recursion, cross-sheet invalidation, the mortgage workbook end-to-end); Gherkin
  stays 522/522.
- Phase 3b slice ① (docs/MIGRATION.md): `rust/gui` — the first cut of the
  Rust/iced Soroban app, a working **log-view calculator** over the Anzan engine.
  Type an expression, press Enter, and the engine evaluates it into a newest-first
  log (values at full 50-digit precision, `λ`/`𝑫` definitions, comments, and
  errors with an aligned caret); ↑/↓ recall the input history; a rime-styled card
  + theme toggle. The engine/history logic lives in a UI-free `Session` (the Rust
  counterpart to the Swift `CalculatorSession`). The crate is **excluded** from
  the cargo workspace for now — it depends on the sibling `rime` kit by path and
  pulls in iced, so a workspace/CI build must not touch it; build it standalone
  with `cd rust/gui && cargo build`.
- Phase 3b slice ② (docs/MIGRATION.md): a **read-only spreadsheet grid** in
  `rust/gui`, sharing the log's engine session — ⌘\ toggles between the log and
  the grid. Cells computed by the engine render through a new rime `grid` widget
  (numbers right-aligned, labels left, `#ERR`/`λ`/`𝑫`/notes styled from the theme
  palette), scroll virtualized over the full sheet, with click / shift-click
  selection. Because the log and grid share one `Calculator` + `SheetStore`, a
  `updateCell(cell("A",1), …)` typed into the log populates the grid and cell
  formulas recompute through the dependency graph.
- Phase 3b slice ③ (docs/MIGRATION.md): **cell editing** in `rust/gui`. A
  formula/edit bar over the grid shows the selected cell's address and raw
  content; Enter commits, Escape cancels. Edits are **undoable** (⌘Z / ⇧⌘Z,
  grouped and capped like the Swift `SheetModel`), and navigating away commits an
  in-progress edit (Excel behavior). **Point mode**: clicking a cell while editing
  an operand-expecting draft inserts its `A:1` reference and refocuses the bar
  (gated on the engine's `Calculator.expectsOperand`) instead of moving the
  selection.
- Phase 3b slice ④ (docs/MIGRATION.md): **interactive controls** in `rust/gui`.
  Selecting a control cell (slider / stepper / checkbox / dropdown) shows a
  control strip above the grid that drives it — dragging the slider, stepping ±,
  toggling, or picking an option rewrites the cell's stored literal in place via
  `Control::rewriting` and commits it as one undoable edit. Slider values snap to
  the step lattice exactly in `BigDecimal`. The grid renders each control's live
  value, and control cells feed the dependency graph like any other. Uses rime's
  `slider`/`stepper`/`toggle`/`select`.
- Phase 3b slice ④, part 2 (docs/MIGRATION.md): **cell formats** in `rust/gui`.
  A format bar over the grid sets the active cell's **number format** (general /
  number / currency / percent / date / hex / binary, rendered through
  `NumberFormat::rendered` — exact string/BigInt math, no float, so `1200` shows
  `$1,200.00` and `0.0825` shows `8.25%`), **alignment**, and **text / fill
  color** (semantic palette colors). Format edits are display-only and
  **undoable** — the undo model now carries cell-content *and* format steps (the
  Swift `SheetEdit.Kind.cells` / `.formats` split). Text styles (bold / italic /
  underline) are deferred pending a rime `GridCell` draw change.
- Phase 3b slice ④, part 3 (docs/MIGRATION.md): **named cells** in `rust/gui`. An
  Excel-style name box (left of the formula bar) names the selected cell's
  location; a `'Rate'` reference in any formula then resolves through the name
  (dependency edges and cycle detection ride the ordinary cell-read path).
  Renaming rewrites every `'Old'` reference to `'New'` across the sheet
  token-precisely (`NamedCells::rewriting`) and clearing removes the name — both
  as one undoable step. A duplicate/illegal name is rejected by the engine and
  the box reverts.
- Phase 3b slice ⑤, part 1 (docs/MIGRATION.md): the **names inspector** in
  `rust/gui`. A "Names" toggle opens a sidebar listing every live name from both
  the log and the active sheet — variables (with values), named cells (address +
  value), functions (signatures), and data types — grouped and sorted, read-only.
  The Rust port of the Swift `InspectorView`'s two-source (log + sheet) merge.
- Phase 3b slice ⑤, part 2 (docs/MIGRATION.md): the **reference window** in
  `rust/gui`. A "Reference" toggle opens a searchable docs sidebar from
  `Calculator::documentation()` — the user's own functions and data types first
  (with their `# comment` docs), then Special Forms and every registry category,
  each entry showing signature + summary. A search field filters live.
- Phase 3b slice ⑤, part 3 (docs/MIGRATION.md): the **binary bit-editor** in
  `rust/gui`. A "Bits" toggle opens a strip bound to the last result (`ans`): a
  plain integer edits as an unsigned register, an `Int…`/`UInt…` in two's-
  complement. Clicking a bit flips it (`BinaryView::flipping_bit`) and "Use in
  input" drops the current value into the log line to fold into an expression (the
  SpeedCrunch flow). A decimal / negative / too-wide value shows why it can't be
  edited. Uses rime's `bit_grid`. This completes slice ⑤.
- Phase 3b slice ⑥ (docs/MIGRATION.md): the **workbook manager** in `rust/gui` —
  **New / Open / Save** in the top bar (⌘N / ⌘O / ⌘S), backed by native `rfd`
  file dialogs. Save writes a real `.soroban` document package (the engine
  `Workbook` codec — cells, names, and log-defined variables / functions / data
  types / namespaces via `soroban_engine::package`), remembering the file so a
  re-save skips the panel; Open restores through `restore_session` (types →
  functions → variables) and rebuilds the grid; New starts a fresh session. The
  title subtitle names the open document and shows a `•` when the live revision
  has moved past the last save. This completes **Phase 3b** — the Rust/iced app
  now covers the log calculator, the editable grid, controls, formats, named
  cells, the inspector, the reference window, the binary editor, and workbook
  save/open.
- Phase 3b — **chrome pass** to match the AppKit original's minimalist REPL feel:
  the log's input bar is pinned to the **bottom** behind a `›` prompt with the log
  flowing oldest→newest, the expression echo is inked in the accent color and its
  result in plain ink, and the window is edge-to-edge — the wordmark and card
  frame are gone, with the document name + unsaved-changes `•` moved to the
  **window title**. The action buttons became a slim, left-aligned strip that
  auto-hides like `fed`'s chrome.
- Phase 3b — **menu-bar chrome**: the auto-hiding row of ghost text buttons is
  replaced by a **File / Edit / View menu bar** (rime's `menu_bar`, with
  ⌘-shortcut hints): File → New/Open/Save, Edit → Undo/Redo/Copy/Cut/Paste,
  View → Show Grid·Log / Names / Reference / Bits / theme. A **sidebar-toggle
  icon pins to the bar's right** and a **log/grid view-toggle icon sits
  bottom-right**, mirroring the original's corner affordances. Built on rime's
  new `menu_bar_with_trailing`.
- Phase 3b — **fidelity batch** closing the visible gap to the AppKit original:
  the log's prompt is `>`; the empty state reads "Type an expression below — or
  click one:" with three clickable sample expressions; the two signature corner
  icons (docs 📖 / grid ▦) sit at the input's right. The **inspector** is rebuilt
  to match — an `Environment` header over small-caps `VARIABLES` / `FUNCTIONS` /
  `DATA TYPES` sections, each row tagged with its provenance (a muted `log`, or a
  clickable `B:2 ↗` that jumps to the cell). The grid gains a `Sheet 1 +` tab
  strip at the bottom-left.
- Phase 3b — **inline cell editing**: double-clicking a cell (or point-mode
  reference insertion) opens a text editor **inside the cell** (the AppKit
  behavior) instead of only the top formula bar. Built on a new rime `grid`
  capability (`.editor(row, col, element)` + `.on_activate`); Enter commits, Esc
  cancels, and clicking another cell mid-edit still inserts its reference (point
  mode) and refocuses the inline editor.
- Phase 3b — **inline controls**: slider / stepper / checkbox / dropdown cells
  render their interactive widget **inside the cell** (the AppKit behavior),
  driven directly there, instead of in a control strip above the grid. Built on
  rime's generalized grid overlays (`.overlay(row, col, element)`); a new
  `Session::control_cells()` enumerates them by scanning only the sheet's occupied
  cells.
- Phase 3b — **grid usability gaps**, closing the last interaction differences
  from the AppKit grid:
  - **Keyboard navigation.** Arrow keys move the selection (Shift-arrow extends
    it), Enter/type-to-edit opens the inline editor, Enter commits and advances
    down (Excel-style), Esc cancels; the clamping lives in a pure `next_selection`
    (unit-tested).
  - **Copy / cut / paste.** ⌘C/⌘X copy the selection as TSV (Excel/Numbers
    interop) via `Session::selection_tsv`; ⌘V pastes clipboard TSV from the anchor,
    clipped to the grid, as one undoable edit (`paste_tsv`); cut also clears the
    source range.
  - **Column-width resize.** Drag a column's right border to resize it (↔ cursor,
    24px minimum); widths persist per sheet in the workbook. Built on rime's new
    per-column widths + `.on_resize_column`.
- Phase 3b — the **bit editor gains its bit-format layer** (parity with the
  AppKit app): a **width picker** (8…256, a fixed-width int stays locked to its
  own width), a **hex readout** (`0x1F4`), a **format dropdown** over the built-in
  presets that decodes the value into **named, colored fields** (`owner rwx`, …);
  **per-field editors** (enum picker, base-aware numeric input, flag chips,
  reserved lock); and **custom build & save** — a visual builder (claim bits,
  name/kind/labels/base, add, Apply or Save) whose saved formats ride the workbook
  via `Calculator::set_user_variable` and survive save/reopen. All wired through
  the UI-free `Session` with headless scenarios.
- Phase 3b — **point mode** improvements: inserts a cell's *name* (`'Rate'`) when
  the clicked cell is named (centralized in a tested `Session::point_click`); plus
  **re-click-replace** (`=B:1` → `=C:1`) and **shift-click-extend** into a range
  (`=sum(B:1` → `=sum(B:1..B:4`), via an `extend` flag + small anchor state.
- Phase 3b — **headless session-scenario suite** for `rust/gui`
  (`tests/session.feature` + a cucumber-rs runner), the Rust counterpart to the
  Swift `SorobanSessionTests` but a **fast `cargo test`** — it drives the UI-free
  `Session` directly (no iced, no rendering). Covers the calculator, the sheet
  (values/formulas/labels/errors, shared log↔grid variables, undo·redo of
  cell/format/name edits, point mode, all four controls, TSV copy/cut/paste,
  named cells + rename-rewrite, formatting, ↑/↓ history, the inspector, the
  reference window, the bit-editor, column-width round-trip, workbook
  save·reopen·new), taking `session.rs` to **~90% line coverage**. Rust-only by
  design (the cross-ecosystem parity oracle stays `spec/anzan`). The gui crate
  gains a `[lib]` target exposing `Session` so the suite links it without iced.
- Phase 3b — a permanent, env-gated **review-screenshot harness** in `rust/gui`
  (`src/shot.rs`). iced captures its own window via wgpu readback (headless, no
  screen-recording permission); inert unless `SOROBAN_SHOT=<path>` is set and
  fully parameterized by environment (`SOROBAN_SHOT_SEED`, `_VIEW`, `_SELECT`,
  `_CHROME`, `_PANEL`, `_WIDTH`, `_FORMAT`, `_BUILD`). Adds a gui-only `png`
  dependency.
- Phase 3b — **macOS UX pass** closing feature gaps against the AppKit app:
  - **Real icons.** rime now embeds a tiny Lucide subset (`rime::icons`), so the
    toolbar / view-toggle / close glyphs render crisply instead of as tofu; the
    grid's "𝑫" definition marker and hamburger placeholders are gone.
  - **Calculator modes.** The `:mode [normal|programmer|finance]` command
    switches the log's input/display dialect (`Calculator::mode`) — `^`/`%`/`&`
    become bitwise in programmer mode — mirroring the CLI's `:mode`.
  - **History reflection.** The log tape is shared (`Rc<RefCell>`) into the
    engine's `History` reflection, so a log-line `len(History)` /
    `first(History).value` reads the live calculation log.
  - **Autocomplete.** Typing in the log bar or the grid formula bar shows a
    completion popup (`Calculator::completions` over the trailing identifier):
    ↑/↓ move the highlight, Tab / Enter accept (a function gets its `(`), a click
    accepts a row. The popup rises *above* the bottom-anchored log prompt, on
    rime's new `suggestion_list`. New `SOROBAN_SHOT_TYPE` shot-harness knob.
  - **Auto-hiding menu bar.** The in-window File / Edit / View bar (chrome, since
    iced has no system menu bar) now hides so content fills the whole window,
    revealing only while the pointer hugs the top edge or a menu is open — it
    overlays the top rather than pushing content down, so nothing jumps.
