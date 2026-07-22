# Soroban — Swift / Apple ecosystem

Everything Apple: the exact-arithmetic **engine**, the **macOS + iPad app**, the
`soroban` **CLI**, and **BinaryEditorKit** — one SwiftPM + XcodeGen world.

For the shared design and the Anzan language, start with
[../docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) and
[../docs/ANZAN.md](../docs/ANZAN.md). This page is the ecosystem entry point;
deeper implementation docs are in [docs/](docs/).

## What's here

| Path | What it is |
|---|---|
| `Engine/` | One SwiftPM package, two library modules — `Anzan` (the language) + `SorobanEngine` (the hosting layer) — plus the `SorobanCLI` executable. No UI. |
| `App/` | The SwiftUI app (macOS + iPad), bundle id `com.alleato.Soroban`. |
| `Kit/` | `BinaryEditorKit` — the shared bit-editor component (also used by the standalone Tama app). |
| `project.yml` | The XcodeGen project definition. `Soroban.xcodeproj` is generated + gitignored. |
| `CHANGELOG.md` | The Swift track's history (`vX.Y.Z`). |

Detailed structure: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) ·
[docs/ENGINE.md](docs/ENGINE.md) · [docs/APP.md](docs/APP.md) ·
[docs/CLI.md](docs/CLI.md) · [docs/KIT.md](docs/KIT.md).

## Build & run

```sh
# The app project is GENERATED — regenerate after changing project.yml or
# adding/removing files under App/.
cd swift && xcodegen generate
cd swift && xcodebuild -project Soroban.xcodeproj -scheme Soroban build

# Launch the built app (judge performance on a Release build, not Debug)
cd swift && open "$(xcodebuild -project Soroban.xcodeproj -scheme Soroban \
  -configuration Release -showBuildSettings 2>/dev/null \
  | awk '/ BUILT_PRODUCTS_DIR/{print $3}')/Soroban.app"

# The soroban CLI (one-shot args / .anzan script files / pipe / REPL) — depends on Anzan only
cd swift/Engine && swift build -c release --product soroban
install -m 755 .build/release/soroban ~/.local/bin/
```

## Test

```sh
# Engine tests (Swift Testing) — the main feedback loop
cd swift/Engine && swift test
cd swift/Engine && swift test --filter GherkinTests   # the shared spec/anzan run

# Session-layer Gherkin (undo, rename rewriting, control commits, History)
cd swift && xcodegen generate && \
  xcodebuild test -project Soroban.xcodeproj -scheme Soroban -destination 'platform=macOS'
```

Both Gherkin runs execute the SAME `spec/**` features as the Rust engine — the
cross-ecosystem parity suite. See [../spec/README.md](../spec/README.md). CI
requires **Swift 6.2+** (PickleKit), so test jobs pin Xcode 26.2 on `macos-26`.

## Agent notes

Working in `swift/`? See [CLAUDE.md](CLAUDE.md) for the ecosystem's architecture
invariants and conventions (loaded automatically alongside the root
[../CLAUDE.md](../CLAUDE.md)).
