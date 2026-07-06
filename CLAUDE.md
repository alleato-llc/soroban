# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository. It holds the **cross-cutting** rules; deep,
ecosystem-specific architecture and conventions live in nested `CLAUDE.md` files
that load automatically when you work under each tree.

## Documentation map

The docs are **modular, ecosystem-first**:

- **Shared design & language:** [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (the
  common design), [docs/ANZAN.md](docs/ANZAN.md) (the language spec) and its
  companions, [docs/FORMAT.md](docs/FORMAT.md) (interchange), the
  [docs/](docs/README.md) index.
- **Swift / Apple:** [swift/README.md](swift/README.md),
  [swift/docs/](swift/docs/ARCHITECTURE.md), and
  [swift/CLAUDE.md](swift/CLAUDE.md) — the authoritative Swift architecture &
  conventions.
- **Rust:** [rust/README.md](rust/README.md),
  [rust/docs/](rust/docs/ARCHITECTURE.md), and [rust/CLAUDE.md](rust/CLAUDE.md) —
  the authoritative Rust conventions.
- **Shared behavior spec:** [spec/README.md](spec/README.md).
- **Contributing / releases:** [CONTRIBUTING.md](CONTRIBUTING.md),
  [docs/RELEASING.md](docs/RELEASING.md).

When you work under `swift/` or `rust/`, that tree's `CLAUDE.md` is the source of
truth for its specifics; this file only covers what spans both.

## Quick commands

The main feedback loops (full cookbook in each ecosystem's `README.md`):

```sh
# Swift engine (the main Swift loop) + the shared spec run
cd swift/Engine && swift test
cd swift/Engine && swift test --filter GherkinTests

# Swift app (Soroban.xcodeproj is GENERATED — regenerate after file/project.yml changes)
cd swift && xcodegen generate && xcodebuild -project Soroban.xcodeproj -scheme Soroban build

# Rust workspace + the shared spec run (anzan, engine, cli share one target)
cd rust && cargo test --workspace --lib
cd rust && cargo test -p soroban-engine --test gherkin

# Rust gui — EXCLUDED from the workspace; build/test standalone, never --workspace
cd rust/gui && cargo test
```

There is no linter configured. The apps have no UI test target; behavior is
covered by the engine/session tests and the shared spec.

## Monorepo layout (ecosystem-first)

`swift/` (everything Apple), `rust/` (the cargo workspace + the excluded `gui`
app), `spec/` (the shared Gherkin behavior spec, owned by neither ecosystem),
`docs/` (shared language/format/design), `site/` (the landing page + living
spec). Bare `Engine/`, `App/`, `Kit/` paths in older notes mean
`swift/Engine/`, `swift/App/`, `swift/Kit/`. The full design is
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md); the port plan is
[docs/MIGRATION.md](docs/MIGRATION.md).

## The shared spec (parity — the load-bearing rule)

User-visible behavior lives in `spec/**` and **both** implementations must honor
it. A behavior change lands as a `spec/` feature-file edit **plus** both
implementations, in that order; until one catches up, tag the scenario
(`@swift-only` / `@rust-pending`) so its runner skips with visibility. The
Swift and Rust test targets reach the features by symlink/relative path — **edit
the features in `spec/`, never a copy.** Details: [spec/README.md](spec/README.md).

## Releases, changelogs, commits

Two independent, [salpa](https://github.com/alleato-llc/salpa)-driven release
tracks, each auto-tagged on a path-gated merge to `main`: **macOS** (`release.yml`,
on `swift/**`·`spec/**` → `vX.Y.Z`, signed/notarized `Soroban.dmg`) and **Rust**
(`release-rust.yml`, on `rust/**`·`spec/**` → `rust-vX.Y.Z`). A `spec/**` change
fires both. Full process + secrets: [docs/RELEASING.md](docs/RELEASING.md).

Rules an agent must not miss:

- **Changelogs are split**: `swift/CHANGELOG.md` (`v*`), `rust/CHANGELOG.md`
  (`rust-v*`), and the root [CHANGELOG.md](CHANGELOG.md) for **cross-cutting**
  changes (shared `spec/**`, monorepo layout, interchange, common CI). Write the
  note under the right `## [Unreleased]` in the SAME commit.
- **`[skip ci]` is mandatory** on any commit that should NOT cut a release —
  docs-only, a CHANGELOG promotion, or test-only — when it touches a release path
  (`swift/**`, `rust/**`, `spec/**`) on `main`. (Path-gating already spares
  `site/**`/`docs/**`-only pushes; a `site/**` change triggers `deploy-site.yml`.)
- Commit/PR conventions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Architecture

Two strictly separated layers — the **Anzan** language (host-agnostic) and the
**hosting layer** (sheets + persistence) — with a thin CLI on the language and a
GUI on the host, in each ecosystem. The common design is
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md); the per-ecosystem realizations are
[swift/CLAUDE.md](swift/CLAUDE.md) / [swift/docs/](swift/docs/ENGINE.md) and
[rust/CLAUDE.md](rust/CLAUDE.md) / [rust/docs/](rust/docs/ARCHITECTURE.md). Deep
subsystem detail that used to live here now lives in those ecosystem docs.
