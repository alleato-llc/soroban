# Changelog

All notable changes to Soroban are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Versions are tagged `vX.Y.Z` and cut automatically by salpa on merge to `main`
(patch by default; `#minor` / `#major` in the merge commit bumps that part — see
[docs/RELEASING.md](docs/RELEASING.md)). The GitHub Release for each tag is the
point of truth for downloads.

## [Unreleased]

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

[Unreleased]: https://github.com/alleato-llc/soroban/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/alleato-llc/soroban/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/alleato-llc/soroban/compare/v1.1.9...v1.2.0
[1.1.9]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.9
[1.1.8]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.8
[1.1.7]: https://github.com/alleato-llc/soroban/releases/tag/v1.1.7
