# Contributing to Soroban

Soroban is an ecosystem-first monorepo: two independent implementations of the
same **Anzan** language and `.soroban` formats — a Swift/Apple stack and a Rust
stack — held to one shared behavior spec. Read
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first for the big picture.

> **Status:** skeleton — Phase 2 of the docs overhaul fills in the per-ecosystem
> setup, the full test-command cookbook, and the commit/PR conventions. The
> essentials are below.

## The golden rule: change the spec → change both

User-visible behavior lives in `spec/**` (Gherkin), and **both** implementations
must honor it. A behavior change lands as:

1. the feature-file edit in `spec/`,
2. the Swift implementation,
3. the Rust implementation,

in that order. Until one side catches up, tag the scenario `@rust-pending` /
`@swift-only` so its runner skips with visibility. See
[spec/README.md](spec/README.md).

## Where things live

- **Shared language & formats:** [docs/](docs/) (spec + design + process).
- **Swift / macOS + iPad:** [swift/README.md](swift/README.md).
- **Rust:** [rust/README.md](rust/README.md).

## Build & test (quick reference)

```sh
# Swift engine (the main Swift feedback loop)
cd swift/Engine && swift test

# Rust workspace + the shared parity suite
cd rust && cargo test --workspace --lib
cd rust && cargo test -p soroban-engine --test gherkin
```

Full per-ecosystem instructions are in each ecosystem's `README.md`.

## Changelogs & releases

Changelogs are split: `swift/CHANGELOG.md` (the `vX.Y.Z` track),
`rust/CHANGELOG.md` (the `rust-vX.Y.Z` track), and the root
[CHANGELOG.md](CHANGELOG.md) for genuinely cross-cutting changes (shared
`spec/**`, monorepo layout, interchange, common CI). Write the note under the
right `## [Unreleased]` in the same commit. Docs-only or test-only commits that
touch a release path (`swift/**`, `rust/**`, `spec/**`) on `main` must carry
`[skip ci]`. The full process is [docs/RELEASING.md](docs/RELEASING.md).
