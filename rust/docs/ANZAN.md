# The `anzan` crate (language implementation)

The Rust implementation of the Anzan language: `lexer` → `parser` → `eval`
(evaluator + value + functions) + `number` (the exact BigDecimal). No grid or
file knowledge — hosts wire in cells and reflection through resolver closures.

> **Status:** skeleton — full content lands in Phase 2. This documents the
> crate's module layout after the recent split (`eval/evaluator/{calls,operators,
> recursion,resolution,values,helpers}`, `parser/{definitions,expressions,primary,
> references}`, `eval/binary_view/{layout,fields}`, `calculator/{host_seams,
> documentation}`, `eval/functions/*`). The language it implements is specified in
> [../../docs/ANZAN.md](../../docs/ANZAN.md).

## See also

- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language spec (not this crate).
- [ARCHITECTURE.md](ARCHITECTURE.md) · [ENGINE.md](ENGINE.md).
