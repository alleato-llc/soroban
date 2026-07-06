# Rust ecosystem architecture

How the Rust implementation realizes the [common design](../../docs/ARCHITECTURE.md):
a cargo workspace of three crates (`anzan` → `soroban-engine` → `soroban` CLI)
plus the workspace-**excluded** `gui` iced app. This page is the map; the
per-crate docs go deep.

The Rust tree mirrors the Swift ecosystem module-for-module — `anzan` ≙ Swift's
`Anzan`, `soroban-engine` ≙ `SorobanEngine`, `soroban` (CLI) ≙ `SorobanCLI`, and
`gui` ≙ the SwiftUI `App`. The behavior is identical (both pass the shared
`spec/**` suite); these docs describe how the Rust crates *realize* it, not what
the language does. For the language and shared semantics, follow the links into
[../../docs/](../../docs/). The porting history and Swift→Rust type mapping is
[../../docs/MIGRATION.md](../../docs/MIGRATION.md).

## The workspace

`rust/Cargo.toml` declares `members = ["anzan", "cli", "engine"]` with a
`resolver = "2"` workspace and shared dependency pins under
`[workspace.dependencies]` (num-bigint/-traits/-integer, libm, stacker, serde,
serde_json, rusqlite, cucumber, tokio) so the member crates never drift on a
version. The three members share **one** `rust/target/` dir and build together:

```sh
cd rust && cargo test --workspace --lib            # anzan + engine + cli
cd rust && cargo test -p soroban-engine --test gherkin   # the shared spec/** oracle
```

## Crate graph

```
        anzan  ──────────────►  soroban-engine  ──────►  gui  (excluded)
   (the language)  │            (hosting layer)          (iced + rime app)
                   └──────────►  soroban (cli)
```

- **`anzan`** (`anzan/`, package `anzan`) — the language: lexer → parser →
  evaluator + the exact `BigDecimal` number + the function library, fronted by
  the `Calculator` facade. Knows **nothing** about grids or files; hosts wire
  cells and reflection in through the calculator's resolver closures. Depends
  only on the numeric/`libm`/`stacker` crates. See [ANZAN.md](ANZAN.md).
- **`soroban-engine`** (`engine/`, package `soroban-engine`) — the hosting
  layer: the spreadsheet model (`Spreadsheet`, `SheetStore`, cells, the
  dependency graph, controls, named cells) and `.soroban` persistence. Its
  `lib.rs` opens with `pub use anzan::*;` (the Swift side's `@_exported import
  Anzan`), so a crate depending on `soroban-engine` gets the whole engine — apps
  never depend on `anzan` directly. Also pulls in serde/serde_json (the
  workbook codec) and rusqlite (data sheets). See [ENGINE.md](ENGINE.md).
- **`soroban`** (`cli/`, package `soroban-cli`, binary `soroban`) — the language
  at the command line. Depends on **`anzan` only** (plus `rustyline` for the
  REPL) — deliberately no sheet layer. See [CLI.md](CLI.md).
- **`gui`** (`gui/`, package `soroban-gui`) — the iced desktop app. **Not a
  workspace member.** See [GUI.md](GUI.md) and below.

`engine` and `cli` both `path`-depend on `../anzan`, so a workspace build
compiles `anzan` first; the three crates cross-compile as one unit.

## Why `gui` is excluded

`rust/Cargo.toml` carries `exclude = ["gui"]`. The `gui` crate depends on the
sibling **rime** component kit by a relative **path** — `rime = { path =
"../../../rime/rime" }` — which lives *outside* this repo (its own repo, serving
other apps), plus `iced` 0.14 and, transitively, wgpu and system graphics
libraries that a headless workspace/CI build must not need. Excluding it keeps
`cargo build`/`cargo test --workspace` and `rust-ci.yml` free of the heavy,
out-of-tree dependency:

```sh
cd rust/gui && cargo build          # standalone — NOT via --workspace
cd rust/gui && cargo test --test session
```

`gui` therefore has its **own** `rust/gui/target/` dir. Migration **Phase 4**
folds it back into the workspace (rime vendored/submoduled + platform deps
settled in a dedicated CI job). The app is also a **library** (`src/lib.rs`
exposes `session::Session`) so the headless cucumber suite can drive the UI-free
view-model without linking iced.

## Two test surfaces

- **The shared parity oracle** — `engine/tests/gherkin.rs` runs the SAME
  `spec/anzan/*.feature` files as the Swift engine against a real `SheetStore`
  (522 scenarios, cucumber, `harness = false`). This is the cross-ecosystem
  contract; a behavior change lands in `spec/` first, then in both ecosystems.
- **Port-local nets** — Rust-only suites the shared spec doesn't need:
  `engine/tests/*.rs` (reflection, mutation, structural edits, interchange
  round-trips against the repo-root `examples/*.soroban`), `anzan/tests/*.rs`
  (typed-error equality, recursion, the binary-view model), `cli/tests/smoke.rs`
  (the two non-interactive CLI modes), and `gui/tests/features/session.feature`
  (the headless `Session` suite — the counterpart to Swift's
  `SorobanSessionTests`).

## Conventions (Rust-specific)

- **File size** ~500 lines max; oversized files split into a module *directory*
  (`foo.rs` → `foo/` with sibling submodules). The recent refactor did exactly
  this — see each per-crate doc's module map.
- **Tests live in sibling files**, never inline: a source file ends with
  `#[cfg(test)] mod tests;` pointing at `<mod>/tests.rs`.
- **Cross-module visibility** uses `pub(crate)`/`pub(super)` — the evaluator's
  submodules expose their methods to their siblings via `pub(super)` inherent
  impls, and child modules see ancestor privates for free.

## See also

- [ANZAN.md](ANZAN.md) — the `anzan` crate · [ENGINE.md](ENGINE.md) ·
  [CLI.md](CLI.md) · [GUI.md](GUI.md).
- [../README.md](../README.md) — build/test/run · [../CLAUDE.md](../CLAUDE.md).
- [../../docs/MIGRATION.md](../../docs/MIGRATION.md) — the port design + type map.
