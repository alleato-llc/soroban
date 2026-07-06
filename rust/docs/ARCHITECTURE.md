# Rust ecosystem architecture

How the Rust implementation realizes the [common design](../../docs/ARCHITECTURE.md):
the cargo workspace (`anzan` + `soroban-engine` + `soroban` CLI) and the
workspace-excluded `gui` iced app. This page is the map; the per-crate docs go deep.

> **Status:** skeleton — full content lands in Phase 2 of the docs overhaul. This
> is the largest authoring gap (the Rust ecosystem has had almost no prose docs),
> so Phase 2 documents the crate graph and each crate's module structure from
> scratch. Until then see [../../docs/MIGRATION.md](../../docs/MIGRATION.md).

## Crate graph

- **`anzan`** — the language (no grid/file knowledge). Mirrors Swift's `Anzan`.
- **`engine` (`soroban-engine`)** — the hosting layer; `pub use`s anzan. Mirrors
  Swift's `SorobanEngine`.
- **`cli` (`soroban`)** — the binary; depends on `anzan` only.
- **`gui`** — the iced + rime app; **workspace-excluded** (rime path-dep +
  iced/wgpu), own target dir, folded in at migration Phase 4.

## See also

- [ANZAN.md](ANZAN.md) — the `anzan` crate · [ENGINE.md](ENGINE.md) ·
  [CLI.md](CLI.md) · [GUI.md](GUI.md).
- [../README.md](../README.md) — build/test/run · [../CLAUDE.md](../CLAUDE.md).
