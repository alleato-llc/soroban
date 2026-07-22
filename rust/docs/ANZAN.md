# The `anzan` crate (language implementation)

The Rust implementation of the Anzan language: `lexer` → `parser` → `eval`
(evaluator + value + function library) + `number` (the exact `BigDecimal`),
fronted by the `Calculator` facade. Knows **nothing** about grids or files —
hosts wire cells, reflection, and mutation in through resolver closures.

**Embedding** is `Calculator` + `script::StatementAccumulator` (splits
multi-line source into logical statements — the same primitive behind `.anzan`
files, statement-aware pipes, and REPL continuation): split, then `evaluate`
each statement. The CLI (`rust/cli`) is a two-file demonstration of exactly
that surface. The crate is consumable as a cargo **git dependency** by package
name (`anzan = { git = "…" }`); it is not on crates.io.

> **This is a doc about the CRATE**, not the language. For the language itself —
> grammar, precedence, the number lexicon, functions, `data` types, reflection —
> read the shared spec [../../docs/ANZAN.md](../../docs/ANZAN.md) and its
> companions ([MODES](../../docs/MODES.md), [MODULES](../../docs/MODULES.md),
> [FIXED-WIDTH](../../docs/FIXED-WIDTH.md), [DECIMAL](../../docs/DECIMAL.md),
> [PROGRAMMER](../../docs/PROGRAMMER.md), [STDLIB](../../docs/STDLIB.md)). This
> page describes how the crate is *structured*. The Swift `Anzan` module is the
> reference implementation; behavior changes land in `spec/` first, then both.

## Crate root

`src/lib.rs` declares the modules and re-exports the public surface. The pipeline
modules are `pub` (`ast`, `documentation`, `error`, `eval`, `lexer`, `mode`,
`number`, `parser`); `calculator` is private and re-exported by type. Key
re-exports: `Calculator`, `EvalOutcome`, `Completion`, `FunctionDoc`; `Value`,
`FunctionValue`, `RecordValue`, `MapEntry`, `HostObject`; `Evaluator`,
`Resolvers`, `Reentry`, `Locals`; `EvaluationEnvironment`, `UserFunction`;
`BigDecimal`, `PrecisionContext`; `FunctionRegistry`; `LanguageMode`; the binary
bit-editor types; `DataType`/`DataField`/`DataFieldType`; `EngineError`.

## Module map

### `number/` — the exact core (`docs/ANZAN.md` §numbers)

- `number/mod.rs` — `BigDecimal` (`BigInt` significand × 10^exponent, always
  normalized so equality is structural) and `PrecisionContext`. `+ − ×`, integer
  `^`, `%` are exact; `/` and `sqrt` round to the context (default 50 sig-digits,
  banker's).
- `number/math.rs` — exact powers/roots, formatting, and `via_double` — the
  **single** transcendental fallback seam (round-trips through f64 via the
  pure-Rust `libm`, so results are platform-independent and match Swift). Route
  any new inexact function through `via_double`; add f64 math nowhere else.

### `lexer/` — source → tokens

`lexer/mod.rs` (the scanner), `lexer/token.rs` (`Token` + its half-open
character range). Positions are **character offsets** into a `Vec<char>` of
Unicode scalars (matching Swift's grapheme counts for everything the language can
express) so hosts render a caret under the offending column — preserve them.

### `ast/` — the expression tree

`ast/mod.rs` (the `Expression` tree the parser produces), `ast/source.rs` (a
**re-parseable** rendering — lambda display + workbook persistence; contract is
round-tripping, not prettiness, so it parenthesizes conservatively).

### `parser/` — Pratt precedence-climbing

`parser.rs` is the entry (grammar overview in its `//!`); productions split into
siblings:

- `parser/definitions.rs` — statement level: assignments, `import`,
  `namespace`/`data` declarations, field-type parsing, lambdas, user functions.
- `parser/expressions.rs` — the operator ladder: comparison → the Programmer-mode
  bitwise band → additive → term (implicit multiplication) → unary → power →
  postfix accessors.
- `parser/primary.rs` — literals, identifiers/calls, the `man`/`if`/∑/∏ special
  forms, array & map literals, reduction bounds.
- `parser/references.rs` — namespace-qualified names, sheet-qualified
  cell/named-cell references, cell ranges, argument lists (and the named-argument
  sugar that desugars to a single map).

The parser is **mode-parameterized** (`Parser::parse(_, mode)`); see
[../../docs/MODES.md](../../docs/MODES.md) for what the dialect switches change.

### `eval/` — values, environment, evaluator, functions

- `eval/value.rs` — `Value` (`Number`/`String`/`Array`/`Map`/`Record`/`FixedInt`/
  `FixedDecimal`/`Function`/`Host`). Immutable, nest freely; the canonical
  `Display` re-parses to an equal value (how structured variables persist).
- `eval/numeric.rs` — shared numeric helpers for the operator table + registry.
- `eval/environment.rs` — `EvaluationEnvironment` (user variables + `ans` +
  built-in constants) and `UserFunction`. Note: unlike Swift's reference-type
  `Environment`, the Rust evaluator threads `&mut EvaluationEnvironment`
  explicitly; host re-entry is mediated by the Calculator (single-threaded
  discipline preserved in both).
- `eval/data_type.rs` — a user-declared record type (`data Person { … }`); keeps
  its `source` line (with trailing `# doc`) for workbook serialization.
- `eval/fixed_int.rs` / `eval/fixed_decimal.rs` — the bounded, **checked**
  `Int…`/`UInt…` and `Decimal(p,s)` payloads (overflow errors, never wraps). See
  [../../docs/FIXED-WIDTH.md](../../docs/FIXED-WIDTH.md) and
  [../../docs/DECIMAL.md](../../docs/DECIMAL.md).
- `eval/json.rs` — hand-rolled `json_text` (`toJson`) + `JsonParser` (`fromJson`),
  exact inverses; JSON number literals go straight to `BigDecimal` (never f64).
- `eval/registry.rs` — `FunctionRegistry`, case-insensitive; a `BuiltinFunction`
  carries arity + implementation **+ required documentation** (signature,
  summary, examples — the doc tests evaluate every example).

**Evaluator** (`eval/evaluator.rs` + siblings) — walks the AST; mutates the
environment only via assignment/definition (the Calculator owns `ans`). Recursion
is bounded by memory + a sanity cap, never by the caller's thread: tail calls
loop at constant stack, and non-tail recursion grows onto fresh 16 MB segments
via `stacker::maybe_grow` (the Rust analogue of Swift's `continueOnFreshStack`).
Split by concern:

| Submodule | Concern |
|---|---|
| `evaluator/values.rs` | array/map literals, argument collection (ranges expand in place), ∑/∏ reductions |
| `evaluator/resolution.rs` | bare `.variable` lookup + namespace registration (the scoping ladder) |
| `evaluator/calls.rs` | resolving a call to builtin/user/scoped fn or a type constructor; overloads; applying a function VALUE |
| `evaluator/operators.rs` | indexing/subscripting + binary/comparison application (incl. the `FixedInt`/`FixedDecimal` hooks) |
| `evaluator/recursion.rs` | the tail-call loop + fresh-segment growth |
| `evaluator/helpers.rs` | free helpers shared across the above (namespace name math, type/operator lookups) |

The submodules expose their methods to siblings via `pub(super)` inherent-impl
blocks on `Evaluator`.

**Function library** (`eval/functions/`) — one submodule per category
(`core` + `core/implementations`, `logic` inside `core`, `trig`, `finance` +
`finance/helpers`, `dates`, `accounting`, `stats`, `data`, `programmer`,
`controls`), all merged in `FunctionRegistry::standard()` (`functions/mod.rs`).
Each is a direct port of the matching `swift/Engine/Sources/Anzan/Functions/*`.
Numeric builtins flatten array arguments; the `data` list is `Value`-aware (does
not flatten — `len([1,2])` must see the array).

### Binary bit-editor model (`eval/binary_view/`, `eval/binary_format.rs`, `eval/format_builder.rs`)

Pure, host-free models behind the app's macOS-Calculator-style bit grid — kept in
`anzan` so the calculator app, the standalone Tama app, and the tests draw from
one tested source. `binary_view.rs` (the read/edit register view + width policy +
two's-complement), `binary_view/fields.rs` (a `FieldSpec` + decoded `Field`),
`binary_view/layout.rs` (decode/encode a layout from a loose map or a typed
`Bits::BitFormat` record), `binary_format.rs` (the band palette, the typed `Bits`
schema/serializer, the built-in presets), and `format_builder.rs` (the visual
builder's model). Behavior spec: [../../docs/PROGRAMMER.md](../../docs/PROGRAMMER.md).

### The `Calculator` facade (`calculator.rs` + siblings)

`calculator.rs` owns the environment and runs lex → parse → eval, returning
`EvalOutcome`; `.value` updates `ans`, other outcomes (definitions, docs,
comments) don't. Siblings:

- `calculator/host_seams.rs` — the **pure** host seams: comment splitting,
  point-mode operand detection, programmer-notation sniffing, SpeedCrunch
  ans-prefixing, autocomplete word/candidate helpers.
- `calculator/documentation.rs` — a name's `man` page + the full reference
  catalogue (built-ins plus the live environment's functions/data types).

`documentation.rs` (crate root) is the reference-window data model
(`FunctionDoc`, `DocCategory`); built-ins document themselves at registration,
special forms/operators/constants are curated there.

## Cross-crate seams

The evaluator is host-agnostic: `Resolvers` are closures a host installs on the
`Calculator` (cell/range/name reads, sheet-scoped definitions, host reflection,
mutation), and `Reentry` threads the evaluator + environment back through those
closures so a cell read can re-enter the calculator without touching a borrowed
`RefCell`. The `soroban-engine` crate is the only in-repo host that wires them;
the CLI leaves them nil. See [ENGINE.md](ENGINE.md).

## Tests

Unit tests are sibling files (`lexer/tests.rs`, `parser/tests.rs`,
`number/tests.rs`, `value/tests.rs`), each a port of the matching Swift `*Tests`.
Integration tests live in `anzan/tests/` (`calculator_api`, `recursion`,
`typed_errors`, and the binary-view family). The behavioral truth is the shared
`spec/anzan` suite, run by the engine crate.

## See also

- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language spec (**not** this
  crate) and its companion docs.
- [ARCHITECTURE.md](ARCHITECTURE.md) · [ENGINE.md](ENGINE.md) · [CLI.md](CLI.md).
