# CLAUDE.md — Rust ecosystem

Agent guidance for working under `rust/`. Loaded automatically (alongside the
root [../CLAUDE.md](../CLAUDE.md)) when you touch files here. The root file owns
the shared semantics (the engine design, the exactness model, the
dependency-graph recalc, reflection); this file owns the Rust-specific mechanics.

## Orientation

- Build/test/run: [README.md](README.md).
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and the per-crate
  docs — [ANZAN](docs/ANZAN.md) (the `anzan` crate), [ENGINE](docs/ENGINE.md),
  [CLI](docs/CLI.md), [GUI](docs/GUI.md).
- The language spec (shared, ecosystem-agnostic): [../docs/ANZAN.md](../docs/ANZAN.md)
  and its companions ([MODES](../docs/MODES.md), [MODULES](../docs/MODULES.md),
  [FIXED-WIDTH](../docs/FIXED-WIDTH.md), [DECIMAL](../docs/DECIMAL.md),
  [PROGRAMMER](../docs/PROGRAMMER.md), [FORMAT](../docs/FORMAT.md),
  [STDLIB](../docs/STDLIB.md)). The port design + Swift→Rust type map is
  [../docs/MIGRATION.md](../docs/MIGRATION.md). Changelog: [CHANGELOG.md](CHANGELOG.md).

Note **`docs/ANZAN.md` (this dir) documents the `anzan` CRATE** — module layout;
**`../docs/ANZAN.md` is the language spec**. Don't conflate them.

## The workspace at a glance

```
rust/                 members = ["anzan", "cli", "engine"]   (one shared target/)
├── anzan/   the language (no grid/file knowledge)            → docs/ANZAN.md
├── engine/  soroban-engine — hosting layer, pub use anzan::* → docs/ENGINE.md
├── cli/     soroban binary — depends on anzan ONLY           → docs/CLI.md
└── gui/     soroban-gui — iced + rime app; EXCLUDED          → docs/GUI.md
```

## Non-negotiables

- **`rust/gui` is excluded from the workspace** (`exclude = ["gui"]` in
  `rust/Cargo.toml`) and has its **own** target dir. It path-depends on the
  sibling **rime** kit (`../../../rime/rime`, outside this repo) plus iced/wgpu.
  Build and test it standalone (`cd rust/gui && cargo …`), **never** via
  `--workspace`. A workspace command must not reach it.
- **The three workspace crates share one target dir**; `engine` and `cli`
  `path`-depend on `../anzan`, so a workspace build compiles `anzan` first — mind
  cross-crate build ordering when you change `anzan`'s public API (it ripples into
  `engine`/`cli`/`gui`).
- **`engine` re-exports the language** (`pub use anzan::*;`). Apps and the engine's
  own code reach `Value`, `Calculator`, etc. through `soroban_engine::…`; only the
  CLI depends on `anzan` directly. Don't add a sheet/persistence dependency to
  `anzan` — the boundary is load-bearing (mirrors the Swift module split).
- **Tests live in sibling files.** House style is `#[cfg(test)] mod tests;` at the
  bottom of a source file, pointing at `<mod>/tests.rs` — **never** an inline
  `#[cfg(test)] mod tests { … }` block. Ports of Swift `*Tests` keep the same
  cases (`lexer/tests.rs` ≙ `LexerTests.swift`).
- **~500 lines per file.** Oversized files split into a module *directory* of
  cohesive siblings (the recent refactor: `evaluator.rs` → `evaluator/{values,
  resolution,calls,operators,recursion,helpers}`, `sheet_store.rs` →
  `sheet_store/{resolvers,mutation}`, `session.rs` → `session/{document,cells,…}`).

## Rust idioms in this tree

- **Cross-module visibility.** A module directory's siblings expose methods to
  each other with `pub(super)` inherent-impl blocks on the shared type (the
  `Evaluator` submodules do this), and child modules see ancestor privates for
  free. Reach for `pub(crate)` before `pub` — keep the crate's public surface the
  set of `pub use`s in `lib.rs`, nothing accidental (e.g. `engine`'s `reflection`
  module is `pub(crate)`: the language navigates its handles by trait).
- **Re-entrancy without a borrowed RefCell.** Swift's `Calculator` is a shared
  reference type; the Rust evaluator threads `&mut EvaluationEnvironment` plus a
  `Reentry`/`Resolvers` pair explicitly. Host resolver closures capture a `Weak`
  of the store and receive `(&Evaluator, &mut EvaluationEnvironment)` as
  arguments, so they never borrow the Calculator `RefCell`. Keep this discipline —
  don't snapshot state to dodge it (a frozen copy wouldn't record dependency
  edges), and don't hold a `RefCell` borrow across an inner evaluation.
- **The transcendental seam.** Inexact math (trig/exp/ln/non-integer pow) goes
  through `number/math.rs::via_double` (pure-Rust `libm`, platform-independent so
  Rust and Swift agree). Add f64 math nowhere else.
- **Deep recursion** grows the stack via `stacker::maybe_grow` (the Rust analogue
  of Swift's `continueOnFreshStack`), not a fixed depth cap. Tail calls loop at
  constant stack. See `eval/evaluator/recursion.rs`.
- **The gui screenshot harness** (`gui/src/shot.rs`) is **permanent** and
  env-gated (`SOROBAN_SHOT*`) — never re-add or remove the plumbing; extend it
  with new vars for new views ([docs/GUI.md](docs/GUI.md)).

## The behavioral contract

- The cross-ecosystem oracle is the shared `spec/**` suite, run by
  `cargo test -p soroban-engine --test gherkin` (522 scenarios against a real
  `SheetStore`). A behavior change lands as a `spec/` feature edit **plus** both
  implementations; scenarios one ecosystem hasn't caught up to get tagged
  (`@rust-pending`). Never make the Rust engine diverge from the Swift one without
  the spec changing.
- `gui/tests/features/session.feature` is the port's OWN net (headless `Session`
  driving), not a cross-ecosystem oracle — the counterpart to Swift's
  `SorobanSessionTests`.
- `rust-ci.yml` runs fmt + clippy + the workspace tests on `rust/**`·`spec/**`
  (`gui` excluded). Keep it clean.

## Verify before finishing

```sh
cd rust && cargo fmt --all && cargo clippy --workspace --all-targets
cd rust && cargo test --workspace --lib && cargo test -p soroban-engine --test gherkin
cd rust/gui && cargo test          # only if you touched gui/
```
