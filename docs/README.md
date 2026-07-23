# Soroban documentation

The `docs/` folder holds the **shared** material — the high-level design, the
canonical Anzan language spec, the interchange formats, and process — that both
the Swift and Rust ecosystems honor. Ecosystem-specific docs live under each
ecosystem (see [Per-ecosystem docs](#per-ecosystem-docs)).

Only [`ANZAN.md`](ANZAN.md) is rendered on the site
(soroban.alleato.dev/anzan); everything else is repo-only.

## Design

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — the common design: the two separated
  layers (language vs host), the ecosystem-first monorepo, the shared-spec
  parity model, and the interchange contracts. Start here.
- **[MIGRATION.md](MIGRATION.md)** — the decision record + phased plan for the
  modular monorepo and the Rust port. The historical "why the layout is this
  way."

## The language spec

- **[ANZAN.md](ANZAN.md)** — the canonical Anzan language specification
  (§1–§14 + grammar appendix). Implementation-agnostic; every doc below is a
  companion to one of its sections.

### Companion specs (shipped language features)

Each expands a section of `ANZAN.md`. These describe the *language*, so they are
shared; implementation details live in the ecosystem docs.

- **[MODES.md](MODES.md)** — input/display dialects (Normal / Scientific /
  Programmer) over one canonical AST. *(ANZAN §4)*
- **[FIXED-WIDTH.md](FIXED-WIDTH.md)** — bounded, *checked* `Int` / `UInt`
  integer types. *(ANZAN §2)*
- **[DECIMAL.md](DECIMAL.md)** — fixed-precision `Decimal(value, precision,
  scale)`, the money type. *(ANZAN §2)*
- **[MODULES.md](MODULES.md)** — namespaces, imports, and generic `data`
  fields. *(ANZAN §9)*
- **[PROGRAMMER.md](PROGRAMMER.md)** — the Programmer dialect and the bit-field
  format model. (The dialect is language; each app's bit-editor *UI* is
  documented in its ecosystem docs.) *(ANZAN §4)*
- **[STDLIB.md](STDLIB.md)** — a standard library of Anzan-written namespace
  modules. A design note, **captured for later, not planned**.

## Interchange format

- **[FORMAT.md](FORMAT.md)** — the `.soroban` package/file format. Not language;
  the container both ecosystems persist and exchange.

## Process

- **[RELEASING.md](RELEASING.md)** — the gitflow + [salpa](https://github.com/alleato-llc/salpa)
  release process (two ecosystem tracks, auto-tagged semver on merge to `main`).
- **[../CONTRIBUTING.md](../CONTRIBUTING.md)** — how to build, test, and land a
  change across the ecosystems.

## Per-ecosystem docs

The shared docs above cover the language and formats. For how a specific
implementation is built and structured:

- **Swift / macOS + iPad** — [../swift/README.md](../swift/README.md) and
  [../swift/docs/](../swift/docs/).
- **Rust** — [../rust/README.md](../rust/README.md) and
  [../rust/docs/](../rust/docs/).
- **Shared behavior spec** — [../spec/README.md](../spec/README.md).
