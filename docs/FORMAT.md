# The `.soroban` Workbook Format

A `.soroban` document is a **package** (a directory macOS shows as one file):

```
MyModel.soroban/
├── workbook.json    ← the manifest below: pretty-printed, sorted-key JSON —
│                      diffable, hand-editable, scriptable
└── data.sqlite      ← present ONLY when the workbook has data sheets
                       (its presence is the signal; pure models have no DB)
```

Legacy single-file `.soroban` JSON documents still open (they're treated as
a package with no database); saves always write the package shape. The codec
lives in the engine (`Workbook.swift`, `WorkbookPackage.swift`).

**Data sheets** (`"kind": "data"` below) hold imported records — CSV imports
land in `data.sqlite` (tables `tables` + sparse `cells(t, r, c, v)`), read
lazily so size doesn't affect open time. Import is a copy: edits in the grid
write back to this database (never the source file), within the table's
fixed shape. Data sheets may exceed the grid's 1,000-row bound
(`Sales!C:50000` is valid), and their values follow grid semantics in
formulas: empty → 0, text → error for direct references, text/empty skipped
in ranges. Cells hold values only — formulas live on calculation sheets.

```json
{
  "format" : "soroban-workbook",
  "version" : 2,
  "activeSheet" : "Loan",
  "sheets" : [
    {
      "name" : "Loan",
      "cells" : { "A:1" : "Loan amount", "B:1" : "350000" },
      "columnWidths" : { "A" : 140 },
      "rowHeights" : { "3" : 36 }
    },
    {
      "name" : "What If",
      "cells" : { "B:1" : "Loan!B:1 * 1.1" }
    }
  ],
  "variables" : { "rate" : "0.0825" },
  "functions" : { "tax" : "tax(x) = x * 1.0825 # TX sales tax" },
  "dataTypes" : { "Person" : "data Person { name: String, age: Number } # a teammate" }
}
```

## Fields

| Field | Type | Meaning |
|---|---|---|
| `format` | string | Always `"soroban-workbook"`. Anything else is rejected as not-a-workbook. |
| `version` | int | Currently `2` (v2 introduced the multi-sheet `sheets` array; v1 was a single-sheet file with top-level `cells` — still readable, see below). Files with a *higher* version than the app understands are rejected with a clear message; fields added later decode with defaults, so older files keep opening. |
| `sheets` | array | Ordered worksheets (max 256). Each entry: `name` (≤128 chars, unique case-insensitively, no `!` or `'`), `cells`, optional `kind`/`table`, and optional `columnWidths`/`rowHeights`. Invalid entries are skipped on open. |
| `sheets[].kind` | string | `"data"` marks a sheet backed by a `data.sqlite` table; absent for normal grid sheets. *(optional)* |
| `sheets[].table` | string | The `data.sqlite` table backing a data sheet. A data sheet whose table is missing is skipped on open. *(optional)* |
| `sheets[].cells` | object | Cell address → **raw contents exactly as typed**, including explicit markers (`=…` forced formula, `"…"` forced text) and cross-sheet references (`Loan!B:1`). Keys are `"A:1"` form: single column letter A–Z, colon, 1-based row 1–1000. Unknown/out-of-range keys are skipped on open. |
| `activeSheet` | string | Which sheet was showing when saved. *(optional)* |
| `variables` | object | Variable name → value as a string: a decimal (`BigDecimal` round-trips exactly, scientific notation allowed: `"1e+40"`), a canonical structure literal (`"[1, 2, 3]"`, `"{name: \"Ada\"}"`, `"\"text\""`), a record's canonical constructor call (`"Person(name: \"Ada\", age: 36)"` — restored by evaluation after `dataTypes`, so the type must be in the same file), or a fixed-width integer's / fixed-precision decimal's constructor call (`"Int32(255)"`, `"UInt8(8)"`, `"Decimal(10.50, 5, 2)"`, or a short-form `"Decimal(0.5)"` / `"Decimal(0.50, 2)"` when the precision is the max default — also restored by evaluation; the `Int`/`UInt`/`Decimal` builtins are always present, so no ordering dependency). Unparseable values are dropped. |
| `functions` | object | Function name → its **original definition line**, including any trailing `# doc comment` (that comment is the function's documentation). On open each line is re-evaluated; lines that no longer parse are dropped. |
| `dataTypes` | object | Data type name → its **original declaration line** (`"data Person { … } # comment"`), same source-line contract as `functions`. Restored FIRST on open — before `functions` and `variables` — so record variables can reconstruct. Decodes to empty for older files. *(optional)* |
| `sheets[].columnWidths` | object | Column name (`"A"`) → width in points. Sparse: only non-default sizes appear. Clamped to 40–400 on open. *(optional)* |
| `sheets[].rowHeights` | object | 1-based row number (`"5"`) → height in points. Clamped to 18–120. *(optional)* |
| `sheets[].names` | object | Cell address → its name (`{"B:7": "Projected Rate"}`) — referenced in formulas as `'Projected Rate'` / `Sheet!'Projected Rate'`. ≤64 chars, unique per sheet case-insensitively, no `'` or `!`. *(optional)* |
| `sheets[].formats` | object | Cell address → presentation, sparse (only formatted cells, only non-default fields): booleans `bold`/`italic`/`underline`/`strikethrough`, `alignment` (`left`/`center`/`right`; absent = automatic), `textColor`/`fillColor` (semantic names: `red orange yellow green blue purple gray`), and a number format as `style` (`number`/`currency`/`percent`/`date`/`hex`/`binary`) + `decimals` + `symbol` (currency only — stored so the workbook renders identically across locales). Unknown styles degrade to general. Display-only: the cell's raw contents and computed value are unaffected. *(optional)* |

**Legacy single-sheet files** (top-level `cells`/`columnWidths`/`rowHeights`
instead of `sheets`) still open: they become one sheet named "Sheet 1".
Saves always write the `sheets` form.

## Semantics worth knowing

- **Control cells need no special fields.** A slider/stepper/checkbox/dropdown
  is just a cell whose raw text is a control expression
  (`rate = slider(0.08, 0, 0.2)`); interacting with it rewrites that text.
  The same goes for sheet-scoped definitions (`tax(x) = x * 2`,
  `data Pt { x: Number, y: Number }`) and note cells (`# a comment`): the
  cells array carries everything — a note cell is just a cell whose raw text
  is a `#…` comment.

- **Cells store text, not values.** Evaluation happens on open against the
  workbook's own variables and functions, so a file is self-contained:
  formulas referencing `rate` or `tax(…)` work immediately.
- **Order doesn't matter.** Functions are late-bound (a function may call one
  defined "later"); cells recalculate as a whole.
- **Numbers are exact.** Variable values round-trip through arbitrary-
  precision decimal — no float drift through save/load cycles.
- **The scratch file is the same format, plus a journal.** Untitled work
  auto-saves to `…/Application Support/Soroban/sheet.json` (a full workbook
  snapshot) with cell edits appended live to `scratch-journal.jsonl`
  (one JSON object per line: `{"sheet": "…", "cell": "A:1", "raw": "…"}`).
  On load the journal replays over the snapshot; compaction rewrites the
  snapshot and empties the journal. `.soroban` files you save never involve
  the journal.

## Versioning policy

Additive fields decode with defaults (no version bump needed). Breaking
changes bump `version`; the app refuses files from its future with a message
telling the user to update, and continues to read all past versions.
