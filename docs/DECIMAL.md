# Fixed-precision decimals — `Decimal(value, precision, scale)`

> **Status: implemented.** The type, the `Decimal(value, precision, scale[,
> Rounding.X])` constructor, the `Rounding` constant, checked arithmetic (mixing
> matrix + overflow + Number-absorb), and the half-up rounding helper are landed
> and tested (`FixedDecimalTests` + `anzan.feature` scenarios). It is the
> **finance/decimal-digits** axis — the sibling of `docs/FIXED-WIDTH.md`'s
> *binary-bits* `Int`/`UInt`. **Resolved open questions:** registered under the
> **Accounting** reference category (Q1); `scale 0` is allowed — a base-10
> rounded integer-valued decimal (Q3). **Deferred:** `^`-power on a decimal
> (errors for now — Q2), typed-param dispatch (Q4), and the currency-display
> tie-in (Q5 — since overtaken: finance mode was retired and the `$10` currency
> literal is core grammar, see `docs/MODES.md`).

## What it is

A **bounded, checked decimal** with a fixed number of fractional digits — the
SQL `DECIMAL(p, s)` / money type. `Decimal(10.5, 5, 2)` is `10.50`: at most 5
significant digits total, exactly 2 after the point.

- **`precision`** — the maximum *total* significant digits (integer + fractional).
  The integer part may hold up to `precision − scale` digits; exceeding it is an
  **overflow error** (the checked-range contract, exactly like `Int`/`UInt`).
- **`scale`** — the number of fractional digits. The value is **rounded** to this
  many places on construction and after each operation.

It sits beside the unbounded exact `Number` (the default) and the binary `Int`/
`UInt`. Where bounded integers preserve exactness with a *range* constraint,
fixed-precision decimals add **controlled rounding** to a declared scale — a
*deliberate*, predictable departure from "always exact" (decimal rounding, never
IEEE float). That's why it's the on-mission finance type.

## Constructor

```
Decimal(value)                                   # scale from the value, precision = max
Decimal(value, scale)                            # that scale, precision = max
Decimal(value, precision, scale)                 # rounding defaults to banker's
Decimal(value, precision, scale, Rounding.HalfUp)
```

- **Short forms.** `Decimal(value)` captures the value *exactly* — its own number
  of decimal places becomes the scale, and the precision defaults to the **max
  (1000)**. It never rounds (`Decimal(3.14159)` keeps all five places) and the
  generous precision means ordinary arithmetic won't overflow. `Decimal(value,
  scale)` pins the scale you ask for (rounding to it is *explicit*, not silent),
  again at max precision. At max precision the canonical form **hides the
  precision** — `Decimal(0.5)` recalls/copies/persists as `Decimal(0.5)`, and
  `Decimal(0.5, 2)` as `Decimal(0.50, 2)` — so the default never clutters the
  round-trip (it stays re-parseable to the same value).
- `1 ≤ precision ≤ 1000` and `0 ≤ scale ≤ precision`, both integers (validated;
  out of range errors; the `1000` ceiling matches PostgreSQL's declared `NUMERIC`).
  Since `scale ≤ precision`, the **maximum scale is 1000** (reached at
  `precision 1000` with a pure-fraction value); the cap also keeps the internal
  `10^precision` range check bounded.
- The optional 4th argument is the **rounding mode**, a member of the reserved
  constant `Rounding` — `Rounding.Bankers` (default) or `Rounding.HalfUp` — modeled
  exactly like `Json.Pretty`/`Json.Compact` (a constant map of plain string values,
  no new value machinery; `man Rounding` documents it).

Examples:

| call | result | why |
|---|---|---|
| `Decimal(10.5, 5, 2)` | `10.50` | padded to scale |
| `Decimal(1.005, 5, 2)` | `1.00` | banker's (round-half-even) |
| `Decimal(1.005, 5, 2, Rounding.HalfUp)` | `1.01` | half-up |
| `Decimal(12345, 4, 0)` | **error** | 5 digits > precision 4 |
| `Decimal(1234.5, 5, 2)` | **error** | rounds to `1234.50` → 6 digits > 5 |

## Semantics: checked + controlled rounding

- **Construction**: round `value` to `scale` places using the mode, then verify
  the result fits `precision` (`|unscaled| < 10^precision`) — else **overflow
  error**. No silent precision loss on the integer side; the fractional side is
  *deliberately* rounded (that's the type's job).
- **The rounding mode is carried with the value** and used for every later
  rounding of a result of this type — so `Rounding.HalfUp` stays half-up through
  the whole computation, not just at construction. It also persists (below).

## The mixing matrix

| operands | result | rule |
|---|---|---|
| `Decimal` ⊕ `Decimal`, same rounding | scale = **max**, precision = **max** | round to result scale; **overflow → error** |
| `Decimal` ⊕ `Decimal`, **different rounding** | **error** | rounding never reconciles (cast one) — the analogue of sign in `Int` |
| `Decimal` ⊕ `Number` | the decimal's `(precision, scale, rounding)` | the Number is folded in exactly, then **rounded to the decimal's scale** (the "money stays at N places" model) |
| `Decimal` ⊕ `Int` / `UInt` | **error** | two different bounded families don't mix — cast explicitly |

One-liner: **scale and precision promote toward the widest declared; rounding
never reconciles; a plain `Number` is absorbed and rounded to the decimal's
scale; results are range-checked against the result precision and error rather
than silently lose integer digits.**

Operations: `+ − × ÷` (and comparison). Each computes the exact result, rounds to
the result scale, and overflow-checks the result precision. `÷` rounds to scale
naturally (`Decimal(10,5,2) / Decimal(3,5,2)` → `3.33`). Bitwise/`^`-power don't
apply to decimals (error). Like `Int`, sums can overflow a tight precision
(`Decimal(999.99,5,2) + Decimal(0.01,5,2)` → `1000.00` → overflow) — **size
`precision` for your totals**, the same discipline as choosing an int width.

> **Deliberate simplification vs. SQL.** SQL *derives* a wider result precision so
> sums/products rarely overflow. We instead take `max(precision)` and **error on
> overflow** — the same "declared bound is a hard ceiling, tell me loudly" ethos
> as fixed-width integers, rather than two precision-derivation regimes. Likewise
> result scale is `max(scale)` for every op (not SQL's grow-on-multiply), so the
> scale never silently explodes.

## Coercion, display, persistence

- **Outside typed arithmetic** a `Decimal` reads as its numeric value (via
  `asNumber`/`flattenedNumbers`) — so comparison, truthiness, `sum`, and cell
  references treat it as the number it represents; `==` is numeric
  (`Decimal(10.50,5,2) == 10.5` is true). Same model as `Int`/`UInt`.
- **Display** pads to scale: `Decimal(10.5, 5, 2)` shows `10.50` (unlike a plain
  `Number`, which normalizes to `10.5`). In a grid cell it shows its value and
  pairs with the currency/number `CellFormat` for presentation.
- **Persistence**: round-trips as the constructor — `Decimal(10.50, 5, 2)` (with
  the rounding arg only when non-default), restored by re-evaluation like a
  record or a fixed-width int (the `Decimal` builtin is always present, so no
  ordering dependency).

## Implementation shape

Ecosystem-neutral, and a direct parallel to the fixed-width integer
([FIXED-WIDTH.md](FIXED-WIDTH.md)):

- A **fixed-decimal value** carries `value` (an exact decimal, already rounded and
  in range), `precision`, `scale`, and `rounding`, as the payload of a dedicated
  `Value.fixedDecimal` case (touching the same exhaustive `Value` switches the
  fixed-int case does).
- A `Decimal(...)` builtin, with `Rounding` as a reserved constant map (the way
  `Json` is a reserved constant).
- Arithmetic is **intercepted before ordinary numeric coercion**, alongside the
  fixed-int hook.
- Rounding routes through the engine's existing banker's-rounding path; half-up
  adds one rounding helper. **No float anywhere** — stays exact-to-scale.

For the concrete types and files, see the ecosystem docs:
[../swift/docs/ENGINE.md](../swift/docs/ENGINE.md) and
[../rust/docs/ANZAN.md](../rust/docs/ANZAN.md).

## Open questions for the build

1. **Home/category** — register `Decimal` under a new **Accounting/Finance**
   reference category, or `Programmer`? (Leaning a finance-facing category.)
2. **`^`-power on a decimal** — error (proposed), or support integer exponents
   with scale-rounding (`Decimal(1.05,…) ^ 12` for compound growth)? The finance
   use case is real; could be a fast follow.
3. **Negative-scale / scale 0** — `scale = 0` is a bounded integer-valued decimal
   (distinct from `Int`: base-10, rounds, no two's-complement). Confirm allowed.
4. **Dispatch** — let `Decimal` appear in typed-param annotations
   (`f(x: Decimal) = …`)? Same deferred question as the per-width `IntN`.
5. **Currency-display tie-in** — largely overtaken: finance mode was retired
   and the `$10` literal (the first-class `Money` type) is core grammar in
   every mode. What remains open is only whether cells should surface
   `Decimal` more ergonomically. (Cross-ref `docs/MODES.md`; out of scope for
   this type's core.)
