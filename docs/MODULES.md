# Modules — namespaces, imports, and generic data types

> **Status: implemented.** Namespaces (`namespace Name { … }`, the `::`
> qualification token, nesting `A::B::c`, and namespaced data types, functions,
> and constants), `import` (unqualified access with loud conflicts, persisted
> per-workbook and replayed on open), builtins reachable as `Module::name`
> behind a curated global prelude, generic `data` fields (lists `[T]`, nested
> `[[T]]`, map-typed `{String: T}`), and the `Bits` module (`BitFormat` /
> `BitField`, including enum bit-fields decoded as a labeled picker) are all
> implemented and tested. Only the `at:` explicit-position field stays deferred
> (positions follow field order).
>
> This is the **companion spec to [ANZAN.md §9](ANZAN.md#9-namespaces-qualified-names-and-imports)** — that section is the summary;
> the design decisions and the per-phase history that shipped this feature are
> recorded in the sections below.

## Motivation

Give the binary editor's bit-field formats a **typed, reusable schema**
(`Bits::BitFormat`) instead of loose maps — but build the general capability
underneath, because it's useful everywhere: richer `data` records, a way to
group and name them, and a way to share library code.

## Decisions (settled)

| # | Decision |
|---|---|
| A | **User-declared namespaces** are supported (not just built-in modules). |
| B | `import` **persists per-workbook** and replays on open. |
| C | Qualification token is **`::`**; imports are `import NAME`. |
| D | Existing builtins are **reorganized into modules**, but a **global prelude** keeps the common ones usable bare (`pmt` works; `Finance::pmt` is an additive alias; `import` optional). |
| E | `data` fields gain **full generality**: lists `[T]`, nested lists `[[T]]`, and **map-typed** fields `{String: T}`. |
| F | A `BitField` carries an explicit **`kind`** field (`"numeric"`/`"flags"`/`"enum"`/`"reserved"`/`"unused"`). |

## 1 — Generic data field types  *(phase 1)*

The `data` field grammar becomes recursive:

```
fieldType  = "Number" | "String" | "Boolean" | TYPENAME
           | "[" fieldType "]"            (* list *)
           | "{" "String" ":" fieldType "}"  (* string-keyed map *)
```

```
data BitField  { name: String, bits: Number, at: Number, kind: String,
                 flags: [String], values: [String] }
data BitFormat { fields: [BitField] }
data Matrix    { rows: [[Number]] }                 # nested list
data Config    { opts: {String: Number} }           # map-typed field
```

- **Construction validates** structurally and recursively: a list value is an
  array whose every element matches the element type; a map value's every entry
  value matches the value type (keys are strings — Anzan map keys always are);
  records are checked by type. Empty `[]` / `{}` allowed. O(n) per container, no
  cycle risk (elements are already-validated immutable values).
- Records stay **immutable**, equality deep, `description` re-parseable
  (`BitFormat(fields: [BitField(…), …])`), and `toJson`/`fromJson` already handle
  arrays and objects — nesting falls out.
- This phase is **self-contained** and shippable without namespaces.

## 2 — Namespaces  *(`::`, phase 2)*

A new token **`::`** (not `.` — that's member/method access). A qualified name
resolves a member of a namespace: `Bits::BitFormat`, `Finance::pmt`.

- **User-declared** via a block:

  ```
  namespace Geometry {
      data Point { x: Number, y: Number };
      data Line  { a: Point, b: Point };
      midpoint(l: Line) = Point(x: (l.a.x + l.b.x) / 2, y: (l.a.y + l.b.y) / 2)
  }
  ```

  Members are **`;`-separated** (a function body would otherwise run into the
  next member via implicit multiplication; a trailing `;` is fine).

  Inside the block, names resolve locally first; outside, they're reached as
  `Geometry::Point`, `Geometry::midpoint`. Namespaces may be reopened (append to
  an existing one). Names within a namespace share its scope; the global flat
  namespace is just the unnamed top level.
- Namespaces can hold **data types, functions, and constants** (anything
  definable). Nesting of namespaces is allowed (`A::B::c`).
- Persistence: a user namespace's declarations persist as their source lines
  (like today's `data`/functions), grouped under the namespace.

## 3 — Imports  *(`import NAME`, phase 2)*

```
import Geometry
midpoint(seg)          # reachable unqualified after import
Geometry::midpoint(seg)  # always works, import or not
```

- `import M` brings module `M`'s exported names into the session unqualified.
- **Source:** built-in modules and **user-declared namespaces** (A). Cross-
  workbook imports (pulling from another `.soroban`) are **deferred** — they need
  file references / security-scoped bookmarks, a separate effort.
- **Persists per-workbook** (B): recorded and replayed on open, before the
  definitions that may rely on it. Restore order becomes
  **imports → namespaces/types → functions → variables**.
- **Conflicts are loud:** importing a name that collides with an existing global
  is an error, not silent shadowing.

## 4 — Builtins as modules + a global prelude  *(phase 3)*

The flat registry is reorganized into namespaces — `Finance`, `Stats`, `Math`,
`Programmer`, `Accounting`, `Dates`, `Logic`, `Data`, `Controls`, `Core` — so
every builtin is *also* reachable as `Module::name` (`Finance::pmt`,
`Stats::stdev`).

- A curated **global prelude** keeps today's names usable **bare** — `pmt(…)`,
  `sqrt(…)`, `sum(…)` all keep working with no import; the namespaced form is an
  additive alias. So existing log entries, cells, the CLI, and the whole gherkin
  suite stay green. (D — "no customers," but a calculator that needs an import to
  do basic math would be hostile, so the prelude stays.)
- This is a large but mechanical reorg (registry grouping + docs + the reference
  window's categories already mirror these groups, so it's mostly a rename of the
  grouping concept to a real namespace).

## 5 — The `Bits` module + binary editor  *(phases 4–5)*

The shipped schema carries an explicit `kind` ("numeric" / "flags" / "enum" /
"reserved" / "unused") plus
the two label lists (`at:` explicit positioning stays deferred — positions follow
field order):

```
namespace Bits {
    data BitField  { name: String, bits: Number, kind: String,
                     flags: [String], values: [String] };
    data BitFormat { fields: [BitField] }
}
```

Five field flavors, by `kind`: a **numeric** field of `bits` width; a **flags**
field whose `flags` name each bit high→low (`r-x`); an **enum** field whose
unsigned value indexes the `values` label list (value 1 of
`["idle","run","halt","max"]` → "run"); a **reserved** gap (locked,
must-be-zero — display only); and an **unused** gap (don't-care bits —
unlabeled but editable). When `kind` is absent (a hand-built record), the reader
derives numeric/flags/enum from which list is non-empty.

The binary editor's Save (`CalculatorSession.saveFormat`) emits the `Bits` schema
once per workbook (when `Bits::BitFormat` isn't yet defined), then the format as
a typed assignment:

```
namespace Bits { data BitField { name: String, bits: Number, kind: String, flags: [String], values: [String] }; data BitFormat { fields: [BitField] } }
perms = Bits::BitFormat(fields: [
    Bits::BitField(name: "owner", bits: 3, kind: "flags", flags: ["r", "w", "x"], values: []),
    Bits::BitField(name: "mode",  bits: 2, kind: "enum",  flags: [], values: ["idle", "run", "halt", "max"]) ])
```

Typed, persisted, fully manipulable (`perms.fields`, edit a `BitField`, re-run),
and `BitFormat` never pollutes the global namespace. The engine reads a
`BitFormat` record into its `[FieldSpec]` (structurally, by field name, in
`BinaryView.layout(from:)` alongside the loose-map form), so the host-neutral
`BinaryView` stays sheet/workbook-agnostic and `savedFormats`/`applyFormat`
handle records for free. The viewer (`BinaryEditorView`) renders an enum field
as a labeled `Picker`, a flags field as its decoded string, and a numeric field
as an editable value.

## Backward compatibility & persistence

- `::` is a new token; the **Normal-mode grammar is otherwise unchanged** — the
  regression oracle stays byte-identical for any module-free program, and the
  prelude keeps every existing program working.
- New workbook fields: **`imports`** (module names) and **`namespaces`**
  (name → declaration source lines), restored before functions/variables.

## Phasing (each lands green on its own)

1. ✅ **Generic data field types** — lists, nested lists, map fields. Parser,
   recursive validation, description/JSON/codec, `datatypes.feature`, `ANZAN.md §7`.
2. **Namespaces + imports**, split:
   - ✅ **2a-i — namespaced data types** — `::` token, `namespace` blocks
     (data members), qualified resolution + qualified type identity + sibling
     field-type qualification; `modules.feature`. *(No runtime context needed.)*
   - ✅ **2a-ii — namespaced functions** — `;`-separated members, the
     home-namespace resolver (siblings resolve unqualified at call time, via
     `EvaluationEnvironment.currentNamespace`, mirrored in `call(name:)` and
     `tailStep`), qualified parameter-type dispatch, recursion.
   - ✅ **2d — constants & nesting** — a namespace may hold CONSTANTS
     (`c = expr`, stored under `M::c`, evaluated eagerly in home context) and
     NESTED namespaces (`A::B::c`). Registration recurses (`registerNamespace`);
     sibling resolution walks UP the chain (`siblingCandidates`, the one source
     of truth for `.variable`/`call`/`tailStep`); type references qualify against
     an accumulated `typeScope` so a nested member can name a parent's type;
     `homeNamespace` is the prefix before the LAST `::`. Constants ride the
     namespace source-line replay (filtered out of the flat variable map;
     `clearNamespaceVariables` + a qualified-preserving `replaceUserVariables`
     keep restore sound).
   - ✅ **2b — imports** — `import NAME` brings a namespace's members into scope
     unqualified (a final-fallback resolution, so any builtin/user/host name
     wins); conflicts with a builtin/global/another import are a loud error at
     import time; re-import is idempotent.
   - ✅ **2c — persistence** — the workbook stores `namespaces` (declaration
     lines) and `imports`; qualified members are kept OUT of the flat
     `functions`/`dataTypes` maps. Restore order: namespaces → imports → types →
     functions → variables. Older files decode with empty defaults.
3. ✅ **Builtins → modules + prelude** — every builtin is reachable as
   `Module::name` (its category: `Finance::pmt`, `Stats::stdev`, `Core::sqrt`),
   validated against the builtin's category; the bare name stays global (the
   prelude — nothing renamed, all existing programs unchanged). `import` of a
   builtin module is a no-op (already in the prelude).
4. ✅ **The `Bits` module** — `BitFormat` / `BitField` with `kind`-tagged
   numeric / flags / enum fields (`{ name, bits, kind, flags, values }`). The
   engine reads such a record into `[FieldSpec]` via `BinaryView.layout(from:)`
   (`.record` case, choosing by `kind`), `BinaryViewTests`
   (`layoutParsesATypedBitFormatRecord`, `enumFieldDecodesItsValueToALabel`).
5. ✅ **Binary editor** — `CalculatorSession.saveFormat` emits the schema (once
   per workbook) + a typed `Bits::BitFormat(...)` assignment; `savedFormats` /
   `applyFormat` / `activeLayout` consume records for free through
   `layout(from:)`; the viewer renders enum fields as labeled pickers.

## Honest scope note

This was a multi-phase language platform, undertaken for a niche (programmer-mode)
feature but justified as general language growth. **Phase 1 was independently
valuable and the right place to start**; phases 2–4 were the heavy, foundational
ones. The binary editor (phase 5) was small once the platform existed.
