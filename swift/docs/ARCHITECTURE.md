# Swift ecosystem architecture

How the Apple implementation realizes the [common design](../../docs/ARCHITECTURE.md):
the `Engine` package (two library modules + the CLI), the SwiftUI app, and the
shared Kit. This page is the map; the per-area docs go deep.

## The three build products

Everything Apple lives under `swift/` as **three** SwiftPM packages plus an
XcodeGen project:

| Package / project | Products | Depends on |
|---|---|---|
| `Engine/` (`Package.swift`) | `SorobanEngine` library, `soroban` executable | BigInt; LineNoise (CLI only); PickleKit (tests only) |
| `Kit/` (`Package.swift`) | `BinaryEditorKit` library | `Engine` (by path), BigInt |
| `App/` (via `project.yml` → generated `Soroban.xcodeproj`) | `Soroban.app` (macOS + iPad), `SorobanSessionTests` | `SorobanEngine`, `BinaryEditorKit`, BigInt, PickleKit (tests) |

`Soroban.xcodeproj` is **generated and gitignored** — run `xcodegen generate`
after editing `project.yml` or adding/removing files under `App/`.

## The two engine modules

`Engine/` is one package with two library targets, mirroring the [common
design](../../docs/ARCHITECTURE.md)'s two-layer split:

- **`Engine/Sources/Anzan/`** — the language ([暗算](../../docs/ANZAN.md), mental
  abacus calculation). `Lexer/` → `Parser/` (Pratt) → `Eval/` + `Number/` +
  `Functions/`, fronted by the `Calculator` façade. Anzan knows **nothing**
  about grids or files; hosts wire cells and reflection in through resolver
  closures. The package boundary enforces this — Anzan must not import
  `Sheet/` or `Persistence/`. Depends only on `BigInt`.
- **`Engine/Sources/SorobanEngine/`** — the hosting layer: `Sheet/`
  (spreadsheet, cells, formats, controls, named cells, worksheets, reflection)
  + `Persistence/` (workbook codec, journal, package, data store). It
  `@_exported import Anzan`s (see `Exports.swift`), so depending on
  `SorobanEngine` gives the whole engine — the app never imports `Anzan`
  directly.

Cross-module internals use Swift's `package` access level. Tests
`@testable import` both. Deep internals: **[ENGINE.md](ENGINE.md)**.

## The CLI

`Engine/Sources/SorobanCLI/main.swift` is the `soroban` executable — the
language without the app. It depends on **`Anzan` only** and deliberately has
no sheet layer: one `Calculator` per invocation, mode chosen by invocation
shape (one-shot args / piped stdin / interactive REPL). Keep it plumbing-thin;
behavior worth testing belongs in the engine. Details: **[CLI.md](CLI.md)**.

## The app

`App/` is the SwiftUI app (bundle id `com.alleato.Soroban`), a thin view layer
over `SorobanEngine`:

- **`App/Sources/Session/`** — the UI-free model layer: `CalculatorSession`
  (the `@Observable` view-model over `Calculator`), `SheetModel` (grid state +
  persistence, split across `SheetModel/SheetModel+*.swift`), `LogStore`,
  `WorkbookManager`.
- **`App/Sources/`** (top level) — the views: `ContentView`, `SpreadsheetView`,
  `GridRowView`/`CellView`, `HistoryLogView`, `InputBarView`, `InspectorView`,
  `ReferenceView`, plus `Theme/ThemeManager.swift`.

Architecture, grid-performance invariants, workbook/persistence, theming, and
the feature tour: **[APP.md](APP.md)**.

## The Kit

`Kit/Sources/BinaryEditorKit/` — the reusable bit-editor UI (the
macOS-Calculator-style bit grid, bit-field format decode/encode, visual
builder), factored out so both the calculator and the standalone **Tama** app
embed one component. It depends on `Engine` for the host-neutral logic
(`BinaryView`, `BinaryFormat`) and exposes a `BinaryEditorHost` seam each app
conforms to. Details: **[KIT.md](KIT.md)**.

## Where behavior is proven

The engine tests (`Engine/Tests/SorobanEngineTests/`, Swift Testing) are the
main feedback loop, and the shared [`spec/`](../../spec/README.md) Gherkin
features run through PickleKit twice — the engine `GherkinTests` (language) and
the app `SorobanSessionTests` (session layer). See [../README.md](../README.md)
for build/test/run.

## See also

- [ENGINE.md](ENGINE.md) — engine internals · [APP.md](APP.md) — the app ·
  [CLI.md](CLI.md) · [KIT.md](KIT.md).
- [../README.md](../README.md) — build/test/run · [../CLAUDE.md](../CLAUDE.md) —
  agent guide.
- [../../docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md) — the shared design ·
  [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language spec.
