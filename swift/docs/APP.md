# The Soroban app (macOS + iPad)

The SwiftUI app that hosts Anzan in a calculation log and a spreadsheet grid.
Part 1 is the **architecture** (the session model, the grid views and their
performance rules, persistence, theming, reflection glue); Part 2 is the
**feature tour** — the user-facing surface. Build/test/run:
[../README.md](../README.md). For what the *language* does, see
[../../docs/ANZAN.md](../../docs/ANZAN.md); this page covers the app.

---

# Part 1 — Architecture

The app is a thin view layer over `SorobanEngine`. The model layer lives under
`App/Sources/Session/` (UI-free, testable without a host app); the views are the
top-level files in `App/Sources/`.

## The session model

- **`CalculatorSession`** (`Session/CalculatorSession.swift`, `@Observable`
  `@MainActor`) wraps `Calculator` and owns: the log (via `LogStore`), the
  persisted ↑/↓ input history, the `SheetModel` (grid), the `WorkbookManager`
  (Save/Open), the `mode` (UserDefaults-persisted dialect), and `activeView`
  (log ↔ grid, ⌘\). Concerns split into `CalculatorSession+Binary.swift` (the
  bit-editor staging) and `CalculatorSession+Autocomplete.swift`.
- **`LogStore`** (`Session/LogStore.swift`) is the UI-free model owning the
  `[HistoryEntry]` tape (`Session/HistoryEntry.swift`) + its persistence
  (`log.json`), and conforms to the engine's `LogSource` **directly** — no
  adapter, no `MainActor.assumeIsolated` (it's a plain `@unchecked Sendable`
  class, not `@MainActor`). `CalculatorSession` is a thin view-model over it:
  `entries` proxies `log.entries`, and observation bridges through
  `logGeneration` (the grid's `generation` pattern, since `LogStore` isn't
  `@Observable`). The visible tape is a **global** running log (one across the
  session, not per-workbook), capped at 500.
- **`WorkbookManager`** (`Session/WorkbookManager.swift`, +
  `WorkbookFileDialogs.swift`) owns `fileURL`/`isDirty` and the NSOpen/NSSave
  panels.

### SheetModel

`SheetModel` is **one type** across `Session/SheetModel/`. `@Observable`
requires every **stored** property in the class body (all in `SheetModel.swift`,
grouped by owner), while behavior lives in per-concern extensions —
`SheetModel+`: `Persistence`, `Workbook`, `Worksheets`, `Layout`, `Formatting`,
`Names`, `Controls`, `Structure`, `Clipboard`, `PointMode`, `DataSheets`, `CSV`,
`Mutation`, `Inspector`. Cross-file members are internal by necessity; treat
anything commented as extension state as private to its section. `SheetModel`
proxies the active sheet of the underlying `SheetStore`.

`SheetModel.generation` is the **observation bridge**: the engine `Spreadsheet`
isn't `@Observable`, so cell views re-render by reading `generation`, bumped on
every commit/recalc. Anything that can change cell values (e.g. a log submission
assigning a variable) **must call `sheet.recalculate()`**.

## The grid views & their performance invariants

The view split (all under `App/Sources/`): `ContentView` → `SpreadsheetView`
(container / keyboard / resize; `GridRowView` lives here too) → `CellView.swift`
(+ `PaletteColor` mapping) → `CellControls.swift` / `CellEditorView.swift`.
`FormatActions.swift` is the single shared Format-menu definition (menu bar +
per-cell context menu). Views read every color/font from `ThemeManager.current`
— **never hardcode colors.**

These rules are correctness-critical for a 26×1,000 grid; profile/judge on a
**Release** build (Debug SwiftUI with thousands of views is 5–10× slower):

1. **`CellView` must NOT read the observable model.** `GridRowView` reads
   selection/display once per row and passes everything down as `Equatable`
   lets; `CellView` conforms to `Equatable` (nonisolated `==`) so SwiftUI skips
   unchanged cell bodies. Cells observing `sheet.selected` directly once made
   every click invalidate ~1,000 visible bodies.
2. **Per-cell `@State`/`@FocusState` is banned** — editor machinery lives in
   `CellEditorView`, instantiated only for the single editing cell. Editor focus
   is grabbed via `DispatchQueue.main.async` in `onAppear` (synchronous
   FocusState writes don't stick mid-update inside the lazy grid).
3. The cell double-tap is a **`simultaneousGesture`**, not stacked
   `onTapGesture(count: 2)` — stacking makes SwiftUI hold every single click for
   the double-click window (~0.3s of input latency).
4. **Resizing is preview-based**: drags update only
   `columnResizePreview`/`rowResizePreview` (observed solely by the guide-line
   overlays); the real widths mutate once on release.

Grid content **undo/redo** lives in `SheetModel.applyEdit` (grouped
`[CellChange]` steps, capped at 100; stacks clear on open/new) — route **any**
new mutation path through `applyEdit`, never `spreadsheet.setCell` directly, or
it won't be undoable. The log is history, not document state.

### Point mode

While a cell editor is open and its draft "expects an operand"
(`Calculator.expectsOperand`), clicking another cell inserts its reference
instead of committing (`SheetModel+PointMode.swift`). The draft lives in
`SheetModel.editingDraft` (not editor-local `@State`); focus-loss commits wait a
250ms grace window because mouseDown steals focus before the tap arrives. All
cell clicks route through `SheetModel.handleCellClick`.

## Persistence

- **Untitled** scratch work → `SheetModel.autosaveToScratch` keeps writing a
  full `Workbook` (cells + variables + functions + data types + layout) to
  `~/Library/Application Support/Soroban/` (the sandbox container). Backed by the
  engine's **snapshot + journal** WAL (`SheetModel+Persistence.swift` → the
  engine's `WorkbookJournal`; see [ENGINE.md](ENGINE.md#persistence)).
- **Named** file → changes only mark dirty; ⌘S writes. Variable/function/
  data-type changes dirty the workbook too, detected via
  `EvaluationEnvironment.changeCount` (compared around `submit()`).
- Quit with a dirty *named* workbook → Save/Discard/Cancel via
  `AppDelegate.applicationShouldTerminate` (`SorobanApp.swift`). Untitled needs
  no prompt (autosaved).
- The `.soroban` UTI + `CFBundleDocumentTypes` and the sandbox entitlements live
  in `project.yml` (regenerate after edits); Finder open arrives via
  `.onOpenURL`.

## Theming

`Theme` (`Theme/Theme.swift`) is Codable JSON (`#RRGGBB` colors). Ten built-ins
in `App/Resources/Themes/*.json` — but XcodeGen copies them **flat** into the
bundle root, so `ThemeManager` (`Theme/ThemeManager.swift`) scans both the root
and a `Themes/` subdir (don't "simplify" that away); user themes load from
`~/Library/Application Support/Soroban/Themes/` at launch. `Theme.fallback` is
the compiled-in safety net (keep it in sync with the JSON schema). Font
family/size are app-level overrides (`fontFamilyOverride`/`fontSizeOverride`,
UserDefaults-persisted) applied inside the computed `current`; the Settings
picker binds `currentName`, not `current`. Only **fixed-pitch** families are
offered — grid alignment and error-caret padding require monospace.

## Reflection & mutation glue

The engine's reflection handles are host-neutral (see
[ENGINE.md](ENGINE.md#reflection)); the app supplies the live sources:

- `SheetStore.logSource` is set to the `LogStore`, so `History` reflects the
  real tape (queried live).
- `SheetModel.installMutationOverride` **replaces** the engine's direct mutation
  default post-init, routing `updateCell`/`addWorksheet`/`renameWorksheet`/
  `deleteWorksheet` through `applyEdit`/`renameActiveSheet`/`removeActiveSheet`
  (undoable, persisted, UI-refreshing) — `SheetModel+Mutation.swift`.

## Inspector

`InspectorView.swift` is a trailing sidebar (240pt, both views, ⌥⌘0,
UserDefaults-persisted) listing every live name with value + provenance. Two
data sources merge per section: the **log** half on `CalculatorSession`
(`logVariables`/`logFunctions`/`logDataTypes`) and the **sheet** half on
`SheetModel+Inspector.swift` (scanning every non-data sheet's definitions/named
cells). Refresh rides both observation bridges —
`session.environmentGeneration` (log) and `sheet.generation` (sheet). Rows are
stateless (no per-row `@State` — the grid-perf rule) and click to jump
(`session.jumpTo`).

## Autocomplete

Candidate generation and word extraction are in the **engine**
(`Calculator.completions(forPrefix:)` / `trailingIdentifier(of:)`, both tested);
the app (`CalculatorSession+Autocomplete.swift` + `SuggestionsView.swift`)
handles only state and keys. ↑/↓ are overloaded — suggestion navigation while
the list is open, input history otherwise; preserve that ordering in
`InputBarView`. `suppressNextSuggestionRefresh` stops the list popping open after
a programmatic recall.

## Binary bit-editor

The Programmer-mode bit-editor overlay is the [BinaryEditorKit](KIT.md)
component. The app conforms to its `BinaryEditorHost` seam via
`CalculatorBinaryHost.swift` (presets + format persistence) and
`CalculatorSession+Binary.swift` (staging, `ans`-prefix, insertion). The overlay
is **Programmer-mode-only** (gated in `ContentView`, ⌥⌘B to toggle); flips stage
a live `binaryDraft` (no log spam), and **Use** *inserts* the value into the
input line (a `0b…` literal for a plain integer, the typed constructor for an
`Int…`) rather than posting to the log. A saved custom format persists as a
typed `Bits::BitFormat` workbook variable.

---

# Part 2 — Feature tour

The user-facing surface. Language behavior (numbers, functions, structured
values, `data` types, modes, fixed-width/decimal) is specified in
[../../docs/ANZAN.md](../../docs/ANZAN.md) and its companions — this tour covers
the app's UX and links out for the language.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| ⌘\\ | Toggle log ↔ grid (also the button right of the input bar / floating bottom-right in grid mode) |
| ⌘N / ⌘O / ⌘S / ⇧⌘S | New / Open / Save / Save As workbook |
| ⇧⌘O | Open CSV as a new workbook (File ▸ Export CSV… writes the current sheet's values) |
| ⌘/ | Function Reference — searchable docs with clickable, live-computed examples; with autocomplete open, jumps to the highlighted function |
| ⌘K | Clear the log |
| ⌘, | Settings (themes, font) |
| ⌥⌘0 | Toggle the environment inspector |
| ⌥⌘B | Toggle the binary bit-editor (Programmer mode) |
| Tab | Accept autocomplete suggestion (input bar) |
| ↑ / ↓ | Suggestions when open, input history otherwise (input bar); move selection (grid) |
| Return | Submit (input bar); edit selected cell / commit + move down (grid) |
| Esc | Dismiss suggestions, then clear line (input bar); cancel edit, then deselect (grid) |
| ⌘Z / ⇧⌘Z | Undo / redo grid edits — content, formatting, and control interactions each undo as their own steps |
| ⌘B / ⌘I / ⌘U / ⇧⌘X | Bold / italic / underline / strikethrough the selection (grid) |
| ⌃⌘. / ⌃⌘, | Increase / decrease decimals on the selection (grid) |
| Shift-click / Shift-arrows | Extend the selection rectangle from the anchor (grid) |
| ⌘C / ⌘X / ⌘V / Delete | Copy / cut / paste / clear the selection — clipboard is TSV (pastes to/from Excel/Numbers) |

## The log

Type an expression in the input bar, get a result; `ans` is the last one. See
[../../docs/ANZAN.md](../../docs/ANZAN.md) for the full language — numbers are
exact to 50 significant digits, functions are case-insensitive, structures
(strings/arrays/maps/records), lambdas, reductions (∑ ∏), dates, currency
(`$10`) and grouped (`138,561`) literals all work in the log and in cells
alike; the Programmer/Scientific modes apply to the log's input line.

- **Autocomplete** as you type (functions, your variables, constants): **Tab**
  accepts, **↑/↓** pick while the list is open, **Esc** dismisses.
- **↑/↓** recall input history (persisted across launches) when the list is
  closed; **Esc** clears the line.
- Log text is deliberately selectable plain `Text`; **recall/insert/copy** live
  in the **right-click** context menu (buttons would swallow text selection —
  keep it that way).
- `man pmt` / `manual pmt` / `help pmt` (no parentheses) print a function's
  docs into the log. **⌘/** opens the full searchable Function Reference
  (`ReferenceView.swift`); documentation is engine-enforced — a function can't
  be added without docs, and every example must evaluate.

## Grid view

A 26×1,000 mini-spreadsheet (columns A–Z), toggled with ⌘\. In grid mode the
input bar hides and the view toggle floats bottom-right. Reference cells as
`A:1` (column letter, colon, 1-based row) — in other cells *and* the log; the
sheet and log share one variable space, and cell evaluation never disturbs
`ans`.

Cells auto-detect their kind, with explicit markers when you want control:

| You type | The cell shows |
|---|---|
| `1200` | `1200` (number) |
| `Q1 revenue` | the text itself (labels never become errors) |
| `B:1 + B:2` | the computed value |
| `B:1 / 0` | `#ERR` (red highlight; hover for the message) |
| `=B:1 * rate` | **forced formula** — any failure, including a typo'd name, shows `#ERR` |
| `"123"` | **forced text** `123` (quotes stripped) — a label even though numeric |

Empty cells read as `0` in formulas; referencing a text cell is an error;
circular references are detected. **Single click selects**; arrows move,
⌘C/⌘X/⌘V copy/cut/paste raw contents, Delete clears, Return opens the editor.
**Double click edits.** While editing, Return commits + moves down, Tab moves
right, Esc cancels. **Point mode**: while editing a formula that expects an
operand, clicking another cell inserts its reference (`B:1 +`, click B:2 →
`B:1 + B:2`); shift-click makes a range. **Resize** by dragging a header edge (a
guide previews; applies on release; double-click a divider to reset). Layout
saves with the workbook. Copy/cut produce **TSV of raw contents**; an in-app
paste also adjusts relative references by the move offset (`$` pins hold).
**Fill Down/Right** (⌘D/⌘R) fills from the top/left of the selection.

### Formatting

Select cells and use the **Format menu** or **right-click** (Cut/Copy/Paste/
Delete plus formatting in a Format submenu — no toolbar):

- **Style**: Bold ⌘B · Italic ⌘I · Underline ⌘U · Strikethrough ⇧⌘X (all-set →
  toggle clears).
- **Alignment**: automatic (text left, numbers right) or forced.
- **Colors**: text and fill from a small palette that adapts to light/dark.
- **Number formats**: General · Number (`1,234,567.50`) · Currency (the symbol
  is stored, so a workbook renders the same everywhere) · Percent (exact) ·
  Date (day serials → `2026-06-06`) · Hex / Binary (`0xC3` / `0b1100_0011` —
  display-only radix; the value and every reference stay exact decimal) — plus
  Increase/Decrease Decimals (⌃⌘. / ⌃⌘,) and Clear Formatting.

Formatting is **display-only**: the underlying value stays exact, formulas and
TSV copy/paste see the raw value, formats save with the workbook, and empty
cells can be formatted.

### Sheet-scoped definitions (λ / 𝑖 / 𝑫 cells)

Type a definition *plainly* into a cell (no `=` marker) and it becomes part of
that sheet, not data on it:

| Cell content | Renders as | Meaning |
|---|---|---|
| `tax(x) = x * A:1  # …` | *λ tax(x)* | a function scoped to this sheet — bodies read cells |
| `rate = 0.0825` | *𝑖 rate* | a sheet variable — re-evaluates as referenced cells change |
| `data Pt { x: Number, y: Number }` | *𝑫 Pt* | a data type scoped to this sheet |

Formulas on the same sheet (and the log, while that sheet is active) use the
names directly. Each sheet is its own namespace. Cell-defined names are **owned
by their cells** (assigning one in the log says so and is refused), shadow
same-named log variables, and their trailing `# comment` is documentation
(`man tax` finds it). Details of the language semantics:
[../../docs/ANZAN.md](../../docs/ANZAN.md).

## Controls (sliders, steppers, checkboxes, dropdowns)

A cell whose expression is a *literal-argument* control call becomes interactive:

```
rate = slider(0.08, 0, 0.2)              # drag — live recalc
n = stepper(5, 1, 20)                    # − / + buttons
flag = checkbox(true)                    # click to toggle; evaluates to 1/0
region = dropdown("EU", ["EU", "US"])    # menu; the cell's value IS the selection
=slider(5, 0, 10)                        # anonymous form — read as the cell (A:1)
```

Interacting rewrites the value literal in the cell's own text as **one undoable
edit** (comments/spacing survive), and readers recalculate — sliders update live
mid-drag (drag = preview, release = commit, with targeted invalidation, never a
whole-workbook recalc per tick). Values display through the cell's number format.
Named controls are sheet-scoped 𝑖 definitions. The value argument *is* the
storage — non-literal args mean it's an ordinary formula.

## Named cells

Right-click a cell → **Name Cell…** (≤64 chars, unique per sheet). Formulas and
the log then read `'Projected Rate' * 12`, `Budget!'Projected Rate'`. Unqualified
names follow the same rule as `A:1` (a formula's own sheet, the active sheet from
the log). In point mode, clicking a named cell inserts its **name**. **Renaming
auto-updates** every reference; **removing** a referenced name asks break /
inline / cancel. Everything is undoable, in an order that always lands coherent,
and names save with the workbook. A name labels a *location*; a 𝑖 definition
names a *value*.

## Worksheets

A workbook holds up to **256 worksheets**. In grid mode a bottom strip
(`SheetTabBar.swift`) shows only the **active** tab — click its name for a menu,
**+** adds, **−** removes (with confirmation), **double-click renames** inline
(≤128 chars; no `!` or `'`). Formulas reference other sheets Excel-style, from
cells and the log:

```
Budget!A:1 * 2
sum('Q1 Budget'!B:1..B:12)
```

Unqualified references mean the sheet the formula lives on; in the log they
follow the active tab. Renaming a sheet **auto-rewrites** every qualified
reference. Undo jumps to the sheet where the edit happened.

## Inspecting the workbook (reflection)

A formula can read the workbook's own structure (read-only), and a small set of
**log-only** commands can change it. The object graph and flat accessors:

```
Workbook.count / Workbook.sheetNames / Workbook.worksheets[0].name
Workbook.worksheets["Budget"].cell("B", 1).value * 2
cell("A", 1).value   cell("Budget", "A", 1).value   sheetName()   rowCount()
```

Reads are **live** (`=cell("A",1).value + 1` recomputes when `A:1` changes). To
*change* the workbook, type a command in the **log** (not a cell — recalc must
stay reproducible); each is one undoable step:

```
updateCell(cell("A", 1), 99)            # a number, "=B:1*2" formula, or "" to clear
addWorksheet("Budget")   renameWorksheet("Budget", "Costs")   deleteWorksheet("Costs")
```

**`History`** is the whole tape — an array of entry handles you query from the
log (`last(History).value`, `filter(entry -> entry.isError, History)`); each
entry has `.input`/`.value`/`.text`/`.kind`/`.isError`/`.referencesCells`/`.note`.
It's read-only and log-only (in a cell it's just a text label). Since `e` is
Euler's number, name lambda parameters `entry`, not `e`. Full behavior:
[../../docs/ANZAN.md §10](../../docs/ANZAN.md).

## CSV & data sheets

Two doors:

- **File ▸ Open CSV…** (⇧⌘O) starts a **new, editable** workbook from a CSV —
  files that fit the grid arrive as ordinary cells (numbers detected), bigger
  ones become a **data sheet** automatically. Either way it's a **copy**; the
  source `.csv` is never written back (edits save into the `.soroban` file).
- **File ▸ Export CSV…** writes the current sheet's computed **values** (numbers
  plain, controls as their value, λ/𝑖 cells as their raw source, errors as
  `#ERR`).

Data sheets are backed by the package's SQLite store, read lazily (100,000-row
imports neither slow opens nor bloat the file), and can exceed the grid's 1,000
rows (`sum(sales!C:2..C:50000)` works); the grid browses the first 10,000. They
are editable copies; *linked* (live read-only) sources are on the roadmap. Try
`../../examples/sales.csv`, then `sum(sales!C:2..C:7)`.

## Workbooks

Save your whole session — grid cells, variables, functions (with doc comments),
data types, and layout — as a `.soroban` file (⌘S / ⇧⌘S / ⌘O / ⌘N). The window
title shows the workbook and an "— Edited" marker; quitting unsaved prompts.
Untitled scratch work auto-persists across launches. On disk a workbook is a
**package** (`workbook.json` + `data.sqlite` when data sheets exist) — schema in
[../../docs/FORMAT.md](../../docs/FORMAT.md). A worked example lives at
[../../examples/mortgage.soroban](../../examples/mortgage.soroban).

## Themes

Pick a theme in Settings (⌘,) — ten ship built-in (six dark, four light). Drop
your own JSON into `~/Library/Application Support/Soroban/Themes/` (restart to
load). Settings also has app-level **font family and size** controls (monospaced
only — column alignment depends on fixed pitch); they override the active
theme's font and survive theme switches.

## See also

- [ARCHITECTURE.md](ARCHITECTURE.md) — the ecosystem map · [ENGINE.md](ENGINE.md)
  — the engine under the app · [KIT.md](KIT.md) — the bit-editor component.
- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language ·
  [../../docs/FORMAT.md](../../docs/FORMAT.md) — the `.soroban` schema ·
  [../CLAUDE.md](../CLAUDE.md) — agent guide.
