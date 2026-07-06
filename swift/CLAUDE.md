# CLAUDE.md — Swift ecosystem

Agent guidance for working under `swift/`. Loaded automatically (alongside the
root [../CLAUDE.md](../CLAUDE.md)) when you touch files here.

> **Status:** skeleton — in Phase 2 of the docs overhaul this file receives the
> Swift-specific architecture invariants and conventions currently in the root
> `../CLAUDE.md` (the engine subsystems, the app layer, grid-performance rules,
> workbook/persistence, theming), with file references corrected to the
> post-refactor module directories. **Until then, the root
> [../CLAUDE.md](../CLAUDE.md) remains authoritative for Swift detail.**

## Orientation

- Build/test/run: [README.md](README.md).
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and the per-area
  docs ([ENGINE](docs/ENGINE.md), [APP](docs/APP.md), [CLI](docs/CLI.md),
  [KIT](docs/KIT.md)).
- The language spec (shared): [../docs/ANZAN.md](../docs/ANZAN.md).

## Non-negotiables (carried from the root guide)

- Trust `swift test` / `xcodebuild` output over SourceKit's phantom "No such
  module" / "Cannot find type" errors.
- `Soroban.xcodeproj` is generated — run `xcodegen generate` after adding/removing
  files or editing `project.yml`.
- Don't add a Sheet/Persistence import to the `Anzan` module — the boundary is
  enforced.
