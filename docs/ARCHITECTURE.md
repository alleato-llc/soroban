# Soroban architecture — the common design

The design ideas that hold across the whole project, independent of which
implementation you're reading. Each ecosystem's docs (`swift/docs`, `rust/docs`)
describe how *that* implementation realizes them; this page is the shared mental
model. For the language itself, see [ANZAN.md](ANZAN.md).

## Two strictly separated layers

Every implementation is split the same way, and the boundary is enforced:

1. **The language** — **Anzan** (暗算, *mental abacus calculation*). Lexer →
   Pratt parser → evaluator, over an exact-decimal number type, fronted by a
   `Calculator` façade. Anzan knows **nothing** about grids or files: hosts wire
   cells and workbook reflection in through resolver closures. It is the same
   language in the app, the CLI, and the test suites.
2. **The hosting layer** — spreadsheets, cells, cell formats, controls, named
   cells, worksheets, and the `.soroban` persistence codec. It depends on the
   language and adds everything about *where values live*. Cell resolvers still
   speak the scalar number type; the evaluator wraps structured values.

A thin **CLI** sits directly on the language (no hosting layer), and a **GUI
app** sits on the hosting layer. The rule "don't let the language import the
sheet/persistence layer" is a module boundary, not a convention — a language
that stays host-agnostic is what lets one engine serve a REPL, a spreadsheet,
and a headless test runner unchanged.

### The exactness invariant

The number type is a big-integer significand × a power of ten, always
normalized. `+ − ×`, integer powers, and modulo are **exact**; division and
`sqrt` round to a configurable significant-digit precision (default 50, banker's
rounding). Transcendentals round-trip through a ~15-digit float fallback that is
deliberately confined to one seam, so a future arbitrary-precision upgrade
happens in one place. `0.1 + 0.2` is exactly `0.3`; money math never drifts.
This is a language-level promise, identical across ecosystems.

## Ecosystem-first monorepo

The repository is organized by *ecosystem*, with a shared core that neither
ecosystem owns:

```
soroban/
├── spec/    # THE shared behavior spec (Gherkin) — the parity oracle
├── docs/    # shared language/format/design docs (this file, ANZAN.md, …)
├── swift/   # everything Apple: engine, macOS/iPad app, CLI, Kit
├── rust/    # one cargo workspace: anzan, engine, cli, gui crates
└── site/    # the landing page + living spec/report (static)
```

Two independent implementations of the same language and formats live side by
side. They ship on their own release tracks and have their own changelogs
(`swift/CHANGELOG.md`, `rust/CHANGELOG.md`), while genuinely cross-cutting
changes land in the root `CHANGELOG.md`. The porting history and rationale is
[MIGRATION.md](MIGRATION.md).

## The shared-spec parity model

`spec/` holds Gherkin feature files that describe user-visible behavior once,
for both implementations. It is owned by neither ecosystem:

- A language or behavior change lands as a **feature-file edit plus both
  implementations**, in that order.
- Until one implementation catches up, the scenario is tagged
  (`@swift-only` / `@rust-pending`) so each runner skips it with visibility
  rather than failing.
- CI enforces both runners green on every untagged scenario.

The Swift side runs these features via PickleKit; the Rust side via cucumber.
The same `spec/anzan/*.feature` files are the executable companion to the
language spec, and `spec/anzan/anzan.feature` pins the grammar facts. See
[spec/README.md](../spec/README.md).

**Division of truth:** all user-visible input→output behavior lives in the
feature files. Ecosystem unit tests keep only what scenarios can't express —
typed-error equality (including positions), codec round-trips, dependency-graph
invalidation, resolver wiring, recursion/stack canaries.

## Interchange contracts (forever shared)

A workbook saved by either app opens in the other. Three contracts make that
true, and they are shared by construction:

- **The `.soroban` package format** — versioned JSON manifest + an optional
  `data.sqlite`. See [FORMAT.md](FORMAT.md).
- **CSV encode/parse** — a strict inverse: `parse(encode(rows)) == rows`.
- **Canonical source text** — the stored/replayed form of every expression is
  the language's normal-mode rendering, byte-identical across ecosystems.
  Presentational dialects (see [MODES.md](MODES.md)) never change what is stored.

Every format change adds a `spec/` round-trip scenario, so parity is tested,
not assumed.

## Where to go next

- **The language:** [ANZAN.md](ANZAN.md) and its companions (MODES, MODULES,
  FIXED-WIDTH, DECIMAL, PROGRAMMER).
- **The Swift/macOS ecosystem:** [../swift/README.md](../swift/README.md).
- **The Rust ecosystem:** [../rust/README.md](../rust/README.md).
- **The shared spec:** [../spec/README.md](../spec/README.md).
- **Releases & process:** [RELEASING.md](RELEASING.md).
