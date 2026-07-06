# CLAUDE.md — Swift ecosystem

Agent guidance for working under `swift/`. Loaded automatically alongside the
root [../CLAUDE.md](../CLAUDE.md) when you touch files here. This file is the
Swift-specific architecture invariants and conventions; the prose walkthroughs
live in [docs/](docs/).

## Orientation

- **Build/test/run**: [README.md](README.md).
- **Architecture**: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (the map) and
  the deep dives — [docs/ENGINE.md](docs/ENGINE.md),
  [docs/APP.md](docs/APP.md), [docs/CLI.md](docs/CLI.md),
  [docs/KIT.md](docs/KIT.md).
- **The language** (shared, don't re-explain in ecosystem docs):
  [../docs/ANZAN.md](../docs/ANZAN.md) and its companions
  ([MODES](../docs/MODES.md), [MODULES](../docs/MODULES.md),
  [FIXED-WIDTH](../docs/FIXED-WIDTH.md), [DECIMAL](../docs/DECIMAL.md),
  [PROGRAMMER](../docs/PROGRAMMER.md), [FORMAT](../docs/FORMAT.md)).

## Where things live

Three build products (details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)):

- `Engine/Sources/Anzan/` — the language (BigInt-only). `Lexer/`, `Parser/`,
  `Number/`, `Eval/`, `Functions/`, `Calculator.swift`.
- `Engine/Sources/SorobanEngine/` — the hosting layer. `Sheet/`, `Persistence/`;
  `@_exported import Anzan` in `Exports.swift`.
- `Engine/Sources/SorobanCLI/main.swift` — the `soroban` executable (Anzan only).
- `App/` — the SwiftUI app. Model layer in `App/Sources/Session/` (`SheetModel/`,
  `CalculatorSession`, `LogStore`, `WorkbookManager`); views at `App/Sources/`.
- `Kit/Sources/BinaryEditorKit/` — the shared bit-editor UI.

A recent refactor split several files into extensions — the split ones are
`Parser` (`+Definitions`/`+Expressions`/`+Primary`), `Evaluator`
(`+Namespaces`/`+Literals`/`+Calls`/`+Operators`/`+Recursion`), `Spreadsheet`
(`+Evaluation`), `SheetStore` (`+Mutation`/`+Structure`), `CalculatorSession`
(`+Binary`/`+Autocomplete`), `SheetModel` (many `+*`), and the Kit's
`BinaryEditorView` (`+Header`/`+Builder`/`+Grid`). When citing a symbol, verify
its current file; don't trust older single-file names.

## Non-negotiables

- **Trust `swift test` / `xcodebuild` output** over SourceKit's phantom "No such
  module" / "Cannot find type" errors.
- **`Soroban.xcodeproj` is generated** — run `xcodegen generate` after
  adding/removing files under `App/` or editing `project.yml`.
- **The module boundary is enforced**: don't add a `Sheet/`/`Persistence/`
  import to `Anzan`. `Calculator.restoreSession(from:)` lives in
  `SorobanEngine/Persistence/Calculator+Workbook.swift` for this reason.
- **BigDecimal exactness**: `+ − ×`, integer `^`, `%` exact; `/` and `sqrt`
  round to `PrecisionContext.current`. Route any inexact function through
  `BigDecimal.viaDouble(...)` in `Number/BigDecimal+Math.swift` — never `Double`
  math elsewhere.
- **`evaluate` vs `evaluateFormula`**: the log path updates `ans`; the cell path
  never does, rejects assignments/definitions, and runs with mutation disabled
  (recalc must stay reproducible — the `rand()` principle). Keep the asymmetry.
- **Dependency-graph recalc**: `setCell` invalidates only the transitive reader
  closure; log/definition/rename/load changes call `invalidateEverything`.
  Don't revert `setCell` to whole-memo clearing (cross-sheet dependents go
  stale).
- **Cells parse once** (at commit, in `Cell.init`); recalc evaluates stored ASTs.
  Don't reintroduce string re-parsing into the recalc path.
- **Adding a builtin requires docs** — `category`/`signature`/`summary`/
  `examples` are required init params; `DocumentationTests` runs every example.
  Manual `Documentation.swift` entry for special forms/operators/constants.
- **Preserve `EngineError` character offsets** through the lexer/parser (the
  caret rendering depends on them).

## App-layer rules

- **Grid render performance** ([docs/APP.md](docs/APP.md)): `CellView` must not
  read the observable model (it's `Equatable`, fed `Equatable` lets by
  `GridRowView`); no per-cell `@State`/`@FocusState`; double-tap is a
  `simultaneousGesture`; resizing is preview-based. **Profile on Release.**
- **Route mutations through `SheetModel.applyEdit`**, never
  `spreadsheet.setCell` directly, or the edit won't be undoable.
- Anything that changes cell values must call `sheet.recalculate()` —
  `SheetModel.generation` is the observation bridge for the non-`@Observable`
  `Spreadsheet`.
- Views read colors/fonts from `ThemeManager.current` — never hardcode.

## Testing

- **Engine** (Swift Testing, the main loop): `cd Engine && swift test`. Coverage
  kept ≥ ~92% regions.
- **Shared spec** ([../spec/README.md](../spec/README.md)) runs twice through
  PickleKit: `Engine` `GherkinTests` (language, `spec/anzan/`) and the app's
  `SorobanSessionTests` target (session layer, `spec/session/`). Both reach the
  features via **symlinks** — edit features in `spec/`, never a copy. A behavior
  change lands as a feature-file edit plus implementation; scenarios one
  ecosystem hasn't caught up to get tagged (`@swift-only` / `@rust-pending`).
- **Division of truth**: user-visible input→output behavior lives in feature
  files; unit suites keep only what scenarios can't express (typed-error
  equality with positions, codec round-trips, dependency-graph invalidation,
  recursion/stack canaries).
- PickleKit needs **Swift 6.2+** (CI: `macos-26`, Xcode 26.2); its revision is
  pinned in **both** `Engine/Package.swift` and `project.yml` — keep them in
  sync when bumping.

## Conventions

- Errors are always `EngineError`. Swift 6 strict concurrency is on — globals
  must be `Sendable`, function closures `@Sendable`.
- Finance golden values are spreadsheet-checked; re-derive, don't "fix" them to
  match changed code.
- Tests that wire cell resolvers must keep the `Spreadsheet` alive — the
  closures capture it weakly.
