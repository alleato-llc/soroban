# Changelog — Swift / macOS app

Changes to the **Swift ecosystem**: the macOS app (`swift/App`), the engine
package (`swift/Engine` — Anzan + SorobanEngine), the `soroban` CLI, and Kit.
Cross-ecosystem changes (both Swift and Rust) live in the repo-root
[CHANGELOG.md](../CHANGELOG.md); the Rust ecosystem has its own
[rust/CHANGELOG.md](../rust/CHANGELOG.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This track is versioned `vX.Y.Z`, cut automatically by salpa on merge to `main`
that touches `swift/**` or `spec/**` (patch by default; `#minor` / `#major` in
the merge commit bumps that part — see [docs/RELEASING.md](../docs/RELEASING.md)).
The GitHub Release for each tag is the point of truth for the signed, notarized
`Soroban.dmg`.

## [Unreleased]

### Added

- **Anzan scripts.** The `soroban` CLI runs `.anzan` files
  (`soroban change.anzan` — halts at the first error with `at file:line`,
  exit 1; mixes with expression arguments in one session), pipes are
  statement-aware, and the REPL grows a `… ` continuation prompt — a statement
  ends at a newline unless a `( [ {` is still open, in which case lines join
  into one logical line (`Anzan/Script.swift`'s public
  `StatementAccumulator`, the SDK primitive). A `#!/usr/bin/env soroban`
  shebang is an ordinary comment, so `chmod +x` scripts run directly. Shared
  behavior — see the root [CHANGELOG.md](../CHANGELOG.md) and
  [docs/CLI.md](docs/CLI.md).

### Changed

- **Faster exact arithmetic: a custom bignum under `BigDecimal`.** The significand
  is now a purpose-built `Integer` (sign-magnitude, with an inline small-value case
  that skips heap allocation and ARC, over a base-2⁶⁴ limb magnitude with schoolbook
  multiply and Knuth division) instead of `attaswift/BigInt`. Results are
  bit-identical — the whole shared spec and a new differential fuzz oracle against
  `BigInt` stay green — but the cross-engine benchmark is markedly faster:
  reduction/∑ ~4.3×, finance (`pmt`) ~1.9×, and recursive integer functions ~1.5×
  (reduction and recursion now outrun the Rust engine), with no regressions on the
  wide-precision division paths. `attaswift/BigInt` remains only for the fixed-width
  / binary-editor types.
- **More exact-arithmetic speedups at the `BigDecimal` layer.** Building on the
  custom bignum, several hot paths shed avoidable work — the division-heavy `pmt`
  finance workload is **~8× faster** (the cross-engine bench) and arithmetic ~1.1×,
  with recursion/reduction/transcendental flat — all bit-identical (the whole
  shared spec and the differential oracle stay green): `normalize` skips its
  trailing-zero strip for odd significands and reuses each division's quotient;
  `digitCount` is derived from the significand's bit length (corrected exactly
  against cached powers of ten) instead of formatting it to a decimal string;
  operand alignment no longer multiplies the aligned operand by 10⁰ and
  subtraction drops a redundant normalize pass; the `Integer` magnitude limb loops
  run over unsafe buffers; and large operands get divide-and-conquer base-10
  conversion and Karatsuba multiplication.
- **Minimum OS is now macOS 15 / iOS 18** (was macOS 14 / iOS 17). The new
  exact-decimal significand uses the stdlib `UInt128`/`Int128` for its limb
  arithmetic, which require these versions.
- **One CSV door: *Open CSV*.** The separate *Import Data (CSV)…* command is
  gone; *File → Open CSV…* (⇧⌘O) is now the single way to bring a CSV in. It
  opens the file as an editable workbook (grid cells when it fits, else a data
  sheet) — a copy, so edits save into the `.soroban` file and the source `.csv`
  is never modified. Matches the Rust app's unified *Open CSV*.

## [1.4.13] — 2026-07-22

### Added

- **Finance-mode currency as a first-class type, plus thousands grouping.**
  Currency is a genuine tagged type (`Value.money`, `Anzan/Eval/Money.swift`) —
  a peer of `Int32(…)`/`Decimal(…)` — with a curated `Currency` enum
  (`Anzan/Eval/Currency.swift`: USD/EUR/GBP/JPY/CNY/INR/KRW/RUB/CHF/BTC) and a
  mode-agnostic constructor `Money(value, "USD")` whose call *is* the canonical
  form; the finance-mode literals `$10`/`€10` are sugar for it. An unsupported
  currency glyph is a loud lex error; CNY/CHF (no unambiguous glyph) are
  constructor-only. The currency propagates through arithmetic the way
  `FixedDecimal`'s type does — `$10 * 5%` is `$0.50`,
  `$10,000 + ($15,000 * 5%)` is `$10,750.00`, and it survives all four operators
  (`$10 * $2` is `$20.00` — a display contract, not a unit system). Mixing
  currencies errors; `%` on a currency errors (`$9%` is a category error;
  `$10 * 5%` still works). **Thousands grouping** (`138,561`) is now a *separate*,
  presentation-only value (`Value.grouped`, `Anzan/Eval/Grouped.swift`) with no
  arithmetic rules: it canonicalizes to the plain number but echoes through a
  calculation (`138,561 * 9%` shows `12,470.49`). The grouping helpers live on
  `BigDecimal` (`Number/BigDecimal+Format.swift`) so a formatted cell and a
  finance-mode result share one implementation. Both literal forms are
  Finance-only: `$A:1` and `max(138,561)` are unchanged. Shared behavior — see
  [docs/MODES.md](../docs/MODES.md) and the root [CHANGELOG.md](../CHANGELOG.md).

### Fixed

- **The shared-spec runner was passing vacuously.** `GherkinTests` loaded its
  features from the test bundle, but `Features` is a symlink to `spec/anzan` and
  SwiftPM copies the *link* — whose relative target no longer resolves once
  inside the bundle. The loader therefore found **zero** feature files and the
  suite reported success without executing a single scenario, which is
  indistinguishable from a real pass. It now resolves the spec directory from
  `#filePath`, and the Swift side runs all 559 shared scenarios again.

## [1.4.5] — 2026-06-17

### Changed

- Docs: `docs/PROGRAMMER.md`, `docs/MODULES.md`, `README.md`, and `CLAUDE.md`
  now cover the expanded preset catalog, all **five** field kinds (numeric /
  flags / enum / **reserved** / **unused**), the `base` field in the `Bits`
  schema, and the `formatValue` encoder.

## [1.4.4] — 2026-06-17

### Changed

- Internal only: real-world decode tests for the new bit-formats (IEEE 754,
  `st_mode`, EFLAGS, RGBA8888) — no user-facing change.

## [1.4.3] — 2026-06-17

### Added

- **14 new built-in bit-formats** across four groups: floating point (IEEE 754
  float/double/half, bfloat16), color (RGBA8888, ARGB1555, RGBA4444),
  networking (DNS header flags, VLAN 802.1Q tag, IPv4 DSCP/ECN), and systems
  (x86 EFLAGS, Unix `st_mode`, FAT date, FAT time). The richer ones mix
  enum, flag, and reserved sub-fields in one layout — backed by a new
  `BinaryView.formatValue([FieldSpec])` encoder that round-trips enum /
  reserved / unused fields (and duplicate field names like EFLAGS's repeated
  `reserved`/`flags`).

## [1.4.2] — 2026-06-16

### Changed

- A bit-field **format now fixes the register width** to its own size — IPv4 is
  32 bits, MAC 48, IPv6 128; applying one snaps the width (up or down) and the
  width selector locks to it. No more widening a format into meaningless unused
  high bits. (Applies to built-in and saved formats alike.)

## [1.4.1] — 2026-06-16

### Fixed

- Bit-editor builder: a single claim can now span **all** the free bits, not
  just 32 — so you can reserve e.g. 47 of 48 bits in one field instead of being
  left with a chunk still "available". (The open cells wrap.)

## [1.4.0] — 2026-06-16

### Fixed

- Bit editor: a wide **field band** — a Reserved/Unused gap, or any field
  wider than the card — now **wraps** into rows instead of overflowing the
  editor (matches the unused high band). Previously a wide reserved span ran
  off-screen.

### Added

- The **calculator** now manages its saved bit-formats in the editor too —
  **rename/delete** from the Format menu (previously only the standalone Tama
  app could; the calculator's were managed via the log). Backed by new
  off-log primitives `Calculator.setUserVariable` / `removeUserVariable`.

## [1.3.0] — 2026-06-16

### Added

- **Binary bit-editor parity** (shared `BinaryEditorKit`, so both the calculator
  and the standalone Tama app get it):
  - A **48-bit** register width (8/16/32/48/64/128/256) — a MAC fits exactly.
  - **Reserved** (locked, must-be-zero) and **Unused** (don't-care, editable)
    bit-field kinds in the builder, persisted in the typed `Bits::BitFormat`
    (`kind: "reserved"` / `"unused"`).
  - **Build new…** vs **Edit current…** — the builder no longer silently edits
    the active format; the Format menu is disabled mid-build.
  - Out-of-format ("unused") bits are grayed and locked until a deliberate
    double-click enables editing (one confirm); the unused high band wraps so a
    wide span (e.g. IPv6 in a 256-bit register) doesn't overflow.
  - Hosts that own their format store (Tama) gain in-editor **rename/delete** of
    saved formats (`BinaryEditorHost.canManageSavedFormats`).

## [1.2.0] — 2026-06-15

### Added

- **Language modes** — a default dialect and a **Programmer** dialect where the
  overloaded glyphs read as C operators (`^ & | << >> ~`, `%` as modulo); a
  display dialect only, the stored formula stays canonical.
- **Fixed-width integers** — `Int8…Int256` / `UInt8…UInt256` and parameterized
  `Int(v, bits)` / `UInt(v, bits)` (8–256 bits, signed/unsigned), exact and
  *checked*: overflow is an error, never a silent wraparound.
- **Fixed-precision decimals** — a SQL-style `Decimal(p, s)` money type that
  rounds to scale and errors on precision overflow.
- **Unix-style `man` pages** for built-ins, with every documented example
  evaluated by the test suite.
- **Programmer-mode binary bit-editor** — a clickable bit grid bound to the last
  result; flip bits to build a value, then **Use** it (or double-click the
  decimal/hex) to drop it into the expression. SpeedCrunch-style `ans`-prefix:
  a leading operator on an empty line continues the last result.
- **Bit-field formats** — label a register's bit ranges as **numeric**,
  per-bit **flags** (`r-x`), or **enum** fields. Built-in presets (Unix
  permissions, TCP flags, RGB565) plus networking presets (IPv4, IPv6, MAC) with
  per-field hex. Formats persist as typed `Bits::BitFormat` records, and a
  **visual builder** carves one by claiming bits, naming, and coloring them.
- **Module system** — namespaces (`Geo::Point`, `namespace` blocks, nested),
  `import` with loud conflict reporting, generic/typed `data` records (list,
  nested-list, and map fields), namespaced functions and types, constants, and
  built-ins reachable as `Module::name` behind the global prelude.
- **Site** — a nine-feature information architecture, a docs refresh, and the
  standard-library note.

### Changed

- Extracted the binary bit-editor UI into a shared **`BinaryEditorKit`** Swift
  package, so the calculator and the standalone [Tama](https://github.com/alleato-llc/soroban)
  app share one component instead of duplicating it.

### Removed

- The text-spec **"Custom…"** format entry in the bit editor — the visual
  builder is a strict superset (it also authors flag/enum fields, colors, and
  per-field bases), so the redundant `owner:3 group:3` text path was dropped.

## [1.1.9] — 2026-06-11

### Fixed

- Anzan: a number can no longer directly follow another value — this is now an
  error rather than a silent multiplication.

## [1.1.8] — 2026-06-11

### Fixed

- Restored the stable download asset after the infrastructure-scrub history
  reset.

## [1.1.7] — 2026-06-11

### Added

- First public release of Soroban — an exact-arithmetic spreadsheet calculator
  for macOS, built on a 50-digit exact decimal engine with the Anzan language.

[Unreleased]: https://github.com/alleato-llc/soroban/compare/v1.4.13...HEAD
[1.4.13]: https://github.com/alleato-llc/soroban/compare/v1.4.12...v1.4.13
[1.4.5]: https://github.com/alleato-llc/soroban/compare/v1.4.4...v1.4.5
[1.4.4]: https://github.com/alleato-llc/soroban/compare/v1.4.3...v1.4.4
[1.4.3]: https://github.com/alleato-llc/soroban/compare/v1.4.2...v1.4.3
[1.4.2]: https://github.com/alleato-llc/soroban/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/alleato-llc/soroban/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/alleato-llc/soroban/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/alleato-llc/soroban/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/alleato-llc/soroban/compare/v1.1.9...v1.2.0
[1.1.9]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.9
[1.1.8]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.8
[1.1.7]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.7
