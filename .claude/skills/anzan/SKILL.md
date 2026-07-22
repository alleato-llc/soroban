---
name: anzan
description: Writing, running, and testing Anzan code — the exact-arithmetic language behind Soroban. Use when authoring .anzan scripts or examples, evaluating expressions, adding/spec'ing language features, or debugging why an Anzan expression misbehaves.
---

# Anzan — using and writing the language

Anzan is Soroban's exact-decimal expression language (50 significant digits;
`+ − ×` and integer `^` exact). Two implementations must stay in lockstep:
Swift (`swift/Engine/Sources/Anzan/`, the reference) and Rust (`rust/anzan/`).

## Where truth lives (read before guessing syntax)

- **The spec prose**: `docs/ANZAN.md` (+ `docs/MODES.md` for dialects).
- **The executable truth**: `spec/anzan/*.feature` — every user-visible
  behavior as scenarios, run by BOTH engines. When unsure how something
  behaves, grep these features first; they never lie.
- **Parity rule** (load-bearing): a language change = `spec/` feature edit
  **plus both engines**, in that order. Never diverge one engine.

## Evaluate something (fast loop)

```sh
# Swift CLI (build once):
cd swift/Engine && swift build --product soroban
printf 'x = 3\nx^2 + 1\n' | .build/debug/soroban       # pipe, line/statement per line
.build/debug/soroban "0.1 + 0.2 == 0.3"                # one-shot args, shared session
.build/debug/soroban file.anzan                         # script file (halts at first error)
# Rust CLI:
cd rust && cargo run -q --bin soroban                   # same four modes
# Mode-dependent behavior: put ':mode finance' (or programmer) on its own line first.
```

Scripts: one statement per **logical line** — an open `( [ {` continues the
statement onto the next line (that's how pretty multi-line `namespace` blocks
work). `#!/usr/bin/env soroban` + `chmod +x` makes a `.anzan` executable.

## Language cheat sheet

```
x = 3                      # variables; `ans` is the previous result
f(x) = x * 2               # functions; recursion works: fact(n) = if(n <= 1, 1, n * fact(n - 1))
if(cond, a, b)             # lazy — only the taken branch evaluates (safe for recursion)
5%                         # percent literal → 0.05 (postfix, exact)
mod(a, b)                  # modulo in normal/finance (the % glyph is percent there)
data Pt { x: Number, y: Number }      # record type; construct Pt(x: 3, y: 4) / Pt(3, 4)
p.x                        # field access; types: Number, String, Boolean, [T], nested data
namespace Geo { data Pt {…}; dist(p: Pt) = … }   # members separated by `;`, use Geo::dist, or `import Geo`
map(x -> x^2, [1, 2, 3])   # lambdas + higher-order fns; sum/len/max/… flatten arrays
man name                   # docs for anything; a trailing `# comment` on a definition IS its doc
```

Modes (`:mode …`) are *display/input dialects* — stored formulas stay canonical:
- **finance**: `$10`, `€10` (currency is a real type — `Money(10, "USD")` is
  the canonical, any-mode constructor; mixing currencies errors; `$9%` errors)
  and `138,561` thousands grouping (presentation-only, echoes through math).
- **programmer**: `^`=XOR, `& | << >> ~` bitwise, `%`=modulo, `0xFF`/`0b1010`.

## Gotchas that cost real debugging time

- **Builtins can't be shadowed — and calls resolve to the builtin silently.**
  Defining `count(c, d) = …` errors, but if you miss the error, later calls
  hit the BUILTIN `count` and return plausible nonsense. Check a name with
  `man <name>` before using it for a user function (this bit us: use `coins`).
- **`,` is the argument separator inside `(` `[` `{` of a call/literal** —
  `max(138,561)` is two args even in finance mode; grouping only lexes at top
  level or inside a bare (non-call) paren: `($15,000 * 5%)` groups.
- **Two result renderings.** `description` is canonical/re-parseable
  (`Money(10, "USD")`, `Int8(8)`, `Decimal(0.50, 2)`) — it's what persists and
  what the spec step `the result is` asserts. `displayDescription` is the
  human echo (`$10.00`, `343353`) — asserted by `the log echoes`. Don't mix
  them up in scenarios.
- **Cells are scalar**: a cell can't hold a record/array/function — reference
  a field (`=changeFor('Cost').nickels`) or aggregate. The log can hold them.
- **`1.5` yes, `2e` no, `1_000` yes** — and a statement must close its
  brackets by end of input or it's an "unterminated" error.
- **Constructor-built values (`Decimal`/`Int32`/`Money`) don't survive
  workbook *variable* persistence** (restore literal-folds, calls excluded);
  cells are fine (they re-evaluate source). Known, type-wide.

## Testing a language change

```sh
cd swift/Engine && swift test          # unit + ALL shared scenarios (PickleKit)
cd rust && cargo test -p soroban-engine --test gherkin   # same features via cucumber
```
Check the **scenario count** in the output, not just green — a vacuous run
looks identical to a real pass (it happened). Steps live in
`swift/Engine/Tests/SorobanEngineTests/SorobanSteps.swift` and
`rust/engine/tests/gherkin.rs`; multi-line programs use the
`When I run the script:` docstring step. For CLI-level checks, byte-compare
the two binaries' output on the same input.
