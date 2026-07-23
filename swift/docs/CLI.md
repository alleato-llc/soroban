# The `soroban` CLI (Swift)

`Engine/Sources/SorobanCLI/main.swift` — the language at the command line, the
same engine that powers the app with no sheet layer. One `Calculator` per
invocation; the mode is chosen by invocation *shape*.

## Build & install

```sh
cd swift/Engine && swift build -c release --product soroban
install -m 755 .build/release/soroban ~/.local/bin/
```

The `soroban` executable is a product of the `Engine` package
(`Package.swift`); the app never builds it.

## Dependencies & scope

- Depends on **`Anzan` only** (not `SorobanEngine`) — the CLI is the language
  without the app, and deliberately has no grid, cells, workbooks, or
  reflection. `Workbook` and `cell()` are simply unknown names here.
- The `linenoise-swift` dependency (the REPL line editor, imported as
  `LineNoise`) is declared **CLI-target-only** and pinned to a *commit* (its
  newest tag predates Swift 5). The `Anzan` library target must stay
  BigInt-only.
- **Keep it plumbing-thin.** Any behavior worth testing belongs in the engine —
  the CLI target is excluded from the coverage report for exactly that reason.

## Four modes, by shape

`main.swift` picks the mode from how it was invoked; one `Calculator` carries
`ans`, variables, and user functions across all inputs in a run:

| Invocation | Mode | Behavior |
|---|---|---|
| `soroban "x = 3" "x^2 + 1"` | one-shot args | evaluate each argument in one shared session |
| `soroban change.anzan` | script file | a `.anzan` argument runs as a script — statement-aware, **halts at the first error** (`at file:line`, exit 1); mixes with expression args in order |
| `echo "sqrt(2)" \| soroban` | piped stdin | evaluate each statement; plain output; exit 1 if any line failed |
| `soroban` (tty) | REPL | linenoise editor; `exit`/`quit`/⌃D to leave; `… ` continuation prompt while a bracket is open |

**Statements are logical lines** (`StatementAccumulator` in the engine — the
same primitive SDK embedders use): a statement ends at a newline unless a
`(` `[` `{` is still open, in which case following lines JOIN into one logical
line — so a pretty-formatted `namespace { … }` block pipes, runs from a file,
and pastes into the REPL. The first line's trailing `#` comment survives the
join (multi-line definitions stay documented); an unclosed block at EOF is a
loud "unterminated" error. A `#!/usr/bin/env soroban` shebang line is an
ordinary comment, so `chmod +x` makes a `.anzan` file directly executable.
Script files don't echo comment-only lines (pipes still do).

`-h`/`--help` prints usage; `--version` prints the CLI version.

## REPL affordances

Built from the same engine seams the app uses, so behavior matches:

- **Tab completion** via `Calculator.completions(forPrefix:)` +
  `trailingIdentifier(of:)`.
- Gray `name(` **signature hints** from `FunctionDoc`.
- **↑/↓ history**, persisted to `~/.soroban_history`.
- `:mode` switches the presentational dialect (normal/programmer/scientific;
  `:mode scientific eng` selects the engineering echo) for the REPL and pipe —
  parsed by the engine's shared `Calculator.setMode(parsing:)` seam, so the
  errors match the app's; see [MODES.md](../../docs/MODES.md). In programmer
  mode a leading binary operator on an empty line is `ans`-prefixed
  (SpeedCrunch-style). Results echo through
  `EvalOutcome.displayDescription(mode:style:)`, so scientific mode shows
  `2.46912e5` (or `246.912e3` under eng) while Money/grouped keep their own
  display.

## Error rendering

Errors render the **same column-accurate caret** as the app: the offending
line, a `^` under the error column (from `EngineError.position` — the same
offsets the engine hands every host), then the message. See `report(...)` in
`main.swift`.

## Documentation & comments

`man pmt` / `manual pmt` / `help pmt` (unix-style, no parentheses) print a
function's signature, summary, and examples into the output — built-ins,
special forms, and your own documented functions alike. In pretty mode a
trailing `# comment` on a calculation echoes after the result (display-only,
kept out of pipe output); comment-only lines are echoed, exit 0.

## See also

- [ARCHITECTURE.md](ARCHITECTURE.md) · [ENGINE.md](ENGINE.md) — the engine the
  CLI plumbs · [../README.md](../README.md) — build/install.
- [../../docs/ANZAN.md](../../docs/ANZAN.md) — the language ·
  [../../docs/MODES.md](../../docs/MODES.md) — the dialects `:mode` switches.
