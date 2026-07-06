# The `soroban` CLI (Rust)

The `soroban` binary — the language without the app. Depends on the
[`anzan`](ANZAN.md) crate **only** (no sheet layer) plus `rustyline` for the
REPL; keep it plumbing-thin (any behavior worth testing belongs in the engine).
Port of `swift/Engine/Sources/SorobanCLI/main.swift` (LineNoise → rustyline).

The crate is package `soroban-cli`, binary `soroban` (`cli/Cargo.toml`
`[[bin]] name = "soroban"`).

## Three modes, chosen by invocation shape

```sh
soroban "0.1 + 0.2 == 0.3"     # one-shot: evaluate each argument
echo "sqrt(2)" | soroban       # pipe: evaluate each stdin line
soroban                        # REPL (stdin is a terminal)
```

One `Calculator` per invocation, so variables, `ans`, and user functions carry
across arguments/lines exactly like the app's log. The engine does all the work;
the CLI is argument plumbing + error presentation (the same column-accurate caret
the app renders).

## Modules

- `src/main.rs` — mode dispatch (args / piped stdin / TTY), the one-shot and
  line-per-line evaluators, and error rendering. Pipe mode prints plain output
  and exits `1` if any line failed.
- `src/repl.rs` — the interactive REPL on `rustyline`: ↑/↓ history persisted to
  `~/.soroban_history` (the **same file** the Swift CLI uses), tab completion and
  gray signature hints — both fed by the engine's own `completions`/docs seams
  (`Calculator` autocomplete + `FunctionDoc`), not reimplemented here.

## Tests

`cli/tests/smoke.rs` — smoke tests for the two non-interactive modes (args and
piped stdin); the REPL is not observable from an integration test. The autocomplete
and documentation logic these lean on is unit-tested in the `anzan` crate.

## See also

- [ARCHITECTURE.md](ARCHITECTURE.md) · [ANZAN.md](ANZAN.md) — the crate it wraps.
- [../README.md](../README.md) — build/install.
