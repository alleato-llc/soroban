# The `soroban-engine` crate (hosting layer)

The HOSTING layer over the [`anzan`](ANZAN.md) language: the spreadsheet model
(sheets, cells, the dependency graph, controls, named cells, worksheets),
`.soroban` persistence, and workbook/history reflection + mutation. `lib.rs`
opens with `pub use anzan::*;` (Swift's `@_exported import Anzan`), so depending
on `soroban-engine` gives the **whole** engine — apps never depend on `anzan`
directly, and Anzan gains no grid/file dependency in return.

> This documents the CRATE structure. The behaviors — cell classification, the
> dependency-graph recalc, reflection, controls, named cells — are shared with
> the Swift `SorobanEngine` and specified in the root
> [../../CLAUDE.md](../../CLAUDE.md) Architecture section and the shared docs. The
> `.soroban` schema is the interchange contract in
> [../../docs/FORMAT.md](../../docs/FORMAT.md). This is the port of
> `swift/Engine/Sources/SorobanEngine`.

## Public surface

`lib.rs` re-exports: `Cell`, `CellAddress`, `CellFormat` (+ `CellAlignment`,
`NumberFormat`, `PaletteColor`), `Control` (+ `SliderInfo`/`CheckboxInfo`/
`DropdownInfo`), `DataSheet`/`DataStore`, `Sheet`/`SheetStore`, `Spreadsheet`
(+ `CellDisplay`), `StructuralChange`/`CellRewrite`, `Workbook`. All modules are
`pub` except `reflection`, which is `pub(crate)` (the language navigates its
`.host` handles by trait, so the types need not be public).

## Module map

### The calculation model

- `cell.rs` — `Cell`: one cell's content, **statically** classified once at
  commit (`=…` forces formula, `"…"` forces text, else a stored candidate AST).
  The *dynamic* half (does a candidate evaluate to a value or degrade to a label)
  is per-recalc in `spreadsheet/evaluation.rs`, because it depends on the live
  environment.
- `cell_address.rs` — `CellAddress`: the single home for every name↔index and
  0-vs-1-based `"A:1"` conversion. Don't re-implement column-letter parsing
  elsewhere.
- `context.rs` — the per-store shared context: the **owning-sheet stack** (an
  unqualified `A:1` in a formula resolves against the sheet that owns the
  formula, not the active tab) and cross-sheet **cycle detection** (keyed by
  (sheet identity, address)). Interior-mutable throughout (`RefCell` with short
  borrows) — evaluation re-enters it recursively.
- `spreadsheet.rs` — `Spreadsheet`: sparse raw contents + memoized evaluation
  with formula auto-detection. The classification rules (blank/text/formula and
  the unknown-name→label carve-out) are in its `//!`. Evaluation methods take
  `(&Evaluator, &mut EvaluationEnvironment)` — the re-entry context the resolvers
  thread (the Rust answer to Swift's shared-class `Calculator` re-entrancy).
  Split: `spreadsheet/definitions.rs` (named cells, sheet-scoped λ/𝑖/𝑫
  definitions, live slider-preview overrides) and `spreadsheet/evaluation.rs`
  (dynamic classification, cycle-safe memoized `display_value`, control
  rendering, the numeric read paths).
- `controls.rs` — `Control`: a cell whose expression is a literal-argument
  control call renders as a slider/stepper/checkbox/dropdown; interaction
  rewrites the storage literal in place (token-precise). The builtins stay pure,
  so workbooks behave identically headlessly.
- `cell_format.rs` — `CellFormat`: per-cell display-only style + number format.
  The value stays exact; rendering is pure string/BigInt math (never f64 or a
  locale formatter). Stored sparsely on `Sheet.formats`.

### Worksheets & resolver wiring

- `sheet_store.rs` — `SheetStore`: an ordered collection of named `Sheet`s
  sharing one `Calculator`. Holds it in `Rc<RefCell<…>>`; the resolver closures
  installed INTO the calculator capture a `Weak` of the store's internals and
  receive `(&Evaluator, &mut EvaluationEnvironment)` as arguments, so they never
  borrow the Calculator `RefCell` (the only borrow is the host's outermost call).
  Split: `sheet_store/resolvers.rs` (installs the cell/range/name reference
  closures, the sheet-scoped-definition closures, the read-only Workbook/History
  reflection API, and the default direct mutation commands) and
  `sheet_store/mutation.rs` (the log-only `updateCell`/`addWorksheet`/
  `renameWorksheet`/`deleteWorksheet` DIRECT no-undo default; worksheet targets
  resolve to an index here).

### Reference rewriting & structural edits

- `reference_rewriter.rs` — token-precise rewriting of cell references inside raw
  text (lex, collect ranges, splice back-to-front so spacing/`# comments`
  survive). Behind structural edits, fill/paste adjustment, and sheet renames.
  Deliberately IGNORES compact map keys (`{b:1}`) and multi-letter columns
  (named-arg sugar). Returns `None` when nothing matched.
- `named_cells.rs` — the same technique for named cells: rename rewrites every
  referencing formula; delete offers to inline the address.
- `structure.rs` — insert/delete rows & columns. One op = two recorded effects
  (raw rewrites across all sheets at PRE-move addresses + a content move on the
  edited sheet); `StructuralChange::revert` is the exact inverse, redo
  re-executes. (Data sheets aren't wired into `Sheet` yet, so Swift's `isData`
  refusal has no counterpart.)

### Reflection (read-only + log-only mutation)

- `reflection.rs` (`pub(crate)`) — the read-only Workbook reflection graph. A
  `Workbook` global + flat `cell()`/`sheetNames()`/… hand the language opaque
  `.host` handles (`HostObject` from `anzan`) it navigates uniformly. Handles
  hold the store/sheet/grid **weakly** (a stored handle never keeps a removed
  sheet alive; reads after teardown throw cleanly). Cell reads route through the
  ordinary numeric/display path via the re-entry pair, so dependency edges and
  cycle detection come for free.
- `history_reflection.rs` — the read-only `History` array of log-entry handles.
  **Log-only**: the resolver returns the array only on the log path (`in_log`);
  in a cell the name is unknown and degrades to a text label (reproducibility —
  the log is global session state, not the workbook). The host feeds the log via
  a `LogSource` trait (host-neutral `LogRecord`s); each entry's `kind`/
  `references_cells` is DERIVED by parsing the stored input.

### Persistence

- `workbook.rs` — `Workbook`: the versioned JSON envelope (cells + variables +
  functions + data types + layout) plus the `restore_session` logic (types →
  functions → variables — order is load-bearing). Field names/nesting/decode
  defaults mirror Swift's `Codable` exactly — the interchange contract
  ([../../docs/FORMAT.md](../../docs/FORMAT.md)). serde/serde_json driven.
- `package.rs` — the `.soroban` document package on disk: a directory of
  `workbook.json` (+ `data.sqlite` only when data sheets exist). Legacy flat
  JSON reads transparently; saves always write the package shape.
- `journal.rs` — the scratch-persistence WAL: cell edits append one JSON line
  each (O(1)); a periodic snapshot compacts. Replay is order-preserving and
  idempotent (entries are absolute values), so snapshot-then-truncate is
  crash-safe. Interchange files never carry a journal.
- `data_store.rs` — `DataStore`/`DataSheet`: SQLite (via bundled `rusqlite`,
  playing the role Swift's system-SQLite link plays) backing data sheets. Values
  read lazily, so opening a workbook never loads tables into memory.
- `csv.rs` — minimal RFC 4180 CSV; `encode` is `parse`'s exact inverse
  (`parse(&encode(rows)) == rows`).

## Tests

Sibling `tests.rs` files for the leaf modules (`cell_format`, `csv`, `data_store`,
`journal`, `named_cells`, `package`, `reference_rewriter`, `structure`,
`workbook`). Integration tests in `engine/tests/`: `gherkin.rs` (the shared
`spec/anzan` parity oracle, cucumber, `harness = false`), plus `reflection`,
`mutation`, `history_reflection`, `sheet_graph`, `workbook_edges`,
`binary_format`, `documentation`, and `interchange` (round-trips the repo-root
`examples/*.soroban` authored by BOTH ecosystems). `examples/author_interchange.rs`
regenerates the Rust-authored `interchange.soroban`.

## See also

- [../../docs/FORMAT.md](../../docs/FORMAT.md) — the `.soroban` interchange format.
- [ANZAN.md](ANZAN.md) — the language crate · [ARCHITECTURE.md](ARCHITECTURE.md) ·
  [GUI.md](GUI.md) — the app that hosts this crate.
