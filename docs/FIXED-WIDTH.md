# Fixed-width integer types — `Int` / `UInt`

> **Status: implemented.** The type, the constructors (parameterized
> `Int(value, bits)` / `UInt(value, bits)` **and** per-width `Int32(value)` /
> `UInt8(value)` …), checked arithmetic (mixing matrix + overflow + implicit
> literal typing), and two's-complement bitwise (`& | ^ << >>` and the `~`
> operator) are landed and tested (`FixedIntTests` + `anzan.feature`). It is the
> **binary-bits** axis — the sibling of `docs/DECIMAL.md`'s *decimal-digits*
> `Decimal`. **Deferred:** typed-param dispatch (`f(x: Int32) = …`); radix display
> following the mode/`CellFormat` (a value currently always renders its canonical
> per-width form).

## What it is, and what it is not

A family of **bounded, checked integer value types** — signed and unsigned at
widths 8/16/32/64/128/256. Two equivalent spellings:

- **Parameterized:** `Int(value, bits)` / `UInt(value, bits)` — width as an
  argument: `Int(27374, 32)`, `UInt(255, 8)`.
- **Per-width:** `Int8(value)` … `Int256(value)`, `UInt8(value)` … `UInt256(value)`
  — reads like a Swift type: `Int32(343353)`. `Int32(x)` ≡ `Int(x, 32)`.

Both produce the same value; the **canonical (re-parseable) form is per-width**
(`Int32(343353)`), so the parameterized form normalizes to it on display.

- **Not a static/formal type system.** Anzan is dynamically typed: a `Value`
  carries its type at runtime (number/string/array/map/record/function/host), and
  `data` records already added a *nominal* type. `Int`/`UInt` are simply more
  built-in numeric value types in that system — checked at *evaluation* time, not
  statically inferred.
- **Does not replace `Number`.** The default `Number` is **unbounded and exact**
  (`BigInt` significand × 10^exp); it never overflows and *is* the "bignum".
  `Int128` is **bounded** (128 bits, checked) — a different axis.
- **Preserves exactness.** A bounded integer is still exact — we only add a *range
  constraint* + *checked overflow*. (That's why fixed-width fits the language's
  identity where IEEE float doesn't.)

## Why it earns its place

- The honest home for **bitwise NOT (`~`)** and **signed shifts**, which need a
  width to be well-defined (`~5` is 250 / 4294967291 / −6 depending on it).
- A **domain assertion**: "this computation must stay in 32 bits — or tell me
  loudly." Checked-range discipline (closer to Ada's range types than C's machine
  words), which fits an exact-arithmetic calculator better than silent wraparound.

## Relationship to modes (orthogonal)

A fixed-width value exists in **any** presentational mode — it's a value, not a
dialect. The mode affects only (1) what the `^`/`&`/`<<` glyphs parse to, and
(2) the radix it would display in. The **type identity is intrinsic and
mode-invariant**: `Int32(255)` round-trips as `Int32(255)`.

## Semantics: checked, not modular

**Overflow is always an error, never a wrap.** `Int8(127) + 1` throws; it does not
roll to `MIN`. This removes the C-style wraparound / modular-hash use case on
purpose — that inexact, surprising behavior fights the language's exact, loud
ethos. Bitwise AND/OR/XOR/NOT of in-range values are in-range by construction;
`+ − * pow <<` are range-checked and error on overflow.

## The mixing matrix

| operands | result | rule |
|---|---|---|
| same type | that type | overflow → error |
| same sign, different width | **largest width** | overflow → error |
| **different sign** | **error** | explicit cast required |
| **`Number` (decimal) + fixed-width** | **error** | explicit cast required |
| typed + plain integer literal | the typed type | literal adopts sign+width; out-of-range → error |
| count slot (exponent / shift / index) | — | sign-neutral plain integer, exempt |

One-liner: **width promotes toward the widest declared type (conventional, like
C/Java/Rust); sign never promotes; `Number`↔fixed never mixes implicitly; every
result is range-checked against the widest type *present* and errors rather than
wrap or auto-widen.** Note the result type is the widest *present*, not an invented
wider one: `Int32 * Int32` stays `Int32` and *errors* on a 40-bit product (matches
Rust's checked `i32 * i32`) — cast an operand (`Int64(a) * b`) for a wider result.

Why width and sign differ: widening loses no information (safe to promote), but
sign is an *interpretation* — `0xFFFFFFFF` is `-1` signed or `4294967295` unsigned
— so mixing signed and unsigned **errors** (safer than C's silent conversions).

## Implicit literal typing

Once any operand is fixed-width, **plain integer literals adopt** the resolved
(widest) width and its sign — `Int32(x) + 3`, not `Int32(x) + Int32(3)`. A literal
is **sign-neutral**: `Int32(x) + 3` and `UInt32(x) + 3` both work; a sign conflict
arises only between two explicitly-typed opposing-sign operands. A **fractional**
value in a fixed-width expression (`Int32(x) + 3.5`) **errors** — no silent
truncation; an already-typed `Number` mixed in **errors** (cast required).

## Operators on fixed-width values

- **Bitwise** (`bitAnd`/`bitOr`/`bitXor`, `~`/`bitNot`): in-range by construction;
  `~` flips within the width in two's-complement (signed → a valid signed value).
- **Shifts** (`<<`/`>>`): the shift *amount* is a count (exempt, plain integer); a
  left shift whose bits leave the width is an overflow → **error**.
- **`pow` / `^`-as-power**: the exponent is a **count** (plain, exempt); the result
  follows the base type and is range-checked (`Int32(2) ^ 40` → overflow).

## No float

`FloatN` (IEEE 754) is **declined**, not deferred-with-intent. Bounded integers
*preserve* exactness (range + checked overflow); IEEE floats *abandon* it
(`0.1 + 0.2 ≠ 0.3`) — the precise failure the language was built to avoid. Their
only use is *simulating* IEEE hardware behavior, which is niche.

## Implementation shape

Ecosystem-neutral (both implementations follow the same shape):

- A **fixed-int value** carries `value` (a big integer) × `bits` × `signed`, and is
  the payload of a dedicated `Value.fixedInt` case.
- The parameterized `Int(value, bits)` / `UInt(value, bits)` builtins plus the
  per-width `Int8…Int256` / `UInt8…UInt256` constructors are **generated from one
  width set** (a single list of allowed widths), so the width lineup lives in one
  place.
- Arithmetic is **intercepted before ordinary numeric coercion** (a single "is a
  fixed-int involved?" hook), leaving the plain number path untouched. Bitwise ops
  use **two's-complement over the width**.
- Persists like a record — the canonical, re-parseable form is the per-width
  spelling (`Int32(255)`), restored by re-evaluation.

For the concrete types and files, see the ecosystem docs:
[../swift/docs/ENGINE.md](../swift/docs/ENGINE.md) and
[../rust/docs/ANZAN.md](../rust/docs/ANZAN.md).

## Open questions for the build

1. **Dispatch** — let fixed-width types appear in typed-param annotations
   (`f(x: Int32) = …`)? Leaning yes; deferred.
2. **Radix display** — show hex / two's-complement following the mode / `CellFormat`?
   Deferred (currently always the canonical per-width form).
