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

- **Scientific mode replaces finance; currency is core grammar** (the shared
  spec change — see the root CHANGELOG). `Mode` is now
  `normal | programmer | scientific`; currency (`$10`) and thousands grouping
  (`138,561`) work in every mode with no mode switch. New on `Calculator`:
  `sciStyle` (`"sci" | "eng"` — the scientific-echo variant) and
  `setModeParsing(":mode"-argument)` — the engine's one shared `:mode` parse
  seam, so the CLI's mode list and errors (including the `finance`
  promotion hint) can't drift from the native hosts'. `displayDescription`
  now honors the session's mode/style across the wasm boundary
  (`123456 * 2` echoes `2.46912e5` in scientific, `246.912e3` under `eng`),
  and the mode-agnostic `°` degree literal arrives with the engine
  (`sin(90°)` is `1`). The CLI speaks `:mode scientific [eng]`; the shared
  spec run moves **267 → 279 scenarios** (modes.feature +9,
  mathematics.feature +3, matching the native runners' 567 → 579). The
  vendor step now refreshes all three wasm locations, including the site's
  (`site/src/wasm`).

- **`environment()` and `reference()`** — the session's inspector data
  (variables, functions, data types, `ans`) and the full builtin reference
  (name/category/signature/summary/examples), mirroring the desktop apps'
  environment inspector and help browser. Exposed on `Calculator` /
  the backend seam; the site's REPL toolbar consumes both.

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
