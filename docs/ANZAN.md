# Anzan・暗算 — the language specification

**Anzan** (暗算, "mental calculation" — the discipline of computing on a
soroban you only imagine) is the expression language implemented by the
`Anzan` module of the engine package. The Soroban app hosts it in the
calculator log and in grid cells; the `soroban` CLI is Anzan without the
app — it depends on the language module alone.

Design intent, in order: **exact** (a value is the mathematically true
answer, or carries an explicit precision), **expression-oriented** (every
line evaluates to a value; there are no statements beyond assignment and
definition), **notation-mirroring** (`f(x) = x * 2`, `2x`, `∑_i=1^10(i^2)`
read like the math they denote), and **small** (no I/O, no loops with side
effects — the host provides persistence and interaction).

This document specifies the canonical language. Two companion specs cover
extensions: **[MODES.md](https://github.com/alleato-llc/soroban/blob/main/docs/MODES.md)** — the Programmer/Finance input-display *dialects*
(what the glyphs `^ % & | << >> ~` mean per mode, over one canonical AST) — and
**[FIXED-WIDTH.md](https://github.com/alleato-llc/soroban/blob/main/docs/FIXED-WIDTH.md)** — the bounded, checked `Int`/`UInt` integer types — and
**[DECIMAL.md](https://github.com/alleato-llc/soroban/blob/main/docs/DECIMAL.md)** — fixed-precision `Decimal(value, precision, scale)` (or the short forms `Decimal(value)` / `Decimal(value, scale)`) (the money
type). The workbook container is specified in [FORMAT.md](https://github.com/alleato-llc/soroban/blob/main/docs/FORMAT.md); app behaviors
(themes, grid UX) live in the [README](https://github.com/alleato-llc/soroban/blob/main/README.md).

## At a glance

```
1_000 * 1.0825                       # 1082.5 — exact, no floating-point drift
0.1 + 0.2 == 0.3                     # 1 (true) — decimals are exact, not binary
pmt(0.05/12, 360, 300000)            # spreadsheet finance, by its Excel name
double(x) = x * 2                    # define a function — it reads like the math
∑_i=1^10(i^2)                        # 385 — notation you can actually type
data Point { x: Number, y: Number }  # a typed record…
Point(x: 3, y: 4)                    # …constructed by field name
```

Every line is an expression that evaluates to a value; there are no statements
beyond assignment and definition. The same language runs in the app's
calculation log, in grid cells, and in the `soroban` CLI.

## Influences

Anzan is small enough to name its ancestors precisely:

- **Mathematical notation** — the deepest influence: `f(x) = x * 2` defines
  a function because that's what it looks like; implicit multiplication
  (`2x`, `2(3+4)`); indexed `∑`/`∏`; `√ π τ × ÷` as first-class spellings.
  The rule: where mathematics already has good syntax, Anzan borrows it
  rather than inventing.
- **Spreadsheet formula languages** (the VisiCalc → Excel lineage) — cell
  references and ranges, variadic aggregates that accept scalars, arrays,
  and ranges interchangeably, the finance library's names and sign
  convention (cross-validated against Excel in the test suite),
  case-insensitive function names beside case-sensitive variables, the
  tolerated leading `=`, and the label-vs-formula forgiveness of hosted
  cells.
- **The functional family** (Scheme, ML) — expression orientation, functions
  as values, lambdas closing over locals by value, `map`/`filter`/`reduce`,
  immutable structures, recursion as *the* loop — with Scheme's proper tail
  calls echoed in the constant-stack tail recursion, and Lisp's special
  forms in the lazy `if()` and unevaluated `man name`.
- **JSON / JavaScript** — structure literals (`[1, 2, 3]`,
  `{name: "Ada"}`), 0-based indexing, `m.key` / `m["key"]` access, string
  escapes. Canonical value rendering is deliberately JSON-adjacent so
  workbooks stay diffable and hand-editable.
- **Databases and systems languages** (SQL, Rust, Ada, Python) — the bounded
  numeric types that sit *beside* the default exact number, and the Programmer
  dialect's operators. `Decimal(p, s)` is SQL's money type (with PostgreSQL's
  1000-digit precision ceiling); `Int32` / `UInt8` take Rust's fixed-width
  spellings and *checked* arithmetic — an overflow is an error, in the
  discipline of Ada's range types, never C's silent wraparound. Programmer
  mode's bitwise operators (`^ & | << >> ~`) follow Python's glyphs and
  precedence (see [Language modes](#language-modes-presentational-dialects)).
- **Unix** — `#` comments, doc-comments-as-man-pages (`man pmt`), and a
  REPL/pipe CLI with honest exit codes.

Equally deliberate is what was *refused*: IEEE-754 float semantics (the
entire reason Anzan exists), statements and side-effecting loops, `null`,
and JavaScript's coercion habits — truthiness is typed, and only string
`+`-concatenation crossed that fence, because labels want it.

## Design rules: language vs. library

Anzan distinguishes sharply between the **language** (syntax, special
forms, evaluation rules) and the **library** (registry functions), and the
bar for the former is deliberately high. A construct earns language status
only when a function *cannot* do the job, which happens for exactly three
reasons:

1. **It needs unevaluated arguments.** `if()` must not evaluate the
   untaken branch; `man name`'s argument is a name, not a value;
   `∑_i=1^10(term)` re-evaluates its term per index. Functions receive
   evaluated values — by the time one runs, it's too late.
2. **It binds names.** Lambda parameters, `∑`'s index, `f(x) =`'s
   parameters, `data`'s type and field names. Functions cannot introduce
   names into scope.
3. **It is a literal shape or a session mutation.** `[1, 2]`,
   `{key: value}`, `x = expr`, `data P { … }` — syntax for constructing
   values or changing the environment.

Everything else **must** be a library function: pure (no environment
access, no I/O, no randomness — recalculation must be reproducible),
self-documenting (registration requires a signature, summary, and examples
the test suite evaluates), and callable like anything else. The
implementation enforces the asymmetry: a special form costs parser code
and hand-written documentation; a function can't even compile without its
docs. Language is expensive on purpose.

Two corollaries guide additions:

- **New library must compose.** A function should consume and produce the
  language's existing value types — especially function values and arrays
  — rather than inventing parallel mini-conventions. `solve(f, target)`
  takes any function value; `Person(fromJson(t))` is ordinary application,
  not a typed-parse form; named arguments are one desugaring (to a single
  map) that every call site gets, not a constructor-only convention.
- **Prefer a value to a construct.** When `toJson` needed named options,
  the answer was a constant map (`Json.Pretty`, a plain string in
  disguise), not an enum syntax. When ranges needed to meet the
  higher-order functions, the answer was `list(…)`, not a new expression
  form.

There is a third tier below the library: **user space**. A function earns
a registry slot only when a user function can't do the job *well* — it
needs engine internals or algorithmic depth (`sqrt`'s 50-digit iteration,
`fromJson`'s parser), it pins a convention everyone must share (banker's
rounding, the finance sign convention, what counts as a business day), or
it's arrival vocabulary spreadsheet users type from muscle memory (`pmt`,
`stdev`). Anything derivable by small composition — spelling preferences,
partial applications like `toHex(n) = toBase(n, 16)` — belongs to user
space, which the language deliberately makes first-class: one line buys
documentation, `man()`, autocomplete, and workbook persistence. The
deciding asymmetry: a builtin name is confiscated from user space forever
(builtins can't be redefined, and removing one breaks workbooks), while a
user function is reversible and yours. When in doubt, user space.

The registry has been audited against these tiers (2026-06). Two findings
stand: (1) `today()` is the library's one purity exception — it reads the
clock, so a sheet using it computes differently tomorrow; accepted and
contained, nothing else may join it. (2) A handful of slots are thin by
the rule — `percent` (= x/100, the weakest), `cbrt`, `forecast`, `ppmt`,
`quarter` — surviving on vocabulary or grandfathering. They stay: the
one-way door means eviction breaks workbooks, a worse sin than a thin
slot. The tier test therefore applies **at admission time only**, judged
against the language's expressiveness *as of that moment* — several
once-irreducible functions (`median`, `npv`, `sumproduct`) became
compositions retroactively when lambdas, `sort`, and `seq` arrived, and
that is the normal fossil record of any standard library, not a defect.

Known tension, recorded honestly: range expansion (`A:1..B:9` flattening
into an argument list) is a language rule that library signatures bend
around — the paired-series functions (`correl`, `sumproduct`, …) split
flat argument lists evenly, and `percentile` takes `p` last, because
ranges aren't first-class values. If ranges ever become real array
expressions (the array-spilling roadmap item), those conventions should
relax into honest `(array, p)` signatures. And the indexed `∑`/`∏` forms
predate lambdas — `sum(map(i -> i^2, seq(1, 10)))` says the same thing in
pure library; the special forms remain because notation-mirroring is the
spec's first-listed design intent, not because they're necessary.

## 1. Lexical structure

### Numbers

Decimal literals: `123`, `1.5`, `.5`, `1_000` (underscore group separators),
`2.5e-3` / `1E6` (scientific). Programmer literals: `0xFF` (hex) and
`0b1010` (binary) — exact integers at any width, `_` separators welcome;
a stray digit, letter, or `.` after one is a lex error (`0xFG`, `0x1.5`),
never a silent implicit multiplication. The leading sign is not part of the
literal — `-2` is unary minus applied to `2` (see §3 for why that matters
next to `^`).

### Strings

Double-quoted: `"Q1 revenue"`. Escapes: `\"` `\\` `\n` `\t`. An unterminated
string or unknown escape is a lex error. Strings render back out in canonical
quoted form (`"a\tb"` echoes as `"a\tb"`).

### Identifiers

Letters, digits, and `_`; can't start with a digit. **Variables are
case-sensitive** (`Rate` and `rate` are different); **function calls are
case-insensitive** (`PMT(…)` ≡ `pmt(…)`). One case-insensitive namespace
covers all function names — you cannot define a function whose name collides
with a built-in.

### Reserved names

`ans` `pi` `π` `tau` `τ` `e` `true` `false` `Json` `Rounding` `sigma` `if`
`man` `manual` `help` cannot be assigned to. Identifiers beginning `sigma_` / `product_` are reserved for
the indexed reduction forms (§8). `data` is a **contextual** keyword — only
the exact shape `data Name {` starts a declaration (§7), so `data = 5` is
still an assignment.

### Comments

`#` runs to end of line. A `#` inside a string literal is literal text, not
a comment. Comments come in three roles:

- **Trailing a calculation** (`100 * 1.0825 # with tax`): the code evaluates
  normally; the comment is kept and shown dimmed beside the result (and on a
  grid formula cell, retained in the raw).
- **Trailing a function or `data` definition**: it **is** the definition's
  documentation, shown by `man()` (§5, §7).
- **A comment-only line** (`# revisit in Q4`): a first-class **note** — a
  recorded annotation, not a parse error and not a no-op. It never touches
  `ans`. In a grid cell, a comment-only cell is a note: dim, holds no value,
  skipped in ranges, and an error to reference numerically (like text).

### Mathematical symbols

First-class spellings: `×` `÷` `−` `·` (operators), `√` (prefix square
root), `π` `τ` (constants), `∑` `∏` (reductions). Every symbol has an ASCII
spelling (`*`, `/`, `-`, `sqrt(…)`, `pi`, `tau`, `sigma…`, `product…`).

### Cell references

`A:1` lexes as a single token when letters are immediately followed by
`:digits` (column A–Z, row 1–1000). `Sheet!A:1` and `'Q1 Budget'!A:1`
qualify by worksheet; a bare `'Name'` is a named-cell reference. `..` builds
a range (`A:1..B:9`). These tokens are part of the language, but they
resolve only where a host wires them (§10).

## 2. Values

Every expression evaluates to one of:

| Type | Literal | Notes |
|---|---|---|
| number | `1.5`, `2.5e-3` | arbitrary-precision decimal (§4) |
| string | `"text"` | |
| array | `[1, 2, 3]` | heterogeneous, nests freely |
| map | `{name: "Ada", age: 36}` | insertion-ordered; keys case-sensitive |
| function | `x -> x * 2`, or a bare function name | first-class (§6) |
| record | `Person(name: "Ada", …)` | an instance of a declared `data` type (§7) |
| fixed-width int | `Int32(255)`, `UInt8(255)` (or `Int(255, 32)`, `UInt(255, 8)`) | a bounded, checked integer — exact, but overflow is an error, not a wraparound ([FIXED-WIDTH.md](https://github.com/alleato-llc/soroban/blob/main/docs/FIXED-WIDTH.md)) |
| fixed-precision decimal | `Decimal(10.5, 5, 2)`, `Decimal(0.5)`, `Decimal(0.5, 2)` | SQL DECIMAL(p,s) / money: rounds to `scale`, checked `precision` (≤ 1000), configurable rounding; short forms capture the value at max precision ([DECIMAL.md](https://github.com/alleato-llc/soroban/blob/main/docs/DECIMAL.md)) |
| handle | *(no literal)* | an opaque, read-only host object — a `Workbook`, worksheet, or cell — navigated with `.` and `[]` (§10) |

Structures are **immutable** — there is no element assignment; rebind the
variable. Values render canonically: `description` re-parses to an equal
value (this is how structured values persist in workbooks). The one exception
is a **handle**: it's a live, read-only *view* of the host, not data, so it has
no literal and does not persist — bind one to a variable for the session, but a
saved workbook stores none.

**Equality** (`==`, `!=`) is deep, and order-insensitive for maps.
**Ordering** (`<` `<=` `>` `>=`) requires numbers. `+` concatenates when
either side is a string (`"Q" + 1` → `"Q1"`); every other operator is
numeric and raises a typed error otherwise.

**Truthiness requires a number**: `if("a", 1, 2)` is an error, not a coercion.
`true`/`false` are the numbers 1/0.

**Indexing is 0-based**, including strings: `arr[0]`, `"abc"[1]` → `"b"`,
`[[1,2],[3,4]][1][0]` → `3`. Map access: `m.key` or `m["key"]`.

## 3. Operators and precedence

From loosest to tightest. Each line binds tighter than the one above:

| Level | Forms | Notes |
|---|---|---|
| statement | `name = expr` · `f(a, b) = expr` | assignment / definition; only at line level |
| lambda | `x -> expr` · `(a, b) -> expr` | legal at every expression position |
| comparison | `< <= > >= == !=` | **non-chaining**: `a < b < c` is a parse error |
| additive | `+ -` | |
| multiplicative | `* /` (`× ÷ ·`) and **implicit multiplication** | `2x`, `2(3+4)`, `(a)(b)`, `2 A:1`, `2pi` — a value against a **name/paren/cell**, NOT a bare number |
| unary | `-` `+` `√` | prefix; `√x` ≡ `sqrt(x)` |
| power | `^` | **right-associative**: `2^3^2` = `512`; exponent may carry its own sign: `2^-2` = `0.25` |
| postfix | `expr[i]` · `expr.name` · `expr%` | binds tighter than `^`; chains freely |
| primary | literals, names, calls, `(expr)`, `[…]`, `{…}`, reductions, `if(…)` | |

Because unary minus binds **looser** than `^`: `-2^2` = `-4`.
**`%` is a postfix percent**: `x%` ≡ `x × 0.01`, exact (`3%` is `0.03`). It binds
tighter than `^`, so `1 * 3%` is `1 * 0.03`, not `(1 * 3)%`. Modulo is the
`mod(x, y)` function (the `%` symbol is percent, not modulo — bitwise stays
functional too).

**Implicit multiplication is a value against a name, paren, or cell — never a
bare number.** `2x`, `2pi`, `2(3+4)`, `2 A:1` all multiply by juxtaposition, but
two numbers in a row (`3 4`, or `3 % 4` — which is `3%` then `4`) is a **missing
operator**, so it's a parse error nudging toward `*`, not a silent product. For
`3` mod `4`, write `mod(3, 4)`.

The table above is the **canonical (Normal) dialect**: `^` is power, `%` is
percent, and bit operations are functions. Programmer mode re-spells some of
those glyphs as bitwise/modulo operators — see [Language modes](#language-modes-presentational-dialects)
next.

## Language modes (presentational dialects)

A **mode** changes only how glyphs are *typed and displayed* — never what a
formula means. Every mode parses to the same canonical AST and stores the same
canonical form, so a saved workbook reads identically under any mode and a
formula can never mean two things. A programmer types `5 ^ 3` for XOR; an
analyst types `5 ^ 3` for *power*; the engine stores `bitXor(5, 3)` or
`pow(5, 3)` either way. Modes are **log/input-line only** — grid cells always
parse the canonical dialect. The full glyph tables are in
[MODES.md](https://github.com/alleato-llc/soroban/blob/main/docs/MODES.md).

- **Normal** *(default — the dialect §3 specifies)*: `^` power, `%` percent;
  bit operations are the functions `bitAnd` `bitOr` `bitXor` `bitShift`
  `bitNot`.
- **Programmer**: the glyphs `^ & | << >> %` read as XOR / AND / OR /
  shift-left / shift-right / modulo, and prefix `~` is bitwise NOT — Python's
  operators and precedence (the bitwise band sits below arithmetic and above
  comparison). Power becomes the `pow(a, b)` function. A glyph a mode lacks is
  always written longhand (`pow` in Programmer, `bitXor` in Normal), so nothing
  is unreachable — only re-spelled.
- **Finance**: grammatically identical to Normal today; reserved as the home
  for future finance *display* defaults (e.g. currency formatting).

**Out-of-mode glyphs are loud, never silent**: a bare `&` in Normal, or `<<` in
Finance, is a clear error, not a misparse. Because only the canonical (Normal)
form is ever stored, switching modes and reloading are lossless — and Normal
must stay byte-identical to the pre-modes grammar (it's the regression oracle).

## 4. The exactness model

Numbers are arbitrary-precision decimals (`BigInt` significand × 10^exponent,
always normalized).

- **Exact, unconditionally**: `+ − ×`, integer `^`, postfix `%` (× 0.01), and
  `mod(x, y)`. `0.1 + 0.2 == 0.3` is `1`; `∏_i=1^25(i)` is all 26 digits of 25!.
- **Exact to the precision context**: `/` and `sqrt` round to 50 significant
  digits (banker's rounding).
- **Double-bridged**: transcendentals (`exp`, `ln`, `log`, trig, non-integer
  `pow`) round-trip through IEEE double (~15 significant digits). This is a
  documented seam, confined to one place in the engine, pending an
  arbitrary-precision upgrade.

Constants `pi`/`π`, `tau`/`τ`, `e` are predefined to ~60 significant digits —
more than the division context, so they never limit a result.

## 5. Variables, `ans`, functions

**Assignment** `x = 12 * 80.5` binds a global and shows the value. A leading
`=` on any line is tolerated (pasted cell formulas just work).

**`ans`** is the previous successful *value* — definitions and `man()` don't
touch it, and a failed calculation never clobbers it.

**Function definition** `f(x) = body` stores the body unevaluated. The
trailing `# comment` becomes the function's documentation, shown by
`man f`. Definitions may appear in any order; names resolve at **call
time** — which also means a body's free variables read the *current* global.
Parameters may be **type-annotated** (`f(p: Point) = …`) for dispatch, and a
definition's name may be an operator symbol — see §7:

```
x = 10
g(y) = x + y
g(1)        # 11
x = 100
g(1)        # 101 — not 11
```

**Parameters shadow globals.** An array argument binds as ONE parameter (it
does not splat).

**Recursion** is a first-class idiom (`fact(n) = if(n <= 1, 1, n * fact(n-1))`).
Tail-recursive calls run at constant stack to any honest depth; non-tail
recursion grows the stack onto fresh segments as needed. Sanity limits
(§11) convert runaway recursion into a clean error with a hint about the
missing base case.

## 6. Lambdas and function values

`x -> body` and `(a, b) -> body` are function literals, legal anywhere an
expression is. They **capture surrounding locals by value** at creation:

```
make(a) = (b -> a + b)
add5 = make(5)
add5(2)      # 7
```

A bare name in expression position falls back to a function value after
variable lookup fails — `double = abs` then `double(-3)` works. Named
references stay symbolic and **re-resolve at call time** (an alias follows a
later redefinition); lambdas carry their own bodies. Higher-order built-ins:
`map`, `filter`, `reduce`.

## 7. Data types

`data` declares a **typed record** — a map with a contract:

```
data Person { name: String, age: Number, active: Boolean }   # a teammate
```

Type names start with a capital letter; the declaration registers the name
as a **constructor** in the function namespace (case-insensitive calls;
collisions with built-ins and your functions are rejected both ways;
redeclaring your own type is allowed). Field types are `Number`, `String`,
`Boolean` (any casing), or **another declared data type** — so records nest:
`data Line { a: Point, b: Point }`. A nested field is checked at construction
(the value must be a record of that type); the field type need not be declared
before the type that uses it, but you'll need it to build an instance. There
are no list-typed fields in v1 (compose with arrays/maps for that). Nesting
depth is bounded only by what you can construct (bottom-up) and the evaluation
stack; there's no cheap-to-hit fixed cap, and validation is O(1) per field
(records are immutable and already-validated, so no recursive re-checking).
The trailing `# comment` is the type's documentation, exactly like functions.

### Constructing and reading

**Construction is by field name or from a map — never positional** (a
deliberate decision: field names at every call site):

```
Person(name: "Ada", age: 36, active: true)
Person({name: "Ada", age: 36, active: true})     # same thing
Person(m)                                        # m holds such a map
```

Named arguments are sugar for the one-map form. Every declared field must be
present and type-correct; extras are errors; a `Boolean` field accepts
exactly `true`/`false` (1/0) — `active: 7` is caught. Instances
canonicalize to declaration order, so `Pt(y: 2, x: 1) == Pt(x: 1, y: 2)`.

One lexing wrinkle: a compact SINGLE-letter named argument (`f(a:1)`) lexes
as the cell reference `a:1` — write `f(a: 1)` with the space. Multi-letter
compacts (`age:36`) can't be cells and decompose correctly.

**Instances read like maps** — `p.name`, `p["age"]`, `keys`/`values`/`len`
all work; they collect into arrays and flow through `map`/`filter`/`reduce`;
a bare type name is its constructor as a value (`map(Person, listOfMaps)`).
Records are immutable, equality is deep, and a record never equals a plain
map. They render as constructor calls (`Person(name: "Ada", …)`), which is
also how record variables persist in workbooks.

### Records nest

A field's type may be another declared data type:

```
data Point { x: Number, y: Number }
data Line  { a: Point, b: Point }
seg = Line(a: Point(x: 1, y: 1), b: Point(x: 4, y: 5))
seg.b.x                                          # → 4    (drill straight in)
seg == Line(a: Point(x: 1, y: 1), b: Point(x: 4, y: 5))   # → 1  (equal by ALL state)
toJson(seg, Json.Compact)                        # {"a":{"x":1,"y":1},"b":{"x":4,"y":5}}
length(s: Line) = sqrt((s.b.x - s.a.x)^2 + (s.b.y - s.a.y)^2)
length(seg)                                      # → 5    (a typed "method", see below)
```

A nested field is **checked at construction** — the value must be a record of
that type, so `Line(a: 5, …)` is a named error. `description`, `toJson`, and
`==` all recurse, so a nested value round-trips and compares by all of its
state. Validation is O(1) per field (the inner record was already validated at
its own construction), so there's no recursive re-checking and no cycle risk;
nesting depth is bounded by what you can build, not a fixed cap.

### JSON conversion

**`toJson(value, option?)`** serializes any data value — 2-space
pretty-printed by default (you're usually reading it); `toJson(x,
Json.Compact)` gives the one-line interchange form. `Json` is a reserved
constant map holding the options as named values (`Json.Pretty`,
`Json.Compact`) — call sites read like intent, not like a magic flag; the
options are plain strings, so `"compact"` works too. Numbers keep full
precision, and `Boolean` fields come out as JSON `true`/`false` — the
declared type is what makes that honest. Hosts display multi-line string
results raw (a block, like `man()` output), so pretty JSON actually looks
pretty in the log and the CLI; single-line strings keep their canonical
quoting.

**`fromJson(text)`** is the inverse: objects → maps, arrays → arrays,
`true`/`false` → 1/0, and numbers parse at **full precision** — straight
into exact decimals, never through floating point (a deliberately
hand-rolled parser; `fromJson("0.30000000000000004")` is exactly that
number). Re-type a parsed map with a constructor:
`Person(fromJson(t))`, or a whole collection with
`map(Person, fromJson(t))` — so `Person(fromJson(toJson(p))) == p`. JSON
`null` is refused (Anzan has no null), as are duplicate object keys.

### Typed parameters, dispatch, and operator overloading

A function parameter may carry a **type annotation**, written `name: Type`
(the same `name: Type` shape as a `data` field). The type is a built-in
scalar — `Number`, `String`, `Boolean` — or a declared `data` type:

```
distance(p: Point) = sqrt(p.x^2 + p.y^2)
```

A typed parameter matches only an argument of that type; an un-annotated
parameter matches anything. The **same name may have several definitions**
distinguished by their parameter types — *multiple dispatch*. At the call the
most specific matching definition runs (an exact data-type match beats an
untyped catch-all); an unresolvable tie is an "ambiguous call" error.

```
kind(n: Number) = "number"
kind(s: String) = "string"
kind(42)     # → "number"
kind("hi")   # → "string"
```

**Operator overloading.** A definition's name may be an arithmetic operator
symbol — `+ - * / ^` — which extends that operator to your data types:

```
+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)
*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)
Point(x: 1, y: 2) + Point(x: 10, y: 20)   # → Point(x: 11, y: 22)
Point(x: 1, y: 2) * 3                       # → Point(x: 3, y: 6)
```

`x op y` uses a matching overload only when a **record** is involved and the
operand types fit; otherwise the built-in operator applies, so `1 + 2` and
`"Q" + 1` are untouched. To keep core arithmetic sacrosanct, an operator
overload **must involve at least one declared data type** — two operands,
not all scalar; `+(a: Number, b: Number) = …` is rejected. Comparisons and
equality are not overloadable.

**Equality is automatic and structural.** Every record supports `==` / `!=`
out of the box, comparing the type name and *all* fields; two records are
equal iff they are the same type with equal fields, and a record never equals
a plain map. Ordering (`< > <= >=`) still requires numbers.

These definitions are ordinary user functions: log-global or sheet-scoped
(λ cells), case-insensitive, and persisted by their source line — overloads
included.

## 8. Conditionals and reductions

**`if(cond, then, else)`** is a special form: only the taken branch
evaluates (`if(1, 2, 1/0)` → `2`), so recursion can guard itself. The
condition must be a number; comparisons return 1/0.

**`∑` and `∏`** each have two syntaxes, split by shape:

- *Plain call* — variadic over values, arrays, and ranges: `∑(1, 2, 3)` → 6
  (≡ `sum`), `∏(2, 3, 4)` → 24 (≡ `product`).
- *Indexed form* — `∑_i=1^10(i^2)` → 385, `∏_i=1^25(i)` = 25!. Typeable as
  `sigma_i=1^10(i^2)` / `product_i=1^25(i)`. The index binds like a
  parameter (shadows globals, nests, composes with user functions). Bounds
  are signed primaries — a compound bound needs parens:
  `∑_i=(n-1)^10(i)`. An empty range yields the identity (∑ → 0, ∏ → 1); a
  span over 100,000 terms is an error.

## 9. Documentation: `man` / `manual` / `help`

`man NAME` (aliases `manual`, `help`) is a special form, written unix-style with
a space and **no parentheses** — `man pmt`, `manual sum`, `help if`. The argument
is a NAME, never evaluated. It returns the documentation entry (signature, summary, examples)
for built-ins, special forms, user functions, and data types (whose docs
come from their trailing `# comment`). Every built-in ships documentation,
and every documented example is evaluated by the test suite.

## 10. Hosted forms: cells, ranges, named cells

These constructs are part of the grammar but resolve only where the host
provides a sheet:

- `A:1` reads a cell's numeric value. Empty cells read as 0; text cells are
  an error to reference directly.
- `A:1..B:9` (corners normalize) is legal **only as a function argument**,
  where it expands in place: `sum(A:1..A:9)`. Empty/text cells are skipped;
  error cells propagate.
- `Sheet!A:1`, `'Q1 Budget'!A:1` qualify by worksheet name; `'Name'` /
  `Sheet!'Name'` read a named cell.
- `$A:1`, `A:$1`, `$A:$1` **pin** a reference's column/row. Pins are
  copy-time data: the host's fill and paste hold pinned axes while
  adjusting unpinned ones. To the evaluator a pinned reference is the same
  cell — `$A:$1 == A:1`, always. A `$` anywhere else is a lex error.
  (Named cells never adjust — a name is the absolute-by-meaning reference.)
- `refError()` always errors with "refers to a deleted cell". Hosts splice
  it over references whose row or column was deleted, so the formula fails
  loudly instead of silently reading shifted neighbors.
- In the CLI there is no sheet: cell syntax parses, but evaluation reports
  `no sheet available for A:1`.

### Workbook reflection

Where a host provides a workbook, a formula can inspect that workbook's own
structure — the sheets it holds and the cells in them. Inspection is
**read-only**, so a calculation can adapt to its surroundings; a small,
separate set of **log-only commands** changes the workbook. Two shapes, the
same underlying handles:

An **object graph** rooted at the `Workbook` value:

| Read | Result |
|---|---|
| `Workbook.count` | number of sheets |
| `Workbook.sheetNames` | array of sheet names |
| `Workbook.worksheets` | the sheet collection |
| `Workbook.worksheets[0]` | a worksheet by position (`-1` counts from the end) |
| `Workbook.worksheets["Budget"]` | a worksheet by name |
| `ws.name`, `ws.rowCount`, `ws.columnCount`, `ws.isData` | worksheet facts |
| `ws.cell("A", 2)` | a cell handle |
| `c.value` | the cell's number (errors like a direct reference when it isn't one) |
| `c.text` | the cell's displayed text |
| `c.raw` / `c.formula` | the cell's source |
| `c.address`, `c.isEmpty` | the cell's address, emptiness |

…and flat **accessor functions** for the common reads:

| Call | Result |
|---|---|
| `cell("A", 1)` | a cell handle on the formula's own sheet |
| `cell("Budget", "A", 1)` | a cell handle on a named sheet |
| `sheetName()` | the current sheet's name |
| `sheetNames()` | array of every sheet name |
| `rowCount()`, `columnCount()` | the grid's dimensions |

Cell reads through reflection are **live**: `=cell("A", 1).value + 1`
recomputes when `A:1` changes, exactly like `=A:1 + 1` — the same dependency
edge is recorded. Unqualified `cell("A", 1)` follows the formula's *owning*
sheet, the owning-sheet rule again. Reflection functions resolve **last**, so
your own `cell(x) = …` shadows the accessor. In the CLI there is no workbook:
`Workbook` and `cell()` are simply unknown.

### Workbook mutation

Mutation commands change the workbook. They run from the **log only** —
inside a cell they raise an error, because cell recalculation must be
reproducible (the `rand()` principle: a recalc that mutated the workbook could
loop or differ run to run). In the app each command is one undoable step.

| Command | Effect |
|---|---|
| `updateCell(cell, value)` | sets a cell — a number writes its digits, a string is verbatim (`updateCell(cell("A",1), "=B:1*2")` writes a formula; `""` clears) |
| `addWorksheet(name)` | appends an empty worksheet, returns its handle |
| `renameWorksheet(sheet, newName)` | renames (by handle or name) and rewrites every `Old!A:1` reference |
| `deleteWorksheet(sheet)` | removes a worksheet (refuses the last one) |

A worksheet argument is either a handle (`Workbook.worksheets[0]`) or a name
string. Like the reads, the commands resolve **after** your own functions, so a
user-defined `updateCell(…)` shadows the builtin.

### Calculation history

`History` is the calculation log as an **array of entry handles**, oldest →
newest — `ans` generalized across the whole tape. Because it's a real array, it
iterates and indexes with the ordinary tools:

| Form | Result |
|---|---|
| `len(History)` | number of entries (the right way to get the size) |
| `History[i]` | the i-th entry (0-based) |
| `last(History)` / `first(History)` | newest / oldest entry (no `[-1]` — arrays are 0-based) |
| `sum(map(entry -> entry.value, History))` | total of every result (`e` is reserved — Euler's — so name the parameter `entry`) |

Bare `History` evaluates to the whole array, which prints as a dump of opaque
`LogEntry(…)` handles — useful to glance at, but for the count use
`len(History)`. A result that carries reflection handles (a `History` dump, a
bare `Workbook`) is recorded **display-only** (`kind == "info"`): its rendering
isn't re-parseable, so it isn't a recallable value.

Each **entry** exposes:

| Field | Meaning |
|---|---|
| `entry.input` | the expression text you typed (`"A:1 + 10"`) — the replay/traceability source |
| `entry.value` | the result as a typed value (number/string), when `entry.kind == "value"` |
| `entry.text` | the displayed string — always present (the result, an error message, or a comment) |
| `entry.kind` | `"value"` · `"error"` · `"comment"` · `"info"` · `"function"` · `"datatype"` (`"info"` = display-only output like `man()`/JSON/a `History` dump — `.value` is absent) |
| `entry.isError` | sugar for `entry.kind == "error"` |
| `entry.referencesCells` | did the line read a cell / named cell? (provenance — the result already froze the cell value at log time) |
| `entry.note` | a trailing `# comment`, or `""` |

`History` is **log-only** and read-only: it resolves on the log input line, but
in a **cell** the name is simply unknown, so it degrades to a *text label* (not
an error) — a cell may hold a header literally named `History`. The reason is
reproducibility: the log is global session state, not the workbook, so a cell
reading it wouldn't be reproducible or portable. (In the CLI there is no log —
`History` is unknown.) History reflects the **tape** (what you did); the *current
value* of a cell is `Workbook` reflection, and the *current* variables/functions
live in the environment, reachable by name.

Cells hosting Anzan have a few host-level rules (formula vs. label
classification, λ/𝑖/𝑫 definition cells, control cells like
`rate = slider(…)`) — see the [README](https://github.com/alleato-llc/soroban/blob/main/README.md); they're behaviors of
the grid, not of the language.

## 11. Limits

| Limit | Value | On breach |
|---|---|---|
| Division/sqrt precision | 50 significant digits, banker's rounding | (rounds) |
| Call depth | 10,000 frames | error with a base-case hint |
| Tail-call iterations | 1,000,000 | error |
| Indexed reduction span | 100,000 terms | error |
| `fromJson` nesting | 128 levels | error |
| Grid (hosted) | 26 columns × 1,000 rows | (host) |

## 12. Errors

All errors are typed and carry a message; lex/parse errors carry a character
offset, which hosts render as a caret under the offending column:

```
> 2 +* 3
     ^
error: parse error at column 4: unexpected token
```

Notable guarantees: unknown names are reported (never guessed), comparison
chains are rejected with a suggestion (`and(a < b, b < c)`), division by
zero says so, and type errors name the offending type.

## Appendix: grammar sketch

EBNF-ish; literals quoted, `{}` repetition, `[]` optional. Token-level rules
(numbers incl. `0x…`/`0b…`, strings, cell references, comments) are in §1. This
is the **canonical (Normal-dialect)** grammar; Programmer mode adds a bitwise
operator band (`|` · `^` · `&` · `<< >>`, between comparison and additive) plus
prefix `~`, parsing to the same canonical bitwise functions — see
[MODES.md](https://github.com/alleato-llc/soroban/blob/main/docs/MODES.md).

```
line        = datadef | definition | assignment | expression ;
assignment  = IDENT "=" expression ;
definition  = ( IDENT | OPSYM ) "(" [ param { "," param } ] ")" "=" expression [ COMMENT ] ;
param       = IDENT [ ":" TYPENAME ] ;     (* typed params dispatch by argument type *)
OPSYM       = "+" | "-" | "*" | "/" | "^" ;  (* operator overload; needs ≥1 data-type param *)
datadef     = "data" TYPENAME "{" field { "," field } "}" [ COMMENT ] ;
field       = IDENT ":" ( "Number" | "String" | "Boolean" | TYPENAME ) ;  (* scalar casing free; TYPENAME = a data type *)

expression  = lambda | comparison ;
lambda      = ( IDENT | "(" [ IDENT { "," IDENT } ] ")" ) "->" expression ;
comparison  = additive [ compop additive ] ;          (* non-chaining *)
compop      = "<" | "<=" | ">" | ">=" | "==" | "!=" ;
additive    = term { ("+" | "-") term } ;
term        = unary { ("*" | "/") unary | unary } ;   (* 2nd alt: implicit × — the juxtaposed factor is a name/paren/cell, not a bare NUMBER (`3 4` is an error) *)
unary       = ("-" | "+" | "√") unary | power ;
power       = postfix [ "^" unary ] ;                 (* right-assoc, signed exponent *)
postfix     = primary { "[" expression "]" | "." IDENT [ "(" [ argument { "," argument } ] ")" ] | "%" } ;
            (* ".name" is member access; ".name(args)" is a method call; trailing "%" is percent (× 0.01) *)
primary     = NUMBER | STRING | CELLREF | NAMEREF | constant
            | IDENT | call | reduction | conditional
            | "(" expression ")" | array | map ;
call        = IDENT "(" [ argument { "," argument } | namedargs ] ")" ;
argument    = expression | range ;                    (* ranges only here *)
namedargs   = IDENT ":" expression { "," IDENT ":" expression } ;  (* ≡ one map argument *)
range       = CELLREF ".." CELLREF ;
            (* CELLREF = ["$"] LETTER ":" ["$"] DIGITS — pins are copy-time
               data for fill/paste; evaluation ignores them *)
conditional = "if" "(" expression "," expression "," expression ")" ;
reduction   = ("∑"|"∏") "_" IDENT "=" bound "^" bound "(" expression ")"
            | ("∑"|"∏") "(" argument { "," argument } ")" ;
bound       = [ "-" ] ( NUMBER | IDENT | CELLREF | "(" expression ")" ) ;
array       = "[" [ expression { "," expression } ] "]" ;
map         = "{" [ mapentry { "," mapentry } ] "}" ;
mapentry    = ( IDENT | STRING ) ":" expression ;
```
