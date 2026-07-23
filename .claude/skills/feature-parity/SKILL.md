---
name: feature-parity
description: The end-to-end workflow for adding or changing Anzan language / engine behavior — spec first, Swift, Rust mirror, differential verification, changelogs, docs. Use for any user-visible behavior change, new language feature, new builtin, or new Value type.
---

# Adding a language/behavior feature (the parity workflow)

User-visible behavior lives in `spec/**` and BOTH engines must honor it. A
change lands as: **spec edit → Swift (the reference) → Rust mirror →
differential check → changelogs → docs**, in that order. This skill is the
checklist; the traps at each step are the reason it exists.

## 0. Branch (never commit to main) and settle the design

Ask the user the genuinely open product decisions BEFORE writing code (error
semantics, syntax forks, display forms). Two examples that changed mid-flight
when skipped: `$10 * $2` semantics, script halt-vs-continue.

## 1. Spec first — `spec/anzan/*.feature`

- Edit features in `spec/`, NEVER a copy (test targets reach them by symlink
  / path).
- `the result is` asserts the **canonical** `description` (`Money(10, "USD")`,
  `Int8(8)`, `Decimal(0.50, 2)`); `the log echoes` asserts the human
  `displayDescription` (`$10.00`). Mixing these up once broke 18 scenarios.
- Multi-line programs: the `When I run the script:` docstring step.
- If one engine will lag, tag the scenario `@swift-only` / `@rust-pending`.

## 2. Swift implementation (the reference)

File map: `Lexer/` → `Parser/` (+AST, Expression+Source) → `Eval/`
(Evaluator+extensions, Value) → `Functions/`. Know the blast radius:

- **A new `Value` case**: the compiler catches the exhaustive switches, but
  these SILENT sites it won't — `asNumber` (coercion), `==` (else it's
  unequal to itself), `Value.literal` (else it never restores), the
  `apply` typed-arithmetic dispatch order in `Evaluator+Operators.swift`,
  and unary-minus/percent tag preservation in `Evaluator.swift`.
- **A new builtin**: `category`/`signature`/`summary`/`examples` are REQUIRED
  init params and `DocumentationTests` evaluates every example. Model on
  `Decimal` in `Functions/AccountingFunctions.swift`.
- **Naming**: check `man <name>` first — user definitions can't shadow
  builtins, and CALLS resolve to the builtin silently (returns plausible
  nonsense, no error).
- New steps go in `swift/Engine/Tests/SorobanEngineTests/SorobanSteps.swift`
  (PickleKit discovers `StepDefinition` properties by reflection; `match`
  exposes `captures` / `docString` / table).

Run `cd swift/Engine && swift test` and **verify the gherkin scenario COUNT
rose** — a green run with a stale count means your scenarios didn't execute.

## 3. Rust mirror

File-for-file twin under `rust/anzan/src/` (lexer/, parser/, ast/, eval/,
number/). House rules (rust/CLAUDE.md): tests in sibling `<mod>/tests.rs`
files, ~500 lines per file, `pub use` the public surface from `lib.rs`, NEVER
run `--workspace` into `rust/gui`. Steps mirror in
`rust/engine/tests/gherkin.rs` (`step: &Step` param for docstrings/tables).

If delegating the mirror to a subagent: give it the Swift diff as the spec,
require verbatim test output back, and do NOT run builds/tests yourself while
it's mid-edit — a half-written tree can compile into nonsense (we chased a
phantom stack overflow that way).

**After ANY `rust/anzan` change, re-vendor the wasm:** `cd ts && npm run
build:wasm` — the script now rebuilds and vendors all three targets:
`ts/wasm/node`, `ts/wasm/web`, AND `site/src/wasm`. Skipping this leaves the
ts runner and the site REPL testing a stale binary of the old behavior.

## 4. Verify (see the `verify` skill for the full battery)

All THREE spec runners green — Swift (PickleKit), Rust (cucumber-rs), and ts
(cucumber-js via `cd ts && npm test && npm run spec`) — plus the site
Playwright smoke (`cd site && npx playwright test`). The scenario-count check
is three-way:

| Runner | Count rule |
|---|---|
| Swift | identical to Rust, risen by exactly your added scenarios |
| Rust | identical to Swift |
| ts | the language-subset count (lower by design); rises when you touch features it runs, never silently drops |

Clippy/fmt clean, and a **differential CLI check**: run the same input
through both binaries (`swift/Engine/.build/debug/soroban` vs
`cargo run -q --bin soroban`) and byte-compare. Stdout/stderr interleave
differently when piped — compare sorted before calling a diff real.

## 5. Changelogs — three files, same commit as the code

| Change touches | Write under `## [Unreleased]` in |
|---|---|
| `spec/**` (shared behavior) | root `CHANGELOG.md` **and** both ecosystem files |
| `swift/**` only | `swift/CHANGELOG.md` |
| `rust/**` only | `rust/CHANGELOG.md` |

A real feature must NOT carry `[skip ci]` (it should release). Docs-only /
test-only / changelog-promotion commits touching release paths (`swift/**`,
`rust/**`, `spec/**`) MUST carry `[skip ci]`.

## 6. Docs

`docs/ANZAN.md` (the language spec — renders to the site's `/anzan` on
deploy) + `docs/MODES.md` for dialect changes + each ecosystem's docs. Sweep
for newly-stale claims (a "three modes" heading survived one pass).

## 7. Land

Commit (explicit paths — NEVER `git add -A` here; see memory), push branch,
PR. A `spec/**` merge releases BOTH tracks; afterward run the
`release-doctor` skill to confirm the tags actually cut.
