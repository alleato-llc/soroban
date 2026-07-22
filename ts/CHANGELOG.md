# Changelog - TypeScript package

Changes to the **TypeScript package only** (`ts/` — `@alleato/anzan`).
Cross-cutting changes (the shared `spec/**`, the language, monorepo layout)
live in the [root CHANGELOG](../CHANGELOG.md); the Rust engine the wasm wraps
has its own [rust/CHANGELOG.md](../rust/CHANGELOG.md).
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Versioning starts at **0.1.0**; the package is npm-ready but not yet
published (nothing publishes it).

## [Unreleased]

### Added

- **`@alleato/anzan` 0.1.0 — Anzan for JS hosts.** The Rust engine compiled to
  WebAssembly (`rust/wasm`, vendored under `wasm/` for both the nodejs and web
  targets — no Rust toolchain needed to install or CI) behind a typed SDK:
  stateful `Calculator` sessions (`evaluate`, `runScript` with script halt
  semantics, mode switching, completions, documentation), the streaming
  `StatementAccumulator`, `runScript`/`statements` helpers, and discriminated
  outcome unions (`EvalOutcome`, `AnzanError`, `ScriptResult`) — the backend's
  JSON is parsed once at the boundary, behind an `EngineBackend` seam a future
  pure-TS engine can fill.
- **The fourth Anzan CLI** (`src/cli/anzan.ts`, bin `anzan`): one-shot args in
  a shared session, `.anzan` script files halting at the first error with the
  caret + `at file:line`, statement-aware continue-on-error pipes with silent
  `:mode` handling, and a readline REPL (`> ` / `… ` continuation) — matching
  the Swift and Rust `soroban` contract (differentially verified against the
  Rust binary).
- **The shared-spec runner** (`npm run spec`): cucumber-js over the shared
  `spec/anzan` features by path — the pure-language subset (267 scenarios,
  100% passing); host-dependent features and the native deep-recursion canary
  are excluded and documented in [README.md](README.md).
