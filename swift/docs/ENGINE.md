# The Swift engine — Anzan + SorobanEngine

The `Engine` SwiftPM package: the `Anzan` language module and the
`SorobanEngine` hosting module. This page is the implementation map — module
layout, load-bearing invariants, and the seams. For what the language *means*
(grammar, functions, types), read [../../docs/ANZAN.md](../../docs/ANZAN.md)
and its companions; this doc does not restate language semantics.

## Module layout

```
Engine/Sources/
├── Anzan/                       # the language — BigInt-only, host-agnostic
│   ├── Calculator.swift         # the façade
│   ├── Documentation.swift  EngineError.swift  LanguageMode.swift
│   ├── Lexer/                   # Lexer.swift, Token.swift
│   ├── Parser/                  # Parser.swift (+ +Definitions/+Expressions/
│   │                            #   +Primary), AST.swift, Expression+Source.swift
│   ├── Number/                  # BigDecimal.swift, BigDecimal+Math.swift
│   ├── Eval/                    # Evaluator (+ +Namespaces/+Literals/+Calls/
│   │                            #   +Operators/+Recursion), Value, Environment,
│   │                            #   FixedInt, FixedDecimal, DataType, JSON,
│   │                            #   BinaryView (+ +Builder), BinaryFormat,
│   │                            #   FunctionRegistry
│   └── Functions/               # core/logic-in-core/trig/finance/dates/
│                                #   accounting/stats/data/programmer/controls
└── SorobanEngine/               # the hosting layer — @_exported import Anzan
    ├── Exports.swift            # the re-export
    ├── Sheet/                   # Spreadsheet (+ +Evaluation), Cell, CellAddress,
    │                            #   CellFormat, Controls, NamedCells,
    │                            #   ReferenceRewriter, SheetStore (+ +Mutation/
    │                            #   +Structure), WorkbookReflection,
    │                            #   HistoryReflection
    └── Persistence/             # Workbook, WorkbookJournal, WorkbookPackage,
                                 #   DataStore, Calculator+Workbook
```

The boundary is enforced by the package graph: **`Anzan` must never import
`Sheet/` or `Persistence/`.** `Calculator.restoreSession(from: Workbook)` lives
in `SorobanEngine/Persistence/Calculator+Workbook.swift` for exactly this
reason. Cross-module internals use the `package` access level.

## Anzan

### BigDecimal — the core invariant

`BigDecimal` (`Number/BigDecimal.swift`) = `BigInt` significand × 10^exponent,
always normalized (no trailing zeros). `+ − ×`, integer `^`, and `%` are
**exact**; `/` and `sqrt` round to `PrecisionContext.current` significant
digits (default 50, banker's rounding). Transcendentals (`exp`, `ln`, `log`,
trig, non-integer `pow`) round-trip through `Double` (~15 digits) — that
fallback is **deliberately confined to `BigDecimal.viaDouble(...)` in
`Number/BigDecimal+Math.swift`** (also used by `Functions/CoreFunctions.swift`
for `solve`). Route any new inexact function through that seam so a future
arbitrary-precision upgrade happens in one place. Don't introduce `Double` math
anywhere else in the engine.

### Value — the engine's value type

Every expression evaluates to a `Value` (`Eval/Value.swift`): `.number` /
`.string` / `.array` / `.map` / `.record` / `.fixedInt` / `.fixedDecimal` /
`.function` / `.host`. Structures nest freely, are **immutable** (rebind the
variable — no element assignment), and render canonically (`description`
re-parses to an equal value — that's how structured variables persist in
workbooks). Maps keep insertion order; `==` is deep and order-insensitive for
maps. `+` concatenates when either side is a string; every other operator is
numeric. Indexing is **0-based**. Adding a `Value` case touches every
exhaustive switch (Value, Evaluator, JSON, DataFunctions, Spreadsheet display)
— the compiler finds them.

The scalar payloads have their own spec docs; the engine files that back them:

| Value case | File(s) | Spec |
|---|---|---|
| `.fixedInt` | `Eval/FixedInt.swift`, `Functions/ProgrammerFunctions.swift` | [FIXED-WIDTH.md](../../docs/FIXED-WIDTH.md) |
| `.fixedDecimal` | `Eval/FixedDecimal.swift`, `Functions/AccountingFunctions.swift` | [DECIMAL.md](../../docs/DECIMAL.md) |
| `.record` (`data`) | `Eval/DataType.swift`, `Eval/JSON.swift` | [ANZAN.md §7](../../docs/ANZAN.md) |
| `.host` (reflection) | see [reflection](#reflection) | [ANZAN.md §10](../../docs/ANZAN.md) |

The bounded numeric types are **checked, not modular** — construction
range-validates and arithmetic overflow throws. Both are intercepted in
`Evaluator.apply` (`Eval/Evaluator+Operators.swift`) before the generic numeric
path, then coerce back to `.number` outside typed arithmetic.

### Lexer & Parser

`Lexer/Lexer.swift` + `Lexer/Token.swift`. Lex/parse errors carry character
offsets that hosts render as a caret; **preserve positions** when editing.

The Pratt parser was split by concern:

| File | Owns |
|---|---|
| `Parser/Parser.swift` | the core recursive-descent driver, `ReservedNames`, precedence |
| `Parser/Parser+Expressions.swift` | `comparison()`/`additive()`/`term()`/`power()`/`postfix()`, lambdas, reductions |
| `Parser/Parser+Primary.swift` | `primary()`, literals, map/array literals, argument lists |
| `Parser/Parser+Definitions.swift` | `functionDefinition()`, `dataDefinition()`, `man`/`help` special forms |
| `Parser/AST.swift` | the `Expression` node type |
| `Parser/Expression+Source.swift` | `sourceText(mode:)` — re-parseable rendering (the persistence contract) |

`ReservedNames` (defined across the `Parser` files) holds `ans`/`pi`/`tau`/`e`/
`true`/`false`/`man`/`manual`/`help`/`sigma` — the parser rejects assignment to
them. The parser is **mode-parameterized** (`Parser.parse(_, mode:)`) and the
renderer too (`sourceText(mode:)`) — but only `.normal` is ever stored, and it
must stay byte-identical to the pre-modes grammar. See
[MODES.md](../../docs/MODES.md); `LanguageMode.swift` holds the dialect enum.

### Evaluator

`Eval/Evaluator.swift` is the driver; the big value-producing `switch` is split
across extensions so a debug build gives the switch **one frame** holding every
case's locals — keep new heavyweight cases extracted:

| File | Owns |
|---|---|
| `Evaluator.swift` | the `evaluate` entry, the `switch`, `locals` threading |
| `Evaluator+Namespaces.swift` | variable/name resolution order, scoped resolvers |
| `Evaluator+Literals.swift` | array/map/record literals, reductions |
| `Evaluator+Calls.swift` | function calls, `call(name:)`, `apply(user:)`, `construct` |
| `Evaluator+Operators.swift` | `apply` (binary ops), the FixedInt/FixedDecimal hooks |
| `Evaluator+Recursion.swift` | tail-call optimization, stack segmentation |

**Recursion is bounded by memory, not a counter.** Tail calls loop at constant
stack (`apply(user:)`; `tailStep` walks conditionals — keep its resolution
order in sync with `call(name:)`); non-tail recursion continues on a fresh
16 MB thread segment when the current stack runs low
(`continueOnFreshStack`/`nearStackLimit`), the caller blocking so the
single-threaded discipline holds. `maxTailIterations` (~1M) and `maxCallDepth`
(10,000) are the runaway sanity caps; both errors hint at a missing base case.
Slim frames still reduce segment hops, so keep fat cases extracted.

`EvaluationEnvironment` (`Eval/Environment.swift`) is a **class** — formula
evaluation re-enters the calculator through a resolver, and threading a struct
`inout` caused an exclusivity crash. It also holds `Constants` (the reserved
`Json`/`Rounding` maps) and `changeCount` (the host's dirty-tracking seam). The
name (not `Environment`) avoids colliding with SwiftUI's `@Environment`.

### Functions & the registry

`Eval/FunctionRegistry.swift` merges the per-domain lists in `Functions/`
(`CoreFunctions` — which also holds logic and trig — `FinanceFunctions`,
`DateFunctions`, `AccountingFunctions`, `StatsFunctions`, `DataFunctions`,
`ProgrammerFunctions`, `ControlFunctions`) into `FunctionRegistry.standard`.
Names are case-insensitive and unique across all lists (asserted at startup).

**Adding a built-in** (the language-vs-library gate is in
[ANZAN.md](../../docs/ANZAN.md) "Design rules"): add a `BuiltinFunction` entry
to the right list. `category`/`signature`/`summary`/`examples` are **required**
— `DocumentationTests` evaluates every example, so an undocumented function
won't compile and a broken example fails CI. Special forms/operators/constants
need a manual entry in `Documentation.swift`. Three implementation kinds:

- **`apply:`** — numeric; array args flatten recursively, arity checked *after*
  flattening.
- **`applyValues:`** — structure-aware (`len`/`first`/`keys`/`concat`…); sees
  raw `[Value]`, arity on the raw count.
- **`applyHigherOrder:`** — additionally receives an `applier` callback
  (map/filter/reduce); the evaluator passes it since only the evaluator owns
  the environment and depth budget. Functions stay pure — the applier is the
  one sanctioned hole.

### The Calculator façade

`Calculator.swift` owns the `EvaluationEnvironment` and exposes
`evaluate(String) -> Result<EvalOutcome, EngineError>` (log path — updates
`ans`) vs. `evaluateFormula(_:)` (cell path — never touches `ans`, rejects
assignments/definitions, runs with mutation disabled). **Keep that asymmetry.**
Hosts wire the language to a grid through resolver closures on the Calculator —
all `nil` in the CLI:

| Resolver | Purpose |
|---|---|
| `cellResolver` / `rangeResolver` / `nameResolver` | `A:1`, `A:1..B:9`, `'Named'` reads |
| `hostValueResolver` | a bare name → a host value (`Workbook`/`History`); `inLog` gates log-only names |
| `hostFunctionResolver` | a free call → a reflection function (checked last, so user defs shadow) |
| `hostMutationResolver` | a log-only mutation (`updateCell`…); the `inLog` flag is `false` on the cell path |
| `scopedFunctionResolver` / `scopedVariableResolver` / `scopedDataTypeResolver` | sheet-scoped λ/𝑖/𝑫 definitions |

## SorobanEngine — the hosting layer

### Spreadsheet & cell classification

`Sheet/Spreadsheet.swift` + `Sheet/Spreadsheet+Evaluation.swift`. Cell
classification is split deliberately:

- **Static** facts in `Cell.init` (`Sheet/Cell.swift`), computed once at commit:
  `=…` forces formula (parse errors → `#ERR`); `"…"` forces text; unparseable →
  plain text; else a stored AST as a `.candidate`. **Cells are parsed once** —
  recalc evaluates stored ASTs via `Calculator.evaluateFormula`; don't
  reintroduce string re-parsing into recalc.
- **Dynamic** facts in `Spreadsheet.evaluate(_:)`, per recalc: candidates that
  evaluate are values; on failure the **error kind** decides —
  unknownVariable/unknownFunction → text (protects labels like `Q1 revenue`),
  any other failure or any cell-referencing expression → `#ERR`. These rules
  are load-bearing UX; don't change them casually.

`Sheet/CellAddress.swift` owns **all** `"A:1"` key / column-name / 0-vs-1-based
conversions — don't re-implement them elsewhere.

### Dependency-graph recalc

Evaluation records read edges in a `ResolutionContext` (per-cell `dependents` +
per-sheet `rangeDependents` rects). `setCell` invalidates only the **transitive
reader closure** across sheets (`context.invalidate`); log variable/function
changes, renames, removes, and loads call `invalidateEverything`. Edges may be
stale (safe — over-invalidation only). **Don't revert `setCell` to
whole-memo clearing** — cross-sheet dependents went stale that way. Cycle
detection is context-wide, keyed by (sheet identity, address).

### References & rewriting

`A:1` is a token lexed only when a letters-only identifier is followed by
`:digits` (with optional `$A:$1` pins — copy-time data only; `$A:$1` ≡ `A:1` to
the AST). Token-precise rewriting lives in `Sheet/ReferenceRewriter.swift`
(structural shifts, relative fill/paste adjustment, sheet renames) — it skips
compact map keys and named-arg columns, pairs range corners only across a real
`..`, and splices `refError()` over references killed by a delete.

### Formats, controls, named cells

- **`Sheet/CellFormat.swift`** — style/alignment/`PaletteColor`/`NumberFormat`.
  Rendering is **pure string/BigInt math** (never Double/NumberFormatter, so
  40-digit values group exactly); **display-only** (formats live in
  `Sheet.formats`, never touch `Cell`/the dependency graph/recalc); colors are
  semantic names the app maps to system colors. Hex/binary formats are
  display-only radix — never a semantics switch.
- **`Sheet/Controls.swift`** — `Control.display(for:)` detects
  `slider`/`stepper`/`checkbox`/`dropdown` cells; the value argument **is** the
  storage, so non-literal args mean it's not a control. Interaction rewrites the
  literal via `Control.rewriting`.
- **`Sheet/NamedCells.swift`** — `'Name'` labels a cell *location*
  (`Spreadsheet.cellNames`, ≤64 chars, unique per sheet). Rename auto-rewrites,
  delete asks break/inline/cancel, both via `NamedCells.rewriting`.

### Worksheets & sheet-scoped definitions

`Sheet/SheetStore.swift` (+ `+Mutation`, `+Structure`) owns the ordered named
`Sheet`s, the 256 cap, name validation, and the **owning-sheet invariant**: an
unqualified `A:1` in a formula resolves against the sheet that *owns* the
formula (current-sheet stack in the shared `ResolutionContext`); only log input
follows the active tab. References are by name; renaming auto-rewrites
qualifiers via `ReferenceRewriter.renamingSheet`. Data sheets
(`Sheet.data: DataSheet?`) are backed by `Persistence/DataStore.swift` (SQLite),
bounded by the table not `rowCount`.

A plain `f(x)=…` / `x=expr` / `data …{}` cell (no `=` marker) classifies as a
sheet-scoped **λ/𝑖/𝑫** definition — `Spreadsheet.definitions` (rebuilt by full
scan on any definition edit), resolved via the scoped resolvers, shadowing log
globals. See [ANZAN.md](../../docs/ANZAN.md) for the user-facing behavior;
[APP.md](APP.md) for the grid rendering.

### Reflection

`Value.host(any HostObject)` (in `Anzan/Eval/Value.swift`) is the escape hatch;
the concrete handles are **host-side**:

- `Sheet/WorkbookReflection.swift` — `WorkbookObject`/`WorksheetCollection`/
  `WorksheetObject`/`CellObject` (read-only object graph + flat accessors).
- `Sheet/HistoryReflection.swift` — `HistoryEntryObject` over a host-neutral
  `LogRecord` fed by the `LogSource` protocol (the app's `LogStore` conforms).
  `History` resolves to an **array** of handles (so `len`/`[i]`/`map` work
  natively).

Handles hold the store/sheet **weakly** and are `@unchecked Sendable` on the
single-threaded discipline; reads route through the ordinary value path, so
dependency edges and cycle detection come for free — **don't snapshot instead.**
Mutation (`updateCell`/`addWorksheet`/`renameWorksheet`/`deleteWorksheet`) is
**log-only** (`hostMutationResolver`'s `inLog` gate — a cell recalc must stay
reproducible, the `rand()` principle). `SheetStore.installMutation` wires the
direct default; the app's `SheetModel.installMutationOverride` re-routes it
through undoable edits.

## Persistence

- **`Persistence/Workbook.swift`** — the `.soroban` JSON codec: versioned
  envelope of `cells` + `variables` + `functions` + `dataTypes` + layout.
  Newer fields decode with defaults; decode rejects future versions. Schema:
  [FORMAT.md](../../docs/FORMAT.md). Open goes through
  `Calculator.restoreSession(from:)` (`Persistence/Calculator+Workbook.swift`),
  which restores **types → functions → variables** — order is load-bearing,
  because record variables persist as constructor calls that must evaluate.
- **`Persistence/WorkbookJournal.swift`** — the scratch WAL: one JSON line per
  cell edit (O(1), written immediately); structural changes + 256-entry growth
  compact (snapshot then truncate — replay is idempotent, so a crash between is
  safe). `.soroban` interchange files never contain a journal.
- **`Persistence/WorkbookPackage.swift`** — the `.soroban` document *package*
  (directory: `workbook.json` + optional `data.sqlite`; legacy flat JSON reads
  transparently; writes are atomic temp-dir swaps).
- **`Persistence/DataStore.swift`** — the SQLite store behind data sheets.

## Engine conventions

- Errors are always `EngineError`; preserve lex/parse character offsets.
- Finance functions follow the spreadsheet sign convention; `rate`/`irr`/`solve`
  solve numerically (Newton + bisection) in the Double domain. Golden values in
  `FinanceTests`/`spec` are spreadsheet-checked — re-derive, don't "fix" them.
- Swift 6 strict concurrency is on: globals (function lists, registry) must be
  `Sendable`; function closures are `@Sendable`.
- Implicit multiplication (`2x`, `2(3+4)`, `2 A:1`) lives in the `term()` loop;
  a bare number as the right operand throws "a number can't directly follow
  another value". New token kinds that can start an operand need a case there.

## See also

- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language spec these modules
  implement, and its companions ([MODES](../../docs/MODES.md),
  [MODULES](../../docs/MODULES.md), [FIXED-WIDTH](../../docs/FIXED-WIDTH.md),
  [DECIMAL](../../docs/DECIMAL.md), [PROGRAMMER](../../docs/PROGRAMMER.md)).
- [../../docs/FORMAT.md](../../docs/FORMAT.md) — the `.soroban` schema.
- [ARCHITECTURE.md](ARCHITECTURE.md) · [APP.md](APP.md) · [CLI.md](CLI.md) ·
  [KIT.md](KIT.md) · [../CLAUDE.md](../CLAUDE.md).
