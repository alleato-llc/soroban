# BinaryEditorKit

`Kit/Sources/BinaryEditorKit/` — the reusable bit-editor UI: the
macOS-Calculator-style bit grid, the bit-field format decode/encode surface,
and the visual format builder. Factored into its own SwiftPM package so both
the Soroban calculator and the standalone **Tama** app embed one component
instead of duplicating it.

The bit-field *format model* (numeric/flags/enum/reserved/unused fields, the
`Bits` module) is language-shared and specified in
[../../docs/PROGRAMMER.md](../../docs/PROGRAMMER.md); this page covers the Swift
**UI component**.

## Package

`Kit/Package.swift` — `platforms: [.macOS(.v14), .iOS(.v17)]` (pure SwiftUI, so
it links into the iPad app). Depends on:

- **`Engine`** (by relative path) for the product `SorobanEngine`, which
  re-exports `Anzan` — the **host-neutral logic already lives in the engine**
  (`BinaryView`, `BinaryFormat`, `BinaryView.FormatBuilder` under
  `Engine/Sources/Anzan/Eval/`). The Kit is the thin host-specific SwiftUI
  surface over that.
- **`BigInt`** — bit-field values are `BigInt`.

## Files

| File | Owns |
|---|---|
| `BinaryEditorView.swift` | the overlay's root view; resolves `binaryView` once |
| `BinaryEditorView+Header.swift` | the header — width picker, format menu, value readouts |
| `BinaryEditorView+Grid.swift` | the clickable bit grid; the `Equatable` `BitButton` |
| `BinaryEditorView+Builder.swift` | the visual format builder (claim bits → detail → Add) |
| `BinaryEditorHost.swift` | the **host seam** (protocol) |
| `NibbleLayout.swift` | pure column-count math for the nibble grid |

## The host seam

`BinaryEditorHost` (`BinaryEditorHost.swift`) is a `@MainActor` `Observable`
protocol — the view talks **only** to it, and each app supplies a concrete
`@Observable` conformer:

- The **value** (`binaryView: Result<BinaryView, Unavailable>`, `width`,
  `hasEdits`, `flipBit`, `setField`, `cancelEdits`) — the host owns the staged
  draft.
- **Emitting** a value (`useValue()` / `insert(_:)`) — the host decides what
  that means: the calculator inserts into its input line; Tama copies to the
  pasteboard.
- **Formats** (`presets`, `savedFormats`, `activeFormat`/`activeLayout`,
  `applyFormat`, `applyBuiltFormat`, `saveFormat`, and the
  `canManageSavedFormats` rename/delete triad) — presets ship with the host;
  saved formats persist however the host wants (a workbook variable for the
  calculator, a JSON file for Tama).
- **Environment** (`theme: BinaryEditorTheme`, `dismiss()`).

Rendering is flip-cheap: the overlay resolves `binaryView` once and the grid
uses an `Equatable` `BitButton`, so a single flip re-renders only the changed
bit; enum fields render as a labeled `Picker`.

`NibbleLayout.swift`'s `nibbleColumnCount(...)` is pure so it unit-tests away
from SwiftUI's `Layout` (`Tests/BinaryEditorKitTests/NibbleLayoutTests.swift`).
Its guards matter: SwiftUI proposes an *infinite* width while sizing a
content-sized window, and a naive `Int(maxWidth / itemWidth)` traps on
`Int(.infinity)` — that crash shipped once.

## The calculator's conformer

The app wires the Kit through `App/Sources/CalculatorBinaryHost.swift` (its
`BinaryEditorHost` conformer, holding the built-in presets and format
persistence) and `App/Sources/Session/CalculatorSession+Binary.swift` (the
session-side staging, `ans`-prefix, and value insertion). See
[APP.md](APP.md#binary-bit-editor) for the app-side behavior — the overlay is
Programmer-mode-only, flips stage a live draft, and **Use** inserts a literal
into the input line rather than posting to the log. The calculator persists a
saved format as a typed `Bits::BitFormat` workbook variable.

## See also

- [../../docs/PROGRAMMER.md](../../docs/PROGRAMMER.md) — the shared bit-field
  format model (the language side).
- [APP.md](APP.md) — the app-side binary editor glue · [ENGINE.md](ENGINE.md)
  — `BinaryView`/`BinaryFormat` (the host-neutral logic) ·
  [ARCHITECTURE.md](ARCHITECTURE.md).
