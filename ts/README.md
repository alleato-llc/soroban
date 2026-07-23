# @alleato/anzan (TypeScript)

Anzan — Soroban's exact calculation language — for JS hosts: the Rust engine
([`../rust/anzan`](../rust/anzan)) compiled to WebAssembly
([`../rust/wasm`](../rust/wasm)), wrapped in a typed TypeScript SDK plus the
fourth Anzan CLI (after the Swift and Rust `soroban` binaries and the apps).
The language itself is [docs/ANZAN.md](../docs/ANZAN.md); nothing is
reimplemented here — the wasm binding is thin (one boundary crossing per
statement, JSON strings across it) and this package parses that JSON once into
typed values.

There is **no hosting layer**: no sheets, cells, workbooks, or persistence —
the language without the app, exactly like the native CLIs.

## Layout

- `src/index.ts` — the SDK: `Calculator` (stateful sessions: `ans`, variables,
  user functions, and the mode persist), `StatementAccumulator` (the streaming
  logical-line splitter), `runScript`/`statements` helpers, and the typed
  outcome unions (`EvalOutcome`, `AnzanError`, `ScriptResult`, …).
- `src/backend.ts` — the `EngineBackend` seam. Today the wasm backend is the
  only (and default) implementation; a future pure-TS engine fills the same
  slot.
- `src/cli/anzan.ts` — the `anzan` CLI (contract: [swift/docs/CLI.md](../swift/docs/CLI.md)
  / [rust/docs/CLI.md](../rust/docs/CLI.md)).
- `src/spec/steps.ts` + `cucumber.mjs` — the shared-spec runner (below).
- `wasm/node`, `wasm/web` — the **vendored** wasm-pack builds (committed:
  `npm install` and CI never need a Rust toolchain). The vendor step also
  refreshes the site's copy (`../site/src/wasm`).

## Install & build

```sh
npm install
npm run typecheck
npm test            # vitest — the binding surface
npm run spec        # cucumber-js — the shared language spec (below)
npm run build       # tsc → dist/ (the publishable SDK + CLI)
```

### Rebuilding the wasm (only after changing the Rust engine)

```sh
npm run build:wasm
```

Builds `../rust/wasm` with wasm-pack for both targets (`--target nodejs` →
`pkg/`, `--target web` → `pkg-web/`, both gitignored) and vendors the four
artifacts into all THREE locations — `wasm/node/`, `wasm/web/`, and the site's
REPL island (`../site/src/wasm/`) — **commit the result**.
`wasm/node/package.json` (`{"type":"commonjs"}`) is written by the vendor step:
the nodejs-target output is CJS and this package is ESM.

## Embedding

```ts
import { Calculator } from "@alleato/anzan";

const calculator = new Calculator();
calculator.evaluate("0.1 + 0.2 == 0.3");
// { ok: true, kind: "value", description: "1", displayDescription: "1" }

const outcome = calculator.evaluate("$10 * 5%"); // currency is core grammar
if (outcome.ok) {
  outcome.displayDescription; // "$0.50"        — the human echo
  outcome.description;        // 'Money(0.5, "USD")' — canonical, re-parseable
}

calculator.mode = "scientific";          // plain numbers echo scientifically
calculator.evaluate("123456 * 2");       // displayDescription: "2.46912e5"
calculator.sciStyle = "eng";             // …or engineering: "246.912e3"

calculator.runScript("x = 2\nx * 3");   // halts at the first error, like .anzan
calculator.completions("sq");           // [{ name: "sqrt" }]
calculator.documentation("pmt");        // { signature, summary, examples }
```

Errors come back as values (`{ ok: false, error, position? }`), never throws;
`position` is the character offset every host renders a caret under.

## CLI

Four modes, chosen by invocation shape — the same contract as the native
`soroban` binaries:

```sh
npm run anzan -- "0.1 + 0.2 == 0.3"    # one-shot args, one shared session
npm run anzan -- change.anzan          # script file: halts at the first error
echo "sqrt(2)" | npm run anzan         # statement-aware pipe, continue-on-error
npm run anzan                          # REPL: > prompt, … continuation, :mode
```

(After `npm run build`, the `anzan` bin in `dist/cli/` does the same.) Pretty
TTY output echoes `= result  # trailing-comment`; script files halt with the
failing statement, a `^` caret at the error position, and `at file:line`,
exit 1; pipes keep going and exit 1 if any statement failed; `:mode
normal|programmer|scientific [eng]` switches the dialect everywhere (parsed by
the engine's shared `:mode` seam, so the mode list and errors match the native
CLIs exactly).

## The shared spec

`npm run spec` runs the SHARED [`../spec/anzan`](../spec) feature files — the
same files the Swift (PickleKit) and Rust (cucumber-rs) suites run, by path,
never a copy — with steps mirroring `rust/engine/tests/gherkin.rs`. One fresh
`Calculator` per scenario. This package has no hosting layer, so only the
pure-language features are wired up; features needing cell/sheet/workbook
steps cannot run here:

| Feature | Scenarios | Status |
|---|---|---|
| calculation.feature | 25 | ✅ runs |
| decimal.feature | 20 | ✅ runs |
| fixedwidth.feature | 15 | ✅ runs |
| functions.feature | 16 | ✅ runs (1 wasm-excluded, below) |
| mathematics.feature | 108 | ✅ runs |
| modes.feature | 61 | ✅ runs |
| scripting.feature | 8 | ✅ runs |
| structures.feature | 27 | ✅ runs |
| anzan.feature | 48 | ⛔ host-excluded — `cell A:1 contains` (pinned cell references) |
| datatypes.feature | 55 | ⛔ host-excluded — cell-scoped `data` declarations |
| formatting.feature | 15 | ⛔ host-excluded — cell formats (`is formatted as` / `displays`) |
| library.feature | 114 | ⛔ host-excluded — `the sheet contains:` (sheet-fed statistics) |
| modules.feature | 33 | ⛔ host-excluded — `the workbook is saved and reopened` |
| reflection.feature | 13 | ⛔ host-excluded — sheets, cells, and workbook reflection |
| spreadsheet.feature | 21 | ⛔ host-excluded — the grid itself |

Current run: **279 scenarios, 279 passed** (the 280th included scenario is the
wasm exclusion below). The numeric-nearness steps (`the result is within …`)
re-enter the engine (`abs((value) - (target)) <= bound` in a fresh session)
instead of parsing floats, so they stay exact at all 50 digits.

### The one wasm-excluded scenario

`functions.feature` — “Deep recursion is bounded by memory, not a counter”.
The native engines grow the stack for deep non-tail recursion (Rust `stacker`,
Swift `continueOnFreshStack`); wasm has no growable stack, so the engine's
wasm build enforces a recursion depth cap instead and answers with its clean
`function calls nested too deeply` error — never a raw `RangeError` escaping
the boundary (pinned by `src/index.test.ts`). Tail recursion is unaffected
(real TCO — constant stack at any depth, spec-covered).

## Wasm notes

- Size: **~620 KiB** per target (`anzan_wasm_bg.wasm`, 634,640 bytes;
  `opt-level = "s"` + LTO + wasm-opt), vendored twice (`wasm/node`,
  `wasm/web`).
- The web build (`wasm/web`) is the browser/bundler flavor (ESM,
  `init()`-style loading); the SDK itself loads `wasm/node` lazily via
  `createRequire`, so importing the package never fails — a missing build
  throws an actionable error naming `npm run build:wasm`.

## Versioning

Starts at **0.1.0**; npm-ready but not yet published. Changes:
[CHANGELOG.md](CHANGELOG.md).
