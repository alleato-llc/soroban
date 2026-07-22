# Soroban

An exact calculator with a mini-spreadsheet attached — named for the Japanese
abacus (算盤). Type expressions into an input line and results accumulate in a
scrolling log, or flip to a grid where cells hold text, numbers, and formulas
that reference each other (`B:1 + B:2`) — and save the whole thing as a
`.soroban` workbook.

Built on an arbitrary-precision decimal engine — `0.1 + 0.2` is exactly `0.3`,
and money math never picks up binary floating-point drift.

The expression language is named **Anzan** (暗算 — "mental calculation", the
discipline of computing on a soroban you only imagine): variables, custom
functions with recursion and doc comments, lambdas with `map / filter / reduce`,
arrays/maps/strings, typed `data` records with `toJson`/`fromJson`, lazy `if()`,
LaTeX-style `∑`/`∏` — every value exact. The full specification is
[docs/ANZAN.md](docs/ANZAN.md), and its promises are executable
(`spec/anzan/anzan.feature` pins the grammar in CI).

## A monorepo of two implementations

Soroban is **ecosystem-first**: two independent implementations of the same
Anzan language and `.soroban` formats, held to one shared behavior spec. Start
with **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** for the common design.

| | |
|---|---|
| **[swift/](swift/README.md)** | Everything Apple — the engine, the macOS + iPad app, the `soroban` CLI, and BinaryEditorKit. This is the shipping product. |
| **[rust/](rust/README.md)** | A second implementation on [iced](https://iced.rs) + rime — the `anzan`, `soroban-engine`, and `soroban` CLI crates (complete, parity-tested) plus the `gui` desktop app. |
| **[ts/](ts/README.md)** | The `@alleato/anzan` TypeScript package — the verified Rust engine compiled to WebAssembly (via `rust/wasm`), for Node and the browser; powers the site's live REPL. A binding, not a third implementation. |
| **[spec/](spec/README.md)** | The shared Gherkin behavior spec — the cross-ecosystem parity oracle both implementations must keep green (run by Swift, Rust, and the TS/WASM binding). |
| **[docs/](docs/README.md)** | Shared language, format, and design docs. |
| **[site/](site/README.md)** | The landing page (Astro, static) + the living spec/report. |

Contributing? See **[CONTRIBUTING.md](CONTRIBUTING.md)**.

## Get it

[**Download Soroban**](https://github.com/alleato-llc/soroban/releases/latest/download/Soroban.dmg)
(signed & notarized macOS app), or grab a specific version from
[Releases](https://github.com/alleato-llc/soroban/releases); open the dmg and
drag Soroban to Applications. Every merge to `main` ships a release automatically
— see [docs/RELEASING.md](docs/RELEASING.md).

The full Anzan language also ships as a CLI (no app, identical 50-digit
arithmetic). To build the app or the CLI from source, see
[swift/README.md](swift/README.md); for the Rust implementation,
[rust/README.md](rust/README.md).

A tour of the app's features lives in [swift/docs/APP.md](swift/docs/APP.md).

## Roadmap

In rough order: **unit & currency support** (`10 USD + 5 EUR`, `3h + 20min`),
**linked data sources** (a data sheet that references an external CSV in place —
read-only, chain-link badge, re-read on open — vs. today's import-as-copy),
Excel-style **array spilling** into neighboring cells, Finder double-click for
`.soroban` files + recent-files menu, a grid-mode formula bar, true
arbitrary-precision transcendentals, and a CLI for running workbooks headlessly.

## License

[MIT](LICENSE) © Alleato LLC.
