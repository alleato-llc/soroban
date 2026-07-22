---
name: run-apps
description: Verified paths for building, launching, driving, and screenshotting the Soroban apps — the Swift/macOS app and the Rust (iced) app — plus the .anzan/.soroban file tricks for driving them without keyboard access. Use when asked to run the app, demo a change, or capture screenshots.
---

# Running the apps (verified paths)

## Swift / macOS app

```sh
cd swift && xcodegen generate       # Soroban.xcodeproj is GENERATED (gitignored)
xcodebuild -project Soroban.xcodeproj -scheme Soroban -configuration Debug build
APP=$(ls -d ~/Library/Developer/Xcode/DerivedData/Soroban-*/Build/Products/Debug/Soroban.app | head -1)
open "$APP"                          # or: open "$APP" path/to/file.soroban
```

- Relaunch after a rebuild: `osascript -e 'quit app "Soroban"'; sleep 1; open "$APP"`.
- **Screenshot window-scoped, not full-screen** (full-screen captures the
  user's entire desktop — private windows included):
  ```sh
  osascript -e 'tell application "Soroban" to activate'; sleep 2
  WID=$(osascript -e 'tell application "System Events" to tell process "Soroban" to get id of window 1')
  screencapture -x -o -l "$WID" shot.png
  ```
  Then LOOK at the image — a blank frame is a failed launch.
- **Keystroke injection is blocked** (`osascript … keystroke` → error 1002)
  unless the terminal has an Accessibility grant. Don't push the user through
  granting it — drive the app with FILES instead (below). A multi-line paste
  into the log input is untested territory; the input bar is single-line.

### Driving without a keyboard: author a `.soroban` workbook

The app opens workbooks; definitions and a demo sheet arrive ready-to-use.
Minimal shape (mimic `examples/interchange.soroban`):

```json
{ "format": "soroban-workbook", "version": 2, "activeSheet": "Demo",
  "dataTypes": { "T": "data T { x: Number }" },
  "functions":  [ "f(x) = x * 2  # doc" ],
  "variables":  {},
  "sheets": [ { "name": "Demo",
      "cells": { "A:1": "label", "B:1": "0.95", "B:2": "=f('In')" },
      "names": { "B:1": "In" },
      "columnWidths": {}, "formats": {}, "rowHeights": {} } ] }
```

Cells: `"=…"` formulas, bare numbers, bare text; `names` maps addresses to
`'Named Cell'` references. Cells are scalar — a record-returning call needs a
field access (`=f(x).field`). The log (⌘\ toggles) can hold records.

## Rust (iced) app

```sh
cd rust/gui && cargo run             # NEVER via --workspace (gui is excluded);
                                     # path-depends on the sibling ../../../rime repo
```

Headless screenshots: the PERMANENT env-gated harness in `gui/src/shot.rs` —
`SOROBAN_SHOT*` vars, see `rust/docs/GUI.md`. Never re-add/remove the
plumbing; extend it with new vars for new views.

## CLI quick checks (no GUI needed)

Both binaries speak one-shot args, `.anzan` script files, statement-aware
pipes, and a REPL — see the `anzan` skill. Fastest smoke test of an engine
change is the CLI, not the app.

```sh
swift/Engine/.build/debug/soroban file.anzan     # after: swift build --product soroban
cd rust && cargo run -q --bin soroban -- file.anzan
```
