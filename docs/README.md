# Soroban documentation

An index of the docs, grouped by **status** so it's clear what is canonical
language, what is an implemented companion spec, what is a deferred idea, and
what is process. Only [`ANZAN.md`](ANZAN.md) is rendered on the site
(soroban.alleato.dev/anzan); everything else is repo-only.

## The language spec

- **[ANZAN.md](ANZAN.md)** — the canonical Anzan language specification
  (§1–§14 + grammar appendix). Every doc below is a companion to one of its
  sections.

## Implemented language features (companion specs)

These are shipped and tested; each expands a section of `ANZAN.md`.

- **[MODES.md](MODES.md)** — input/display dialects (Normal / Programmer /
  Finance) over one canonical AST. *(ANZAN §4)*
- **[FIXED-WIDTH.md](FIXED-WIDTH.md)** — bounded, *checked* `Int` / `UInt`
  integer types. *(ANZAN §2)*
- **[DECIMAL.md](DECIMAL.md)** — fixed-precision `Decimal(value, precision,
  scale)`, the money type. *(ANZAN §2)*
- **[MODULES.md](MODULES.md)** — namespaces, imports, and generic `data`
  fields. *(ANZAN §9)*
- **[PROGRAMMER.md](PROGRAMMER.md)** — the Programmer dialect and the binary
  bit editor. (The dialect is language — also in `MODES.md`; the bit editor is
  an app feature, now the shared `BinaryEditorKit` package.) *(ANZAN §4)*

## Workbook format

- **[FORMAT.md](FORMAT.md)** — the `.soroban` package/file format. Not language;
  the container the host persists.

## Deferred / not planned

- **[STDLIB.md](STDLIB.md)** — a standard library of Anzan-written namespace
  modules. A design note, **captured for later, not planned**.

## Process

- **[RELEASING.md](RELEASING.md)** — the gitflow + [salpa](https://github.com/alleato-llc/salpa)
  release process (auto-tagged semver on merge to `main`).
