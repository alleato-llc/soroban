# CLAUDE.md — Rust ecosystem

Agent guidance for working under `rust/`. Loaded automatically (alongside the
root [../CLAUDE.md](../CLAUDE.md)) when you touch files here.

> **Status:** skeleton — in Phase 2 of the docs overhaul this file receives the
> Rust-specific conventions (the crate graph, module-privacy rules, the
> workspace/target layout, the gui/rime nuance), authored fresh since the Rust
> ecosystem had almost no prose docs. Until then see
> [../docs/MIGRATION.md](../docs/MIGRATION.md).

## Orientation

- Build/test/run: [README.md](README.md).
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and per-crate docs
  ([ANZAN](docs/ANZAN.md), [ENGINE](docs/ENGINE.md), [CLI](docs/CLI.md),
  [GUI](docs/GUI.md)).
- The language spec (shared): [../docs/ANZAN.md](../docs/ANZAN.md).

## Non-negotiables

- `rust/gui` is **excluded** from the workspace (`exclude = ["gui"]`) and has its
  own target dir — build/test it standalone, not via `--workspace`.
- The workspace crates (`anzan`, `engine`, `cli`) share one target dir; `engine`
  and `cli` build against `anzan`, so mind cross-crate build ordering.
- House test style is **sibling test files** (`#[cfg(test)] mod tests;` →
  `<mod>/tests.rs`), not inline `#[cfg(test)] mod tests { … }` blocks.
- Keep every source file under ~500 lines; split into cohesive modules.
