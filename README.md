# Soroban

An exact calculator for macOS, with a mini-spreadsheet attached — named for
the Japanese abacus (算盤). Type expressions into the input line and results
accumulate in a scrolling log, or flip to a 26×1,000 grid (⌘\\) where cells
hold text, numbers, and formulas that can reference each other
(`B:1 + B:2`) — and save the whole thing as a `.soroban` workbook.

Built on an arbitrary-precision decimal engine — `0.1 + 0.2` is exactly `0.3`,
and money math never picks up binary floating-point drift.

The expression language is named **Anzan** (暗算 — "mental calculation", the
discipline of computing on a soroban you only imagine): variables, custom
functions with recursion and doc comments, lambdas with `map / filter /
reduce`, arrays/maps/strings, typed `data` records with `toJson`/`fromJson`, lazy
`if()`, LaTeX-style `∑`/`∏` — every value exact. The app hosts Anzan in the log and the grid; the `soroban` CLI is
Anzan without the app. The full specification lives in
[docs/ANZAN.md](docs/ANZAN.md) — and its promises are executable
(`Engine/Tests/…/Features/anzan.feature` pins the grammar in CI).

## Installing

[**Download Soroban**](https://github.com/alleato-llc/soroban/releases/latest/download/Soroban.dmg)
(signed & notarized), or grab a specific version from
[Releases](https://github.com/alleato-llc/soroban/releases); open it and drag
Soroban to Applications. Every merge to `main` ships a release automatically
(semantic version from the git tag), uploaded to the download host above — see
[docs/RELEASING.md](docs/RELEASING.md).

## Building

Requirements: Xcode 16+, [XcodeGen](https://github.com/yonaskolb/XcodeGen)
(`brew install xcodegen`).

```sh
xcodegen generate                          # produces Soroban.xcodeproj (gitignored)
xcodebuild -project Soroban.xcodeproj -scheme Soroban build
```

Or open `Soroban.xcodeproj` in Xcode and run.

## Command line (`soroban`)

The full Anzan language ships as a CLI — no app, no GUI, the identical
50-digit arithmetic:

```sh
cd Engine && swift build -c release --product soroban
install -m 755 .build/release/soroban ~/.local/bin/    # or anywhere on PATH
```

Three modes, chosen by shape:

```sh
soroban "0.1 + 0.2 == 0.3"                  # one-shot → 1
soroban "x = 3" "x^2 + 1"                   # arguments share a session → 3, 10
echo "pmt(0.05/12, 360, 200000)" | soroban  # pipe: plain output, exit 1 if any line fails
soroban                                     # interactive REPL
```

The REPL has tab completion (built-ins, constants, and your own
variables/functions), gray signature hints as you type `name(`, persistent
↑/↓ history across sessions (`~/.soroban_history`), and the full `man name`
documentation. `:mode programmer` (or `finance`/`normal`) switches the input
dialect — `^` reads as XOR, `&`/`|`/`<<`/`>>`/`~` as bitwise, `%` as modulo (see
[docs/MODES.md](docs/MODES.md)). `exit`, `quit`, or ⌃D leaves.

## Testing

The engine is a standalone SwiftPM package tested two ways: Swift Testing
unit suites, and hundreds of Gherkin scenarios (PickleKit) that read as
user-perspective specs — `Engine/Tests/SorobanEngineTests/Features/*.feature`,
including `anzan.feature`, the executable companion to the
[language spec](docs/ANZAN.md). Every documented example is itself evaluated
by the suite, and region coverage stays above ~90%:

```sh
cd Engine && swift test

# Coverage report (SorobanCLI is argument plumbing, excluded by design)
swift test --enable-code-coverage
xcrun llvm-cov report .build/arm64-apple-macosx/debug/SorobanEnginePackageTests.xctest/Contents/MacOS/SorobanEnginePackageTests \
  -instr-profile .build/arm64-apple-macosx/debug/codecov/default.profdata \
  -ignore-filename-regex "Tests|checkouts|SorobanCLI"
```

The app's session layer (undo, named-cell rewriting, control commits, CSV
export) has its own Gherkin bundle, run with
`xcodegen generate && xcodebuild test -scheme Soroban`.

Either Gherkin run can emit a Cucumber-style **HTML report** (collapsible
scenarios, per-step timing, status filters) — CI uploads both as build
artifacts on every run:

```sh
# Engine scenarios
cd Engine && PICKLE_REPORT=1 PICKLE_REPORT_PATH=$PWD/pickle-report.html \
  swift test --filter GherkinTests

# Session scenarios (xcodebuild forwards env only with the TEST_RUNNER_ prefix)
TEST_RUNNER_PICKLE_REPORT=1 TEST_RUNNER_PICKLE_REPORT_PATH=$PWD/session-report.html \
  xcodebuild test -project Soroban.xcodeproj -scheme Soroban -destination 'platform=macOS'
```

## Using it

| Input | Result |
|---|---|
| `0.1 + 0.2` | `0.3` (exactly) |
| `2(3 + 4)` | `14` — implicit multiplication |
| `x = 12 * 80.5` | assigns and shows `966` |
| `ans * 1.0825` | `ans` is the last result |
| `pmt(0.05/12, 360, 200000)` | monthly payment on a 30-year $200k loan at 5% APR |
| `irr(-70000, 12000, 15000, 18000, 21000, 26000)` | internal rate of return |
| `round(margin(100, 80), 2)` | gross margin %, rounded to cents |
| `√(2 + 2)`, `2π`, `6 × 7 ÷ 2` | math symbols work (√ π τ × ÷ − ·) |
| `∑(1, 2, 3)` | `6` — ∑ over a list is a plain sum (`∏(2, 3, 4)` → `24` likewise) |
| `∑_i=1^10(i^2)` | `385` — indexed summation, LaTeX-style (`sigma_i=1^10(i^2)` to type it) |
| `∏_i=1^25(i)` | 25! — exact to all 26 digits (`product_i=…` to type it) |
| `if(B:1 > 1000, B:1 * 0.1, 0)` | conditionals — comparisons return 1/0, branches are lazy |
| `fact2(n) = if(n <= 1, 1, n * fact2(n - 1))` | recursion works — bounded by memory, not a counter (missing base cases fail with a hint) |
| `sum(A:1..A:9)` | cell ranges — rectangles too (`A:1..B:9`); empty/text cells skipped |
| `date(2026, 6, 6) - date(2026, 1, 1)` | dates are exact day serials — subtract, compare, aggregate |
| `f(x) = x * 2` | defines a function — then `f(21)` is `42` |
| `"Q" + 1` | strings — `+` concatenates when either side is one (`concat(…)` too) |
| `arr = [1, 2, 3]` | arrays — `arr[0]` is `1` (0-based), `sum(arr)` works like a range |
| `{name: "Ada", age: 36}` | maps — read with `.age` or `["age"]`; nest freely (arrays of maps…) |
| `people[1].age` | structures compose: index, member access, functions, ∑ |
| `map(x -> x * 2, arr)` | higher-order functions — lambdas are values (`filter`, `reduce` too) |
| `= 1 + 2` | a leading `=` is tolerated (pasted cell formulas just work) |

- **Autocomplete** as you type: functions, your variables, and constants. **Tab** accepts (functions come with their opening paren), **↑/↓** pick a candidate while the list is open, **Esc** dismisses it
- **↑ / ↓** recall input history (persisted across launches) when the suggestion list is closed, **Esc** clears the line
- Select any log text to copy it; **right-click** an expression to edit it again or a result to insert the value

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| ⌘\\ | Toggle log ↔ grid (also: the button at the right of the input bar / floating bottom-right in grid mode) |
| ⌘N / ⌘O / ⌘S / ⇧⌘S | New / Open / Save / Save As workbook |
| ⇧⌘O | Open CSV as a new workbook (File ▸ Export CSV… writes the current sheet's values) |
| ⌘/ | Function Reference — searchable docs for every function with clickable, live-computed examples; with autocomplete open it jumps to the highlighted function (also: the book button in the input bar) |
| ⌘K | Clear the log |
| ⌘, | Settings (themes, font) |
| Tab | Accept autocomplete suggestion (input bar) |
| ↑ / ↓ | Suggestions when open, input history otherwise (input bar); move selection (grid) |
| Return | Submit (input bar); edit selected cell / commit + move down (grid) |
| Esc | Dismiss suggestions, then clear line (input bar); cancel edit, then deselect (grid) |
| ⌘Z / ⇧⌘Z | Undo / redo grid edits — content, formatting, and control interactions each undo as their own steps |
| ⌘B / ⌘I / ⌘U / ⇧⌘X | Bold / italic / underline / strikethrough the selection (grid; see Formatting) |
| ⌃⌘. / ⌃⌘, | Increase / decrease decimals on the selection (grid) |
| Shift-click / Shift-arrows | Extend the selection rectangle from the anchor (grid) |
| ⌘C / ⌘X / ⌘V / Delete | Copy / cut / paste / clear the selection (grid) — clipboard is TSV, so blocks paste to and from Excel/Numbers |

## Grid view

A 26×1,000 mini-spreadsheet (columns A–Z), toggled with ⌘\\. In grid mode the
expression input bar hides (its results belong to the log) and the view
toggle floats over the bottom-right corner. Reference cells as `A:1` (column
letter, colon, 1-based row) — in other cells *and* in the log's input bar;
the sheet and the log share one variable space.

Cells auto-detect their kind, with explicit markers when you want control:

| You type | The cell shows |
|---|---|
| `1200` | `1200` (number) |
| `Q1 revenue` | the text itself (labels never become errors) |
| `B:1 + B:2` | the computed value |
| `B:1 / 0` | `#ERR` (red highlight; hover for the message) |
| `=B:1 * rate` | **forced formula** — any failure, including a typo'd name, shows `#ERR` |
| `"123"` | **forced text** `123` (quotes stripped) — stays a label even though it looks numeric |

Empty cells read as `0` in formulas; referencing a text cell is an error;
circular references are detected. Cell formulas may use log variables
(`rate = 0.0825` in the log, then `B:3 * rate` in a cell) and cell evaluation
never disturbs `ans`.

**While editing a formula**, clicking another cell inserts its reference
(Excel's point mode): type `B:1 +`, click B:2, get `B:1 + B:2`. Clicking
again replaces the inserted reference; shift-click turns it into a range
(`B:1..B:4`). If the text ends with a complete value (or is plain text),
clicking commits and moves on, as before.

**Single click selects** a cell (highlight): arrow keys move the selection,
**⌘C/⌘X/⌘V** copy, cut, and paste the raw contents, **Delete** clears, and
**Return** opens the editor. **Double click edits** directly. While editing:
Return commits and moves down, Tab moves right, Esc cancels.

**Resize columns and rows** by dragging the right edge of a column header or
the bottom edge of a row number (a guide line previews; the size applies on
release); double-click a divider to reset. Layout saves with the workbook.

### Formatting

Select cells (single, or a shift-extended rectangle) and use the **Format
menu**, or **right-click** for Cut/Copy/Paste/Delete plus the same
formatting under its Format submenu — no toolbar:

- **Style**: Bold ⌘B · Italic ⌘I · Underline ⌘U · Strikethrough ⇧⌘X — if
  everything selected already has the style, toggling clears it
- **Alignment**: automatic (text left, numbers right) or forced Left ⌘{ /
  Center ⌘| / Right ⌘}
- **Colors**: text and fill from a small palette that adapts to light/dark
  themes
- **Number formats**: General · Number (`1,234,567.50`) · Currency (your
  locale's symbol by default, or $ € £ ¥ — the symbol is stored, so the
  workbook renders the same everywhere) · Percent (`0.0825` → `8.25%`,
  exactly) · Date (day serials render as `2026-06-06`) · Hex / Binary
  (integers render as `0xC3` / `0b1100_0011` — the honest "programmer
  mode": display flips, the value and every reference stay exact decimal)
  — plus Increase/Decrease Decimals (⌃⌘. / ⌃⌘,) and Clear Formatting

Formatting is **display-only**: the underlying value stays exact, formulas
and TSV copy/paste see the raw value, and formats save with the workbook.
Empty cells can be formatted too (fill a region before its data arrives).

Untitled scratch work auto-persists to
`~/Library/Application Support/Soroban/sheet.json` (inside the sandbox
container) — including variables, functions, and layout.

### Numbers

Arithmetic (`+ − × ÷`, integer `^`, postfix `%` percent, `mod()`) is exact to 50
significant digits (`/` and `sqrt` round to that precision). Transcendentals (`exp`, `ln`, `log`, trig, non-integer
powers) currently round-trip through Double (~15 significant digits); they're
isolated behind one seam in `BigDecimal+Math.swift` for a future
arbitrary-precision upgrade.

### Functions

Case-insensitive. `ans`, `pi`, `tau`, `e` are built-in constants.

- **Core**: `abs min max round floor ceil trunc sqrt cbrt root pow mod fact choose perm gcd lcm percent exp ln log10 log`, plus `solve(f, target, guess)` — goal seek as a formula (`solve(x -> x^2, 2)` is √2)
- **Logic**: `if(cond, then, else)` (lazy branches), `not and or`; comparisons `< > <= >= == !=` (also `≤ ≥ ≠`) return 1/0
- **Trig** (radians): `sin cos tan asin acos atan atan2`, hyperbolics `sinh cosh tanh asinh acosh atanh`, and `deg rad` conversions at full precision (`deg(pi)` is exactly 180)
- **Finance** (spreadsheet sign convention): `pv fv pmt nper rate npv irr effectiveRate nominal`, and the amortization split `ipmt ppmt cumipmt cumprinc` (`ipmt + ppmt = pmt`, exactly, every period)
- **Dates** (exact day serials since 1970-01-01): `date today year month day weekday weeknum quarter edate eomonth days`, business days `workday networkdays` (holidays as extra arguments), plus `xnpv xirr` for irregular cash flows (`xirr(A:1..A:5, B:1..B:5)` — dates first, flows second)
- **Accounting**: `markup margin percentOf percentChange`, depreciation `sln syd ddb`, and **fixed-precision decimals** `Decimal(value, precision, scale)` (SQL DECIMAL(p,s) / money — rounds to scale, checked precision ≤ 1000, `Rounding.Bankers`/`Rounding.HalfUp`), with short forms `Decimal(value)` (exact capture at max precision) and `Decimal(value, scale)`. See [docs/DECIMAL.md](docs/DECIMAL.md)
- **Stats** (variadic, range-friendly): `sum product count avg median stdev variance mode geomean sumproduct`, `percentile(data…, p)`, and the regression set `correl slope intercept forecast` (paired series split evenly — pass two equal ranges)
- **Data & Text**: `len first last keys values concat sort unique reverse seq toJson fromJson`, plus the higher-order `map filter reduce` (take a lambda `x -> x * 2` or a function name) and `list(…)` — collect a range into an array, so `sum(filter(x -> x > 10, list(A:1..A:9)))` works
- **Programmer**: hex/binary literals (`0xFF`, `0b1010` — exact at any width), `toBase fromBase` (bases 2–36), arbitrary-width bit math `bitAnd bitOr bitXor bitShift bitNot`, and **fixed-width integer types** — per-width `Int8…Int256` / `UInt8…UInt256` (e.g. `Int32(v)`), or parameterized `Int(v, bits)` / `UInt(v, bits)` (8–256 bits, signed/unsigned — exact and *checked*: overflow is an error, never a wraparound). In **Programmer mode** the bit functions read as the C operators `^ & | << >> ~` (with `%` as modulo) — a display *dialect*; in the default dialect `^` stays power and `%` percent. Programmer mode also reveals a **binary bit-editor** (⌥⌘B, dismissable) that shows the last result as a clickable bit grid — flip bits to build a value, then **Use** it (or double-click the decimal/hex) to drop it into the expression you're typing (a typed `Int…` uses its own width; a plain integer picks 8–256 bits). Apply a **format** — Unix permissions, TCP flags, RGB565, or a custom `owner:3 group:3` spec — to label bit ranges as colored fields you can read and edit by value; a format is just a map (`{owner: 3, …}`), so saving it persists an ordinary variable in the workbook. And a leading operator continues the last result, SpeedCrunch-style: typing `+5` on an empty line becomes `ans+5`. See [docs/MODES.md](docs/MODES.md) and [docs/FIXED-WIDTH.md](docs/FIXED-WIDTH.md)
- **Controls**: `slider stepper checkbox dropdown` — pure functions in formulas, interactive controls in cells (see Controls section)

**Cell ranges**: `sum(A:1..A:9)`, rectangles like `avg(A:1..C:10)` — usable in
any function, in the log and in cells. Empty and text cells are skipped
(Excel-style), so `count`/`avg` over sparse columns behave; error cells
propagate.

### Summation & products (∑ ∏)

Each symbol has two forms, mirroring math notation:

- **Plain list**: `∑(…)` is just `sum(...)` and `∏(…)` is `product(...)` —
  `∑(1, 2, 3)` → 6, `∏(2, 3, 4)` → 24, `∑(B:1, B:2)` over cells.
- **Indexed**: `∑_i=1^10(i^2)` → 385 and `∏_i=1^5(i)` → 120 — the index runs
  from the lower bound to the upper bound, re-evaluating the parenthesized
  term each time. Type `sigma_i=…` / `product_i=…` if the symbols are out of
  reach (those name prefixes are reserved).

Indexed bounds are a number, variable, cell, or parenthesized expression —
compound bounds need parens, the plaintext version of LaTeX braces:
`∑_i=(n-1)^(n+1)(i)`. The forms nest and compose with custom functions
(`triangle(n) = ∑_i=1^n(i)`; `∏_i=1^n(1 + r)` is textbook compound growth).
An empty range (from > to) is 0 for ∑ and 1 for ∏; ranges cap at 100,000
terms.

### Comments & documenting your functions

`#` starts a comment anywhere: `100 * 1.0825  # with tax`. A trailing comment
on a function definition becomes its **documentation** — shown by `man()`,
the reference window, and autocomplete:

```
tax(x) = x * 1.0825   # TX sales tax on a subtotal
```

Because the doc lives in the definition line, it saves into workbooks
automatically and updates whenever you redefine the function.

### man

`man pmt` (or `manual pmt` / `help pmt` — unix-style, no parentheses) prints a
function's signature, summary, and examples straight into the log — built-ins,
special forms (`man if`, `man sigma`), and your own documented functions alike.

### Function Reference

**⌘/** opens a searchable reference window covering every built-in function,
the special forms (∑ ∏ if), operators, and constants — each with a signature,
explanation, and clickable examples whose results are computed live. Your own
functions appear at the top automatically as you define them. While
autocomplete is open, the footer shows the highlighted function's signature,
and ⌘/ jumps straight to its full entry. Documentation is enforced by the
engine's test suite: a function cannot be added without docs, and every
example must evaluate.

### Custom functions

Define your own in the log: `f(x) = x * 2`, `area(w, h) = w * h`. They
compose (`g(x) = f(x) + 1` — `f` resolves at call time, so definition order
doesn't matter), parameters shadow variables, calls are case-insensitive,
and they work inside grid cells (`f(B:1)`). Built-in names are protected —
`abs(x) = x` is an error — and runaway recursion is cut off cleanly.
Custom functions are saved in workbooks alongside cells and variables.

## Structured values

The log speaks more than numbers — strings, arrays, and maps are first-class
values that nest freely and persist in workbooks like any variable:

```
greeting = "hello"                       # strings; + concatenates: "Q" + 1 → "Q1"
arr = [1, 2, 3]                          # arrays; arr[0] → 1 (0-based)
person = {name: "Ada", age: 36}          # maps; person.age or person["age"]
people = [{name: "Bob", age: 32}, person]
people[1].name                           # → "Ada"
sum(arr)                                 # numeric functions take arrays like ranges
∑_i=0^2(arr[i] ^ 2)                      # ∑ + indexing = iteration
total(m) = m.price * m.qty               # functions take and return structures
map(x -> x * 2, arr)                     # → [2, 4, 6] — lambdas are values
filter(x -> x > 1, arr)                  # → [2, 3]
reduce((a, b) -> a + b, arr, 0)          # → 6 — fold left from an initial value
map(sqrt, [1, 4, 9])                     # a bare function name is a value too
scale(xs, n) = map(x -> x * n, xs)       # lambdas close over parameters
f = x -> x * 2                           # …or live in variables: f(21) → 42
```

`len` / `first` / `last` / `keys` / `values` / `concat` / `toJson` /
`fromJson` / `map` / `filter` / `reduce` live in the **Data & Text**
reference category; `true` /
`false` are constants for 1/0. `==` is deep equality (map key order doesn't
matter). Values are immutable — rebind the variable rather than assigning to
an element.

### Data types

When a shape repeats, declare it once and let the constructor keep every
instance honest:

```
data Person { name: String, age: Number, active: Boolean }   # a teammate
p = Person(name: "Ada", age: 36, active: true)   # named fields…
q = Person({name: "Grace", age: 30, active: false})  # …or one map; never positional
p.age                                    # records read like maps
team = [p, q]
sum(map(x -> x.age, team))               # collections + HOFs just work
filter(x -> x.active, team)
toJson(p)                                # pretty-printed by default — and the log
                                         # shows it as a real block, not \n escapes
toJson(p, Json.Compact)                  # {"name":"Ada","age":36,"active":true}
Person(fromJson(toJson(p))) == p         # fromJson parses JSON back — numbers stay
                                         # EXACT (never through floating point)
```

**Records nest** — a field can be another data type:

```
data Point { x: Number, y: Number }
data Line  { a: Point, b: Point }        # a field can be another data type
seg = Line(a: Point(x: 1, y: 1), b: Point(x: 4, y: 5))
seg.b.x - seg.a.x                        # → 3   (drill in: seg.b.x)
seg == Line(a: Point(x: 1, y: 1), b: Point(x: 4, y: 5))   # → 1, equal by ALL state
toJson(seg, Json.Compact)                # {"a":{"x":1,"y":1},"b":{"x":4,"y":5}}
```

**Write functions and operators for your types.** A type-annotated parameter
(`p: Point`) makes a function dispatch by argument type, and a definition
named with an operator symbol overloads that operator — gated so core
arithmetic is never touched:

```
length(s: Line) = sqrt((s.b.x - s.a.x)^2 + (s.b.y - s.a.y)^2)   # a "method"
+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)       # operator overload
*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)          # mixed with a scalar
Point(x: 1, y: 2) + Point(x: 10, y: 20)  # → Point(x: 11, y: 22)
Point(x: 1, y: 2) * 3                     # → Point(x: 3, y: 6)
1 + 2                                     # → 3   (built-in `+` untouched)
length(seg)                               # → 5
```

Scalar field types are `Number`, `String`, `Boolean` (strict: `active: 7` is
an error, and `toJson` emits real `true`/`false` because the type says so), or
**another declared data type** for nesting. Missing, extra, and mistyped
fields fail with named errors. Every record gets structural `==`/`!=` for
free; an operator overload must involve at least one data type
(`+(a: Number, b: Number)` is rejected). A trailing `# comment` documents the
type (`man Person`); types, record variables, and overloads save in
workbooks. In the grid, a plain `data … { … }` cell is a sheet-scoped
**𝑫** definition, next to λ and 𝑖.

In the **grid**, cells stay scalar: a formula that returns a string displays
as text (`="Q" + quarter` makes a label), while an array or map in a cell is
an error — aggregate it (`=sum(arr)`). Ranges remain the grid's native
array: `sum(B:1..B:100)`.

## Named cells

Right-click any cell → **Name Cell…** and give it a name (≤64 characters,
unique per sheet). Formulas — and the log — then read like prose instead of
coordinates:

```
'Projected Rate' * 12                 # the named cell on this sheet
Budget!'Projected Rate'               # qualified, like Budget!A:1
'Q1 Budget'!'Loan Amount'             # both quoted — single quotes always
                                      # mean "the name of a thing"
=pmt('Rate'/12, 360, 'Loan Amount')
```

Unqualified names follow the same rule as `A:1`: a formula's own sheet, the
active sheet from the log. In point mode, clicking a named cell inserts its
**name** instead of its address. **Renaming auto-updates** every referencing
formula; **removing** a referenced name asks — break the references, replace
the name with the cell address everywhere, or cancel. Everything is
undoable: ⌘Z walks back the formula rewrites and the name change itself, in
an order that always lands coherent. Names save with the workbook, and
dependency tracking flows through them (change the cell, every reader
updates).

A name labels the *location* (whatever the cell holds — data, a formula
result, a slider); a 𝑖 definition names a *value*. They're complementary:
name your `=pmt(…)` output "Projected Payment" and build on it elsewhere.

## Controls (what-if sliders, checkboxes, steppers, dropdowns)

Type a control expression into a cell and it becomes interactive:

```
rate = slider(0.08, 0, 0.2)              # drag — live recalc, step defaults to range/100
n = stepper(5, 1, 20)                    # − / + buttons, step defaults to 1
flag = checkbox(true)                    # click to toggle; evaluates to 1/0
region = dropdown("EU", ["EU", "US", "APAC"])   # menu; the cell's value IS the selection
=slider(5, 0, 10)                        # anonymous forms work too — read as the cell (A:1)
```

Interacting rewrites the value literal in the cell's own text
(`rate = slider(0.11, 0, 0.2)`) as **one undoable edit** — comments and
spacing survive — and everything reading the value recalculates (sliders
update live mid-drag). Values display through the cell's number format
(a percent-formatted rate slider reads `11.00%`); dropdown options can be
strings (`if(region == "EU", …)`) or numbers. Controls are just cells: they
save as plain text, evaluate headlessly (`slider(v, lo, hi)` is `v` clamped,
`checkbox(s)` is 1/0, `dropdown(v, opts)` is `v`), and named ones are
sheet-scoped 𝑖 definitions, immutable from the log. Press Return on a
selected control to edit its expression. The arguments must be literals —
the value argument *is* the storage.

## Sheet-scoped definitions (λ / 𝑖 cells)

Type a definition *plainly* into a cell (no `=` marker) and it becomes part
of that sheet, not data on it:

| Cell content | Renders as | Meaning |
|---|---|---|
| `tax(x) = x * A:1  # uses the rate in A:1` | *λ tax(x)* | a function scoped to this sheet — bodies can read cells |
| `rate = 0.0825` | *𝑖 rate* | a sheet variable — the expression may reference cells, and re-evaluates as they change |
| `data Pt { x: Number, y: Number }` | *𝑫 Pt* | a data type scoped to this sheet — formulas construct with `=Pt(x: 3, y: 4).x` |

Formulas on the same sheet (and the log, while that sheet is active) use the
names directly: `=tax(B:2)`, `=100 * rate`. Each sheet is its own namespace —
two sheets can define different `rate`s. Cell-defined names are **owned by
their cells**: assigning one in the log says so ("'rate' is defined in cell
A:3 — edit that cell to change it"), and they shadow same-named log
variables. Defining a name twice on one sheet errors in the later cell;
built-in names stay protected. A definition's trailing `# comment` is its
documentation — `man tax` finds it. Referenced numerically, definition
cells behave like text (skipped in ranges); the definitions live in the
workbook as ordinary cell contents, so they save, load, and undo like
everything else.

## Worksheets

A workbook holds up to **256 worksheets**. The UI stays minimal: in grid
mode, a bottom strip shows only the **active** tab — click its name for a
menu of all sheets, **+** adds one, **−** removes (with confirmation;
formulas referencing a removed sheet show errors), and **double-click
renames** inline. Names run up to 128 characters; long ones truncate to the
window with the full name in a tooltip. Sheet names can't contain `!` or `'`.

Formulas reference other sheets Excel-style — from cells *and* from the log:

```
Budget!A:1 * 2
sum('Q1 Budget'!B:1..B:12)
if(Costs!A:1 > Revenue!A:1, 1, 0)
```

Unqualified references (`A:1`) always mean the sheet the formula lives on;
in the log they follow the active tab. Cross-sheet circular references are
detected. Renaming a sheet does not rewrite formulas — references are by
name, so stale ones show "unknown sheet". Undo jumps to the sheet where the
edit happened.

## Inspecting the workbook

A formula can read the workbook's *own* structure — handy when a calculation
should adapt to however many sheets it finds, or pull a value by computed
address. Reading is **read-only**; a small set of **log-only commands** changes
the workbook (see below).

An object graph rooted at `Workbook`:

```
Workbook.count                          # how many sheets
Workbook.sheetNames                     # ["Sheet 1", "Budget"]
Workbook.worksheets[0].name             # by position (-1 = last)
Workbook.worksheets["Budget"].name      # by name
Workbook.worksheets["Budget"].cell("B", 1).value * 2
```

…and flat accessors for the everyday reads:

```
cell("A", 1).value                      # a cell on this sheet
cell("Budget", "A", 1).value            # …or another, by name
sheetName()                             # the current sheet's name
sheetNames()                            # every sheet name
rowCount()   columnCount()              # the grid's size
```

A cell handle exposes `.value` (its number — an error if it isn't one, like a
direct reference), `.text` (what it displays), `.raw`/`.formula` (its source),
`.address`, and `.isEmpty`. Reads are **live**: `=cell("A", 1).value + 1`
recomputes when `A:1` changes, exactly like `=A:1 + 1`. Your own
`cell(x) = …` definition shadows the accessor. (In the `soroban` CLI there's
no workbook, so `Workbook` and `cell()` are simply unknown.)

To *change* the workbook, type a command **in the log** (not a cell — a cell
recalc must stay reproducible). Each is one undoable step:

```
updateCell(cell("A", 1), 99)            # a number…
updateCell(cell("A", 1), "=B:1 * 2")    # …a formula, or "" to clear
addWorksheet("Budget")                  # append a sheet
renameWorksheet("Budget", "Costs")      # rename + rewrite every reference
deleteWorksheet("Costs")                # remove (won't remove the last sheet)
```

### The calculation log (`History`)

`ans` is the last result; `History` is the *whole* tape — an array of entry
handles you can query from the **log** (it's `ans`, generalized):

```
last(History).value                     # the last result (first(History) = oldest)
History[2].input                        # what you typed on line 3
sum(map(entry -> entry.value, History)) # add up every result so far
filter(entry -> entry.isError, History) # the lines that failed
```

Each entry has `.input` (what you typed), `.value` (its result — a number or
string), `.text` (what it displayed), `.kind` (`"value"`/`"error"`/`"comment"`/
`"info"`/`"function"`/`"datatype"`), `.isError`, `.referencesCells` (did it read
the grid?), and `.note` (a trailing `# comment`). For the *size* of the log use
`len(History)`; dumping bare `History` prints its entries and is recorded
display-only (`"info"`). `History` is **read-only** and
**log-only** — in a *cell* it's just a text label (so a column headed
`History` is fine), because the log is session history, not part of the
document. (`e` is reserved for Euler's number, so name lambda parameters
`entry`, not `e`.)

## Data sheets (CSV import)

**Sheet ▸ Import Data (CSV)…** copies a CSV into a **data sheet** — records
live in the package's SQLite store, not the JSON manifest, and are read
lazily, so 100,000-row imports neither slow opens nor bloat the file. Data
sheets can exceed the grid's 1,000 rows (`sum(sales!C:2..C:50000)` works
from any formula or the log); the grid browses the first 10,000 rows.
Try it: import `examples/sales.csv`, then `sum(sales!C:2..C:7)`.

Import is a *copy*: data sheets are editable, and edits go to the workbook's
own database — the source CSV is never touched. Data cells hold values, not
formulas, and the table's shape is fixed (you can't type past its last row
or column). *Linked* data sources — live read-only views of an external file
— are on the roadmap.

Three CSV doors, three jobs: **File ▸ Open CSV…** (⇧⌘O) starts a NEW
workbook from a CSV — files that fit the grid arrive as ordinary editable
cells, Excel-style; bigger ones become a data sheet automatically.
**Sheet ▸ Import Data (CSV)…** adds a data sheet to the CURRENT workbook.
**File ▸ Export CSV…** writes the current sheet's computed *values*
(numbers plain, controls as their value, definitions as their source) —
the interop convention: formulas don't survive a CSV.

## Workbooks

Save your whole session — grid cells, variables, custom functions (with their
doc comments), and column/row layout — as a `.soroban` file: **⌘S** Save,
**⇧⌘S** Save As, **⌘O** Open, **⌘N** New. The window title shows the current
workbook and an "— Edited" marker for unsaved changes; quitting with unsaved
changes prompts to save. Untitled scratch work auto-persists across launches.

On disk a workbook is a **package**: `workbook.json` (the diffable model)
plus `data.sqlite` when data sheets exist — see
[docs/FORMAT.md](docs/FORMAT.md). Finder double-click opens `.soroban`
documents directly. A worked example lives at
[examples/mortgage.soroban](examples/mortgage.soroban) (⌘O it).

```json
{
  "format" : "soroban-workbook",
  "version" : 1,
  "cells" : { "A:1" : "Q1 revenue", "B:1" : "1200", "B:3" : "=B:1 * rate" },
  "variables" : { "rate" : "0.0825" },
  "functions" : { "tax" : "tax(x) = x * 1.0825 # TX sales tax" },
  "columnWidths" : { "A" : 140 },
  "rowHeights" : { "3" : 36 }
}
```

## Themes

Pick a theme in Settings (⌘,). Ten ship built-in — dark: Soroban Dark,
Solarized Dark, Terminal Green, Dracula, Nord, Gruvbox Dark; light: Soroban
Light, Solarized Light, GitHub Light, One Light. Drop your own JSON into
`~/Library/Application Support/Soroban/Themes/` (restart to load):

```json
{
  "name": "My Theme",
  "windowBackground": "#1E1E28",
  "inputBackground": "#2A2A38",
  "expressionText": "#9DA5B4",
  "resultText": "#E6E6F0",
  "errorText": "#FF6B6B",
  "secondaryText": "#6C7086",
  "accent": "#7AA2F7",
  "fontName": "JetBrains Mono",
  "fontSize": 14
}
```

`fontName` is optional (defaults to the system monospaced font).

Settings also has app-level **font family and size** controls (monospaced
fonts only — column alignment depends on fixed pitch). They override the
active theme's font and survive theme switches; Reset returns to the theme's
own font.

## Layout

- `Engine/` — one SwiftPM package, two library modules plus the CLI, no UI
  dependencies. `Sources/Anzan/` is the **language**: `BigDecimal` (BigInt
  significand × 10^exponent), lexer, Pratt parser, evaluator, function
  registry, autocomplete, docs. `Sources/SorobanEngine/` is the **hosting
  layer** — the `Spreadsheet`/`Cell` calculation model and the `Workbook`
  file codec — and re-exports Anzan. `Sources/SorobanCLI/` (the `soroban`
  binary) depends on Anzan alone. Everything testable with `swift test`.
- `App/` — SwiftUI app: calculation log, input bar with autocomplete, the
  grid, workbook open/save, theming + font settings.
- `site/` — the landing page (Astro + Preact + TypeScript, static): palettes
  mirror the app's themes, the hero carousels real app screenshots, and it
  deploys to a static host via `deploy-site.yml`. Its own docs live in
  [site/README.md](site/README.md); site-only commits use `[skip ci]` so they
  don't spend a release version.
- `project.yml` — XcodeGen definition; the `.xcodeproj` is generated
  (rerun `xcodegen generate` after adding files).

## Roadmap

In rough order: **unit & currency support** (`10 USD + 5 EUR`, `3h +
20min`), **linked data sources** (a data sheet that references an external CSV in
place — read-only, chain-link badge, re-read on open — vs. today's
import-as-copy), Excel-style **array spilling** into neighboring cells,
Finder double-click for `.soroban` files + recent-files menu, a grid-mode
formula bar, true arbitrary-precision transcendentals, and a CLI for running
workbooks headlessly.

## License

[MIT](LICENSE) © Alleato LLC.
