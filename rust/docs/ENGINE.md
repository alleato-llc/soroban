# The `soroban-engine` crate (hosting layer)

Sheets, cells, formats, controls, named cells, worksheets, the `.soroban` codec,
and workbook/history reflection. `pub use`s the `anzan` crate, so depending on
`soroban-engine` gives the whole engine.

> **Status:** skeleton — full content lands in Phase 2, documenting the crate's
> module layout after the recent split (`sheet_store/{mutation,resolvers}`,
> `spreadsheet/{definitions,evaluation}`, `reference_rewriter`, `structure`,
> `data_store`, `reflection`, `history_reflection`, `workbook`, `package`,
> `journal`, `named_cells`, `cell_format`, `csv`).

## See also

- [../../docs/FORMAT.md](../../docs/FORMAT.md) — the `.soroban` interchange format.
- [ANZAN.md](ANZAN.md) — the language crate · [ARCHITECTURE.md](ARCHITECTURE.md).
