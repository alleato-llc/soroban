# The Swift engine — Anzan + SorobanEngine

The `Engine` SwiftPM package: the `Anzan` language module (BigDecimal, Value,
lexer, Pratt parser, evaluator, functions, modes, fixed-width/decimal types,
reductions) and the `SorobanEngine` hosting module (spreadsheet, cells, formats,
controls, named cells, worksheets, workbook codec, reflection).

> **Status:** skeleton — full content lands in Phase 2. This will absorb the deep
> per-subsystem architecture from the root [../../CLAUDE.md](../../CLAUDE.md),
> rewritten to the post-refactor module directories (`Parser/`, `Eval/`,
> `Sheet/`, …) so the file references are current.

## See also

- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language spec these modules implement.
- [ARCHITECTURE.md](ARCHITECTURE.md) · [../CLAUDE.md](../CLAUDE.md).
