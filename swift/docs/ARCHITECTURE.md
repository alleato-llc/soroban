# Swift ecosystem architecture

How the Apple implementation realizes the [common design](../../docs/ARCHITECTURE.md):
the `Engine` package (two library modules + the CLI), the SwiftUI app, and the
shared Kit. This page is the map; the per-area docs go deep.

> **Status:** skeleton — full content lands in Phase 2 of the docs overhaul.
> Until then, the authoritative source is the root [../../CLAUDE.md](../../CLAUDE.md)
> and [../CLAUDE.md](../CLAUDE.md).

## Modules at a glance

- **`Engine/Sources/Anzan/`** — the language: `Lexer/`, `Parser/` (Pratt),
  `Eval/`, `Number/`, `Functions/`, fronted by the `Calculator` façade.
- **`Engine/Sources/SorobanEngine/`** — the hosting layer: `Sheet/` +
  `Persistence/`; `@_exported import Anzan`.
- **`Engine/Sources/SorobanCLI/`** — the `soroban` executable (Anzan only).
- **`App/`** — the SwiftUI app (`CalculatorSession`, `SheetModel`, the grid views).
- **`Kit/`** — `BinaryEditorKit`, the shared bit editor.

## See also

- [ENGINE.md](ENGINE.md) — engine internals · [APP.md](APP.md) — the app ·
  [CLI.md](CLI.md) · [KIT.md](KIT.md).
- [../README.md](../README.md) — build/test/run.
