# Modes — input/display dialects over one language

> **Status: implemented (log-only, live-input).** The engine (mode-aware
> parser + per-mode renderer + the scientific echo), the CLI (`:mode`), and the
> app (Settings picker + input-bar affordance) are landed and tested;
> `modes.feature` pins every dialect's behavior. **One deliberate scoping:** the
> log shows existing entries *verbatim* (they're inert records — stored input +
> result text, never re-evaluated), so there is no correctness risk; the
> *uniform historical re-skin* described under "Switching modes" is **deferred**
> (it needs canonical storage of every entry plus recall/copy changes plus
> comment/error/def handling — disproportionate for a cosmetic effect on an
> inert tape). Fixed-width integer types are a *separate* feature — see
> `docs/FIXED-WIDTH.md`. The app's binary bit editor and bit-field formats that
> build on Programmer mode are covered in `docs/PROGRAMMER.md`.

## The one-sentence model

A **mode** is an input/display *dialect* — it changes which glyphs you **type**
and **read**, and nothing else. The stored program, the values, and the math are
identical in every mode. A programmer sees and types `5 ^ 3` for XOR; an analyst
sees and types `5 ^ 3` for *power*; the engine stores and computes neither glyph
— it stores `bitXor(5, 3)` or `pow(5, 3)`, canonically, forever.

This is the inverse of the design we explicitly **refused** (a mode that flips
what a *stored* symbol means — see "Rejected alternatives"). The refused version
stored the ambiguous surface text; this one stores the unambiguous canonical
form and treats the symbol as a skin. That distinction is the whole spec.

## The trio

Three modes — the standard multi-mode-calculator lineup (macOS, Windows, Casio):

- **Normal** *(default)* — **is** the canonical spelling: today's grammar,
  unchanged, the regression oracle.
- **Scientific** — Normal's grammar untouched; changes only how a plain
  *numeric result echoes*: scientific notation (`123456 * 2` → `2.46912e5`),
  or the **ENG** variant (`:mode scientific eng`) with the exponent snapped to
  a multiple of 3 (`246.912e3`).
- **Programmer** — `^ & | << >> %` read as XOR / AND / OR / shifts / modulo
  (Python precedence), `~` is bitwise NOT; power is written `pow(a, b)`.

**Finance mode is gone.** Its two literal forms earned a place in the *core*
grammar (below), which left the mode with nothing to say; `:mode finance` is
the ordinary unknown-mode error, with a hint that currency now works
everywhere.

## Core literals that used to be finance-mode

Currency and thousands grouping are **core grammar — every mode**. No existing
formula changes meaning: `$` before a *letter* is still the cell-reference
column pin (`$A:1`), and `,` is still the argument separator first.

| you type | any mode |
| --- | --- |
| `$10` | `$10.00` — a currency amount (`Money(10, "USD")` canonically) |
| `138,561` | `138,561` — grouped; canonically the plain `138561` |

**Currency is a first-class type** — a peer of `Int32(…)` and `Decimal(…)`.
Its canonical form is the constructor `Money(value, "CODE")`, and the literal
`$10` is sugar for it. A currency **literal** is one of a closed, curated set
of symbols directly before a number — `$` `€` `£` `¥` `₹` `₩` `₽` `₿` (`$`→USD,
`¥`→JPY canonically); an *unsupported* currency glyph is a loud lex error, and
currencies without an unambiguous glyph (CNY, CHF) are reachable through the
constructor. `$` before a *letter* is still the cell-reference column pin, so
`$A:1` and `$10` never collide.

The currency is part of the **value**, not just its rendering — it propagates
through arithmetic the way a `Decimal`'s type does, which is what makes
`$10 * 5%` answer `$0.50` (`5%` has already become a plain `0.05` by the time the
multiply sees it). A plain number is absorbed by the currency operand;
**two different currencies is a hard error** (there is no exchange rate to apply,
so guessing would be worse than refusing); `%` applied *to* a currency is a
category error. The currency survives all four operators, so a money input always
reads back as money — `$10 * $2` is `$20.00`. That is deliberate: the tag is a
*display contract*, not a unit system, so it never models dimensionality. Money
renders grouped at 2 decimals with the symbol outside the sign (`-$1,234.50`,
matching the sheet's currency format). `Money(v, "CODE")` recalls the type; the
echo (`$10.00`) is only what the log and CLI show.

**Thousands grouping** (`138,561`) is a *separate*, presentation-only concern —
NOT a type. `,` between digit groups (1-3 digits, then exactly-3-digit runs; a
malformed group like `1,23` is a loud lex error) marks a plain number as grouped;
it has no arithmetic rules and canonicalizes to the plain number, but it *echoes*
through a calculation so `138,561 * 9%` shows `12,470.49`.

**`,` is the argument separator first.** Grouping is suppressed inside a call's
argument list and inside `[…]`/`{…}` literals, so `max(138,561)` still means two
arguments — in every mode. A bare (non-call) paren re-enables it, so
`($15,000 * 5%)` groups.

## Using modes

### Switching

Both app entry points set the same per-session dialect (persisted like the theme):

- **App → Settings → Mode** (⌘,) — a Normal / Programmer / Scientific picker.
- **App input-bar affordance** — the small icon just left of the 📖 reference
  button (`#` Normal · `</>` Programmer · `π` Scientific); click it for a menu.
  It turns accent-colored whenever you're off Normal.
- **`:mode` command** — type `:mode programmer` (or `scientific` / `normal`)
  directly into the **app log** *or* the **CLI** REPL/pipe;
  `:mode scientific eng` selects the engineering echo. In the CLI a bare
  `:mode` prints the current dialect. In the app it logs a divider like any
  switch.
- **Embedders** — set `Calculator.mode` (and `Calculator.sciStyle`), or use
  `Calculator.setMode(parsing:)` — the one shared `:mode` argument parser, so
  every host errors identically.

Mode applies to the **log / input line only**; grid cells always use the
canonical (Normal) grammar (see *Scope*).

### Same keystrokes, different dialect

| you type | Normal / Scientific | Programmer |
|---|---|---|
| `5 ^ 3` | `125` (power) | `6` (XOR) |
| `5 & 3` | error — `&` is Programmer-only | `1` (AND) |
| `8 >> 2` | error — `>>` is Programmer-only | `2` (shift right) |
| `17 % 5` | error — that's `17%` then `5` (missing operator) | `2` (modulo) |
| `3%` | `0.03` (percent) | error — `%` is binary modulo here |
| `~UInt8(0)` | error — `~` is Programmer-only | `UInt8(255)` (bitwise NOT) |
| `pow(2, 3)` | `8` | `8` (functions work everywhere) |

Whatever a mode has **no glyph for is written longhand** as the canonical
function: power in Programmer is `pow(a, b)`; XOR in Normal is `bitXor(a, b)`;
modulo in Normal/Scientific is `mod(a, b)`. Nothing is ever unreachable — only
re-spelled.

### Scientific changes the echo, not the grammar

Scientific mode parses **exactly** like Normal — same glyphs, same precedence,
same errors. What changes is the *display* of a plain numeric result:

| result of | Normal echoes | Scientific echoes | `eng` echoes |
|---|---|---|---|
| `123456 * 2` | `246912` | `2.46912e5` | `246.912e3` |
| `5` | `5` | `5e0` | `5e0` |
| `1 / 8` | `0.125` | `1.25e-1` | `125e-3` |

The mantissa keeps the value's **own** significant digits — nothing is rounded
or padded (exactness is the language's first rule; formatting is pure
digit-string math, never floats). The **canonical** form stays the plain
number: `the result is`, recall, copy, and persistence all carry `246912`.

**Value-carried display wins.** Money still shows `$10.00`, a grouped number
still echoes `12,470.49`, strings/records/fixed-width values keep their own
rendering — only bare numeric results take the scientific echo.

**ENG** is a display *style* on the one `scientific` mode, not a fourth mode:
`Calculator.sciStyle` (`sci`|`eng`, default `sci`), set via
`:mode scientific eng`. The exponent snaps down to a multiple of 3 and the
mantissa shifts to match (1–3 integer digits).

### The `°` degree literal (mode-agnostic, showcased by Scientific)

`x°` is a postfix literal like `%`: it converts degrees to radians —
`x × π/180`, with π at the engine's 50-digit working precision — so
`sin(90°)` is `1` and `90° == pi / 2` holds exactly. It works in **every**
mode (no dialect owns another meaning for `°`); Scientific mode is simply
where a hand calculator's DEG habit makes it shine. The canonical AST node is
`.degrees(expr)`; it renders and re-parses as `x°` everywhere. The `rad(x)`
builtin computes the same thing longhand.

## Why this is allowed (the principle it must satisfy)

The governing rule (the "rand() principle", applied at the syntax level) is
**not** "no modes" — it is: *what is stored must mean the same thing under every
UI state.* A mode satisfies it iff:

1. **Canonical is the only thing stored or transported.** Cells, the log tape,
   the workbook codec, copy/paste, recall, and `man()` all carry the canonical
   spelling (`mod`, `bitXor`, `pow`, `bitShift`) and the canonical number
   (`246912`, never the `2.46912e5` skin). The dialect glyph exists only
   while you are typing into / reading from a surface.
2. **Every operation a mode lacks a glyph for is written longhand** (as the
   canonical function). A mode never *hides* an operation; it only *re-spells*
   the ones it owns a symbol for.

Together these make even the most dangerous overload — `^` as XOR vs. power —
safe by construction, because the glyph `5^3` is never the thing of record.

## Architecture: canonical store, presentational lens

```
   type "5 ^ 3"                         render under mode
        │                                      ▲
        ▼   parse-under-mode                    │  AST → skin · value → echo
   ┌─────────────┐   AST    ┌──────────────────────────────┐
   │ mode parser │ ───────▶ │  canonical AST (the truth)    │ ──┐
   └─────────────┘          │  e.g. .call("bitXor", 5, 3)   │   │ store / transport
                            └──────────────────────────────┘   │ (always canonical)
                                                                ▼
                                          log.json · workbook · copy · recall
```

- **Input** is mode-aware: the lexer/parser resolves overloaded glyphs and their
  binding powers per the active mode (this is the one part that is *not* free —
  see "What this costs").
- **Storage and transport** are canonical and mode-free. There is **no mode
  metadata anywhere** — not on entries, not in the codec.
- **Display** is a pure function of `(AST, mode)` for source text
  (`Expression.sourceText(mode:)`, with a **function fallback** for operations
  the mode doesn't own a glyph for) and of `(value, mode, style)` for result
  echoes (`EvalOutcome.displayDescription(mode:style:)` — the one seam every
  host renders through; Scientific lives entirely here).

Because the store carries no mode, **reload, replay, and undo are mode-free** —
there is nothing to disambiguate. (This is what makes tracking mode-switches
unnecessary; see "Rejected alternatives → per-segment mode tracking".)

## The dialects

`Normal` **is** the canonical spelling, so the stored form is exactly what
`Normal` renders — today's grammar, unchanged.

| operation | canonical | Normal / Scientific | Programmer |
|---|---|---|---|
| power | `pow(a,b)` | `a ^ b` | `pow(a,b)` |
| XOR | `bitXor(a,b)` | `bitXor(a,b)` | `a ^ b` |
| AND | `bitAnd(a,b)` | `bitAnd(a,b)` | `a & b` |
| OR | `bitOr(a,b)` | `bitOr(a,b)` | `a \| b` |
| shift L/R | `bitShift(a,n)` | `bitShift(a,n)` | `a << n` / `a >> n` |
| modulo | `mod(a,b)` | `mod(a,b)` | `a % b` |
| percent | `.percent` node | `x%` | `x * 0.01` |
| degrees | `.degrees` node | `x°` | `x°` |
| concat | `concat`/`+` | `a + b` | `a + b` |

The point of the table: **the same glyph (`^`, `&`, `%`) is owned by a different
operation per mode, and whatever a mode doesn't own falls back to the canonical
function.** So in Programmer mode `pow` has *no* infix glyph and renders as
`pow(2,3)`; in Normal mode `bitXor` renders as `bitXor(5,3)`. You can never look
at a glyph and be unsure — in your current mode it means exactly one thing, and
the meanings it lacks are spelled out. (Scientific shares Normal's column: it
is a *display* dialect over results, not a glyph dialect over input.)

Notes that fell out of the design walk-through:

- **`pow` is the one builtin modes added** (power as a function, for when `^`
  is taken by XOR). `mod`, `bitAnd/Or/Xor`, `bitShift`, and `concat` already
  exist; `+` already concatenates when either side is a string.
- **`bitShift(a, n)`** already encodes both directions (`n > 0` left, `n < 0`
  right), so Programmer `a << n` / `a >> n` both canonicalize to it (`>>`
  negates `n`).
- **Percent is *not* a function.** A typed `x%` is the `.percent` postfix node,
  rendered `x%` in Normal/Scientific and `x * 0.01` in Programmer (where `%` is
  taken by modulo). `3% == 0.03 == 3*0.01`, exact, re-parseable in any mode — so
  no `pct` builtin is confiscated. Percent and modulo never collide because they
  have **distinct canonical forms** (`.percent` vs `mod`); they merely reuse the
  `%` glyph. One wrinkle: *editing* a percent line while in Programmer mode edits
  `x * 0.01`, which re-parses as multiplication — same value, lost percent-ness.
  Viewing-and-switching-back is lossless (viewing never touches the AST).
- **Degrees (`°`) is not overloaded** — every mode renders `.degrees` as `x°`,
  so it needs no fallback row logic; it's listed for completeness.
- **`&` is Programmer-only AND.** Concatenation stays `+` (string-aware) and
  `concat()` in every mode; the long-reserved "`&` for Excel-style concat" plan
  is dropped, since `+` already covers it. In Normal/Scientific, `&` is a loud
  mode-scoped error (see E3).
- **Bitwise NOT (`~`)** ships with **fixed-width integers** — see
  `docs/FIXED-WIDTH.md`, where width is well-defined.

## Precedence (Python-style)

Overloaded glyphs use **mode-native precedence**, so the Pratt binding powers are
parameterized by mode for those tokens. We follow **Python's** ordering, not
C/Java's — because Anzan's audience is the scripting/calculator world (where
Python is the lingua franca), and because Python *fixes* C's two footguns
(`a & b == c` parsing as `a & (b == c)`; `a << b + c` as `a << (b+c)` is kept,
but the bitwise/comparison trap is removed by lifting the whole bitwise band
above comparison). Programmer mode, tight → loose:

```
postfix · unary · (* / mod) · (+ -) · (<< >>) · & · ^ · | · comparison · lambda · assignment
```

So bitwise binds *below* arithmetic (compute the numbers, then combine bits) but
*above* comparison (no `& ==` trap), and AND-before-OR holds (`&` tighter than
`|`). In Normal/Scientific, `^` keeps its tight, right-associative power slot and
the bitwise functions are ordinary calls (primary level); `%` is the existing
tight postfix, and `°` chains at the same postfix level.

This means `a ^ b == c` parses to different ASTs in Programmer (XOR, above
comparison) vs Normal (power) — correct, because they're different dialects.
The ASTs are unambiguous once formed; only the parse is mode-sensitive.

## Enforcement

What actively prevents a wrong meaning — by construction, not discipline:

- **E1 — canonical is the stored truth.** The dialect glyph is never persisted,
  so it can never be re-read under a different meaning.
- **E2 — function fallback on render.** A glyph only ever renders for its mode's
  meaning; the other meanings render as their canonical function.
- **E3 — out-of-mode glyphs are loud, never silent.** Typing `5 << 2` in Normal
  is a *mode-scoped parse error* — "`<<` is a Programmer-mode operator; use
  `bitShift(5, 2)`" — not a misparse. (`<<` and `&` aren't operators outside
  Programmer at all.)
- **E4 — transport emits canonical.** Copy, recall, insert, and cell-commit all
  produce the canonical spelling, so a line lifted out of context (into a cell, a
  bug report, a chat) is unambiguous. Recall a Programmer `5 ^ 3` anywhere → it
  lands as `bitXor(5, 3)`; recall a Scientific `2.46912e5` → it lands as
  `246912`.

## Switching modes

Switching is **change the live mode, then re-render the visible surface**. There
is no migration of stored data — the store was canonical all along.

- **Lossless in meaning and value, always.** `render` is a pure function of
  `(AST, mode)` and the AST is invariant across switches, so
  Normal → Programmer → Normal returns identical text by construction. The
  numbers never move.
- **Uniform, not per-segment.** The *whole* visible tape (and visible grid, if
  in scope) re-skins to the current dialect — one consistent reading surface,
  not a patchwork of "the dialect each line was typed in". *(Deferred — see
  the status note at the top; today existing entries show verbatim and the mode
  applies to live input only.)*
- **Text normalizes.** Re-deriving the skin from the AST loses incidental
  formatting (extra spaces, `1_000` vs `1000`). Trailing `# comments` survive;
  other literal trivia normalizes — the same way every spreadsheet reformats a
  formula regardless of how you typed it.

Cost of a switch is negligible: the log is capped at 500 entries and the grid
re-skins only *visible* cells (lazy); re-rendering a few hundred short ASTs is
sub-millisecond. No per-keystroke cost — one entry renders when added, one cell
when shown.

## Scope: log-only first, workbook-wide later

**Today — log-only.** The log is a mode-switchable *calculator* (a physical
calc's DEC/HEX/DEG toggle); the grid stays canonical (`Normal`) — the familiar
spreadsheet. Smallest blast radius; cells and the codec are untouched. Mode is
set like theme — a UI toggle (`:mode` in the CLI), persisted in UserDefaults,
never in a workbook.

- The only quirk: `^` in the log (Programmer) ≠ `^` in a cell (`Normal`/power)
  *at the same time*. This is a cognitive seam, not a correctness bug — the log
  reads cells **by value**, never by their text, so a mode on one surface can
  **never corrupt** the other. (A Programmer log can even echo a currency cell's
  value in hex; what crossed the boundary was a `BigDecimal`, formatted
  independently on each side.)

**Later — workbook-wide.** The grid follows a workbook-level mode; one dialect
everywhere, no seam. This requires: cell storage moves fully to canonical (the
displayed raw becomes a derived skin, normalizing trivia on every render), and
the workbook codec gains a `mode` field (back-compat default `Normal`,
`FORMAT.md` bump). A clean extension once the log-only seam proves either
acceptable or annoying.

## What this costs

The simplification (canonical store + live lens) **eliminates** the riskiest
machinery — no mode metadata, no per-entry mode, no mode-change tracking, no
replay/undo threading. What **remains** is intrinsic to "type and read your
dialect":

| | eliminated | required |
|---|---|---|
| storage / transport | ✅ canonical only, no mode metadata | — |
| switch / reload / replay | ✅ mode-free; re-render visible only | — |
| **parser** | — | ❌ mode-aware tokens + per-mode binding powers |
| **render** | — | ❌ per-mode skin table + function fallback + the sci/eng echo |
| **new builtins** | — | ❌ `pow(a,b)` only |
| **tests** | — | ❌ round-trip `render(parse(x, m), m) == x` per mode |

The expensive-but-bounded part is the mode-aware Pratt parser and the render
table. The part one might fear — tracking switches, persistence, replay fidelity
— is **zero**, because nothing mode-relative is ever stored.

## Rejected alternatives

- **Semantic modes (flip what a *stored* symbol means).** Storing `5 % 4` and
  reinterpreting it per mode. Violates the principle directly: the stored text
  floats in meaning, naked extraction is ambiguous, and `^`-as-XOR "silently
  computes the wrong number" — the exact failure refused in CLAUDE.md. Replaced
  by canonical storage + skin.
- **Per-segment mode tracking** (a `mode` field per log entry, or mode-change
  marker entries). Necessary *only if* the store held mode-relative surface text.
  With a canonical store it buys nothing for reload or replay (both are
  mode-free), and its one effect — a tape that displays each line in its
  authoring dialect — is a confusing patchwork rather than a feature. Dropped.
- **Canonical-only input** (type `bitXor(5,3)`, merely *see* `5 ^ 3`). Throws
  away the entire ergonomic point — typing your dialect — keeping only a display
  reskin. The mode-aware parser is what earns the feature.
- **C/Java bitwise precedence.** Reproduces the `a & b == c` footgun; Python's
  ordering fixes it and matches the audience. (Go's "fold bitwise into the
  arithmetic tiers" was the runner-up — cheaper to implement, but Python's
  explicit band is more teachable for a calculator.)
- **A mode that hides/removes operations.** A mode only re-spells; it never makes
  an operation unreachable. Everything is always available as its canonical
  function.
- **A finance mode.** Shipped, then retired: once currency (`$10`) and
  grouping (`138,561`) proved safe in the default grammar (the `$`-letter cell
  pin and separator-wins rules make them collision-free), the mode had no
  dialect left to own. Literals that don't *conflict* with anything belong in
  the core language, not behind a toggle.
- **A `scientific` display rounded to N digits.** The mantissa keeps the
  value's own significant digits — a display mode must never round (exactness
  is the language's first rule); `Decimal(…)`/`round(…)` exist for that.

## Gotchas & notes

- **The log↔grid `^` seam.** With the log in Programmer mode, `5 ^ 3` on the
  input line is XOR, but a *cell* `=5 ^ 3` is still power — the same glyph means
  different things on the two surfaces at once. This can't corrupt anything (the
  log reads a cell's *value*, never its text), but it's a genuine cognitive seam;
  it's the reason the scope is log-only.
- **History shows what you typed — and recall re-evaluates under the current
  mode.** Existing entries display verbatim (they aren't re-skinned when you
  switch). And **recall (↑) / copy give back the text you typed**, not the
  canonical form — so recalling a line authored in one mode while in another
  re-evaluates it under the *current* dialect. Recall `5 ^ 3` from Programmer
  history into Normal and it computes power (`125`), not XOR (`6`). Switch the
  mode back, or recall deliberately. (The spec's canonical-recall + uniform
  re-skin is deferred — see the status note up top.)
- **Bitwise on fixed-width integers is type-preserving.** In Programmer mode
  `&`/`|`/`^`/`<<`/`>>`/`~` over `Int`/`UInt` values operate in
  two's-complement and keep the type — see [FIXED-WIDTH.md](FIXED-WIDTH.md).
- **Out-of-mode glyphs are loud, never silent.** A `&` (or `<<`, `~`) in Normal
  mode is a clear error naming the function to use, not a misparse.
- **Scientific + Programmer never meet.** The programmer hex echo (an integer
  result of a `0x…` line showing its hex form) is Programmer-flavored display;
  the sci/eng echo is Scientific's. One mode at a time, one echo rule at a time.

## Resolved decisions

1. **Precedence** — Python-style (bitwise band below arithmetic, above
   comparison). Not C/Java.
2. **Scope** — log-only for now; workbook-wide later.
3. **New builtins** — `pow(a,b)` only. No `pct` (percent is the `.percent` node;
   `* 0.01` fallback in Programmer). `~` (prefix bitwise-NOT in Programmer mode)
   shipped with fixed-width integers.
4. **`&`** — Programmer-only bitwise AND. Concat stays `+` / `concat()`; the
   reserved-`&`-for-concat plan is dropped.
5. **The trio** — normal / scientific / programmer. Finance is retired; its
   literals (currency, grouping) are core grammar in every mode, and the merged
   default keeps the name `normal`.
6. **Scientific** — an echo dialect only: SCI notation at the value's own
   significant digits, plus the ENG style (`:mode scientific eng`, exponent a
   multiple of 3). Value-carried display (Money, grouping) wins over it.
7. **`°`** — a mode-agnostic postfix literal (`x × π/180`, 50-digit π), not a
   Scientific-only glyph: no other mode owns a meaning for it, so gating it
   would only cost reach.

Related work, **now shipped**: **fixed-width integer types** (`Int32`, `UInt64`,
…) — the home for `~`, signed shifts, and checked bounded arithmetic. Orthogonal
to modes (a value exists in any mode; the mode only affects glyph parsing + radix
display). Specified in `docs/FIXED-WIDTH.md`.
