# Modes — input/display dialects over one language

> **Status: implemented for v1 (log-only, live-input).** The engine (mode-aware
> parser + per-mode renderer), the CLI (`:mode`), and the app (Settings picker +
> input-bar affordance) are landed and tested; `anzan.feature` pins the
> Programmer-mode grammar. **One deliberate v1 scoping:** the log shows existing
> entries *verbatim* (they're inert records — stored input + result text, never
> re-evaluated), so there is no correctness risk; the *uniform historical
> re-skin* described under "Switching modes" is **deferred** (it needs canonical
> storage of every entry plus recall/copy changes plus comment/error/def
> handling — disproportionate for a cosmetic effect on an inert tape). Fixed-width
> integer types are a *separate* feature — see `docs/FIXED-WIDTH.md`.

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

## Using modes

### Switching

Both app entry points set the same per-session dialect (persisted like the theme):

- **App → Settings → Mode** (⌘,) — a Normal / Programmer / Finance picker.
- **App input-bar affordance** — the small icon just left of the 📖 reference
  button (`#` Normal · `</>` Programmer · `$` Finance); click it for a menu. It
  turns accent-colored whenever you're off Normal.
- **`:mode` command** — type `:mode programmer` (or `finance` / `normal`)
  directly into the **app log** *or* the **CLI** REPL/pipe; in the CLI a bare
  `:mode` prints the current dialect. In the app it logs a divider like any
  switch.
- **Embedders** — set `Calculator.mode`.

Mode applies to the **log / input line only**; grid cells always use the
canonical (Normal) grammar (see *Scope*).

### Same keystrokes, different dialect

| you type | Normal / Finance | Programmer |
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
modulo in Normal/Finance is `mod(a, b)`. Nothing is ever unreachable — only
re-spelled. (Finance is grammatically identical to Normal today; it's the home
for future finance *display* defaults.)

## Why this is allowed (the principle it must satisfy)

The governing rule (the "rand() principle", applied at the syntax level) is
**not** "no modes" — it is: *what is stored must mean the same thing under every
UI state.* A mode satisfies it iff:

1. **Canonical is the only thing stored or transported.** Cells, the log tape,
   the workbook codec, copy/paste, recall, and `man()` all carry the canonical
   spelling (`mod`, `bitXor`, `pow`, `bitShift`). The dialect glyph exists only
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
        ▼   parse-under-mode                    │  AST → skin
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
- **Display** is a pure function `render(AST, mode) → text`, the existing
  `Expression.sourceText` parameterized by mode, with a **function fallback** for
  operations the mode doesn't own a glyph for.

Because the store carries no mode, **reload, replay, and undo are mode-free** —
there is nothing to disambiguate. (This is what makes tracking mode-switches
unnecessary; see "Rejected alternatives → per-segment mode tracking".)

## The dialects

Three to start. `Normal` **is** the canonical spelling, so the stored form is
exactly what `Normal` renders — today's grammar, unchanged.

| operation | canonical | Normal | Programmer | Finance |
|---|---|---|---|---|
| power | `pow(a,b)` | `a ^ b` | `pow(a,b)` | `a ^ b` |
| XOR | `bitXor(a,b)` | `bitXor(a,b)` | `a ^ b` | `bitXor(a,b)` |
| AND | `bitAnd(a,b)` | `bitAnd(a,b)` | `a & b` | `bitAnd(a,b)` |
| OR | `bitOr(a,b)` | `bitOr(a,b)` | `a \| b` | `bitOr(a,b)` |
| shift L/R | `bitShift(a,n)` | `bitShift(a,n)` | `a << n` / `a >> n` | `bitShift(a,n)` |
| modulo | `mod(a,b)` | `mod(a,b)` | `a % b` | `mod(a,b)` |
| percent | `.percent` node | `x%` | `x * 0.01` | `x%` |
| concat | `concat`/`+` | `a + b` | `a + b` | `a + b` |

The point of the table: **the same glyph (`^`, `&`, `%`) is owned by a different
operation per mode, and whatever a mode doesn't own falls back to the canonical
function.** So in Programmer mode `pow` has *no* infix glyph and renders as
`pow(2,3)`; in Finance mode `bitXor` renders as `bitXor(5,3)`. You can never look
at a glyph and be unsure — in your current mode it means exactly one thing, and
the meanings it lacks are spelled out.

Notes that fell out of the design walk-through:

- **`pow` is the one new builtin** this proposal adds (power as a function, for
  when `^` is taken by XOR). `mod`, `bitAnd/Or/Xor`, `bitShift`, and `concat`
  already exist; `+` already concatenates when either side is a string.
- **`bitShift(a, n)`** already encodes both directions (`n > 0` left, `n < 0`
  right), so Programmer `a << n` / `a >> n` both canonicalize to it (`>>`
  negates `n`).
- **Percent is *not* a function.** A typed `x%` is the `.percent` postfix node,
  rendered `x%` in Normal/Finance and `x * 0.01` in Programmer (where `%` is
  taken by modulo). `3% == 0.03 == 3*0.01`, exact, re-parseable in any mode — so
  no `pct` builtin is confiscated. Percent and modulo never collide because they
  have **distinct canonical forms** (`.percent` vs `mod`); they merely reuse the
  `%` glyph. One wrinkle: *editing* a percent line while in Programmer mode edits
  `x * 0.01`, which re-parses as multiplication — same value, lost percent-ness.
  Viewing-and-switching-back is lossless (viewing never touches the AST).
- **`&` is Programmer-only AND.** Concatenation stays `+` (string-aware) and
  `concat()` in every mode; the long-reserved "`&` for Excel-style concat" plan
  is dropped, since `+` already covers it. In Finance/Normal, `&` is a loud
  mode-scoped error (see E3).
- **Bitwise NOT (`~`) is deferred**, not dropped — it needs a fixed bit-width,
  which Anzan's arbitrary-precision integers don't have. Its home is
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
`|`). In Normal/Finance, `^` keeps its tight, right-associative power slot and
the bitwise functions are ordinary calls (primary level); `%` is the existing
tight postfix.

This means `a ^ b == c` parses to different ASTs in Programmer (XOR, above
comparison) vs Finance (power) — correct, because they're different dialects.
The ASTs are unambiguous once formed; only the parse is mode-sensitive.

## Enforcement

What actively prevents a wrong meaning — by construction, not discipline:

- **E1 — canonical is the stored truth.** The dialect glyph is never persisted,
  so it can never be re-read under a different meaning.
- **E2 — function fallback on render.** A glyph only ever renders for its mode's
  meaning; the other meanings render as their canonical function.
- **E3 — out-of-mode glyphs are loud, never silent.** Typing `5 << 2` in Finance
  is a *mode-scoped parse error* — "`<<` is a Programmer-mode operator; this
  surface is in Finance mode (use `bitShift(5, 2)`)" — not a misparse. (`<<`
  and `&` aren't tokens in Finance at all.)
- **E4 — transport emits canonical.** Copy, recall, insert, and cell-commit all
  produce the canonical spelling, so a line lifted out of context (into a cell, a
  bug report, a chat) is unambiguous. Recall a Programmer `5 ^ 3` anywhere → it
  lands as `bitXor(5, 3)`.

## Switching modes

Switching is **change the live mode, then re-render the visible surface**. There
is no migration of stored data — the store was canonical all along.

- **Lossless in meaning and value, always.** `render` is a pure function of
  `(AST, mode)` and the AST is invariant across switches, so
  Finance → Programmer → Finance returns identical text by construction. The
  numbers never move.
- **Uniform, not per-segment.** The *whole* visible tape (and visible grid, if
  in scope) re-skins to the current dialect — one consistent reading surface,
  not a patchwork of "the dialect each line was typed in". *(Deferred in v1 — see
  the status note at the top; v1 shows existing entries verbatim and applies the
  mode to live input only.)*
- **Text normalizes.** Re-deriving the skin from the AST loses incidental
  formatting (extra spaces, `1_000` vs `1000`). Trailing `# comments` survive;
  other literal trivia normalizes — the same way every spreadsheet reformats a
  formula regardless of how you typed it.

Cost of a switch is negligible: the log is capped at 500 entries and the grid
re-skins only *visible* cells (lazy); re-rendering a few hundred short ASTs is
sub-millisecond. No per-keystroke cost — one entry renders when added, one cell
when shown.

## Scope: log-only first, workbook-wide later

**v1 — log-only.** The log is a mode-switchable *calculator* (a physical calc's
DEC/HEX/DEG toggle); the grid stays canonical (`Normal`) — the familiar
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
| **render** | — | ❌ per-mode skin table + function fallback |
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

## Gotchas & notes (v1)

- **The log↔grid `^` seam.** With the log in Programmer mode, `5 ^ 3` on the
  input line is XOR, but a *cell* `=5 ^ 3` is still power — the same glyph means
  different things on the two surfaces at once. This can't corrupt anything (the
  log reads a cell's *value*, never its text), but it's a genuine cognitive seam;
  it's the reason v1 is log-only.
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

## Resolved decisions

1. **Precedence** — Python-style (bitwise band below arithmetic, above
   comparison). Not C/Java.
2. **Scope** — log-only for v1; workbook-wide later.
3. **New builtins** — `pow(a,b)` only. No `pct` (percent is the `.percent` node;
   `* 0.01` fallback in Programmer). No `~` (deferred to fixed-width).
4. **`&`** — Programmer-only bitwise AND. Concat stays `+` / `concat()`; the
   reserved-`&`-for-concat plan is dropped.

Related future work: **fixed-width integer types** (`int32`, `uint64`, …) — the
home for `~`, signed shifts, and checked bounded arithmetic. Orthogonal to modes
(a value exists in any mode; the mode only affects glyph parsing + radix
display). Specified in `docs/FIXED-WIDTH.md`.
