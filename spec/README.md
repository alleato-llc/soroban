# The shared behavior spec

`spec/` holds the Gherkin feature files that define Soroban's user-visible
behavior **once**, for both implementations. It is the cross-ecosystem parity
oracle — owned by neither `swift/` nor `rust/`, run by both.

## Layout

- **`spec/anzan/`** — the language scenarios (calculation, functions,
  spreadsheet, mathematics, structures, datatypes, library, reflection, modes,
  modules, decimal, fixedwidth, formatting, and `anzan.feature` itself — the
  executable companion to [docs/ANZAN.md](../docs/ANZAN.md), pinning grammar
  facts: precedence, associativity, the number lexicon, reserved words, scoping).
- **`spec/session/`** — session-layer scenarios (undo/redo, named-cell rename
  rewriting, control commits, CSV export, log-driven workbook mutation, and
  `History` reflection).

## How each ecosystem runs it

- **Swift** — via [PickleKit](https://github.com/alleato-llc/pickle-kit). The
  engine test target symlinks `spec/anzan` in as `Features/`; a session test
  target symlinks `spec/session`. Both surface as parameterized tests.
  See [../swift/README.md](../swift/README.md).
- **Rust** — via cucumber. The `soroban-engine` crate runs the same
  `spec/anzan` features against a real sheet store; `rust/gui` runs
  `spec/session`. See [../rust/README.md](../rust/README.md).

The features are reached by symlink/relative path from each runner — **edit the
files here in `spec/`, never through a copy.**

## The parity rule

A language or behavior change lands as a **feature-file edit plus both
implementations**, in that order. Until one side catches up, tag the scenario so
each runner skips it with visibility instead of failing:

- `@rust-pending` — implemented in Swift, not yet in Rust.
- `@swift-only` — Swift-specific behavior the Rust port won't mirror.

CI enforces both runners green on every **untagged** scenario. A change to
`spec/**` triggers CI in both ecosystems (and both release tracks), because it
is shared behavior.

## Division of truth

All user-visible input→output behavior belongs here, in the feature files.
Ecosystem-local unit tests keep only what scenarios can't express — typed-error
equality (including caret positions), codec round-trips, dependency-graph
invalidation, resolver wiring, recursion/stack canaries. When in doubt, a new
behavior is a scenario here first.
