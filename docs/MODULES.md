# Modules — generic data types, namespaces, and imports (design)

> **Status: IN PROGRESS.** Phase 1 (generic data field types), phase 2a-i
> (namespaced *data types*), and phase 2a-ii (namespaced *functions* with the
> home-namespace resolver — siblings resolve unqualified at call time, incl.
> typed-parameter dispatch and recursion), and phase 2b (`import` — unqualified
> access with loud conflicts) are **implemented and tested**.
> Constants-in-namespaces, the builtin module reorg, and persistence are still
> ahead. A foundational
> language initiative, specced before code (like `docs/MODES.md` /
> `docs/DECIMAL.md`). It is **the largest single direction proposed for the
> language**, and the design has deliberately taken the *expansive* option on
> every axis (decisions below). It is sequenced into independently shippable
> phases; phase 1 is the cheapest and most broadly useful. No customers exist
> yet, so reorganizing existing builtins is on the table.

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
| F | A `BitField` carries an explicit **`kind`** field (`"numeric"`/`"flags"`/`"enum"`). |

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

```
namespace Bits {
    data BitField  { name: String, bits: Number, at: Number, kind: String,
                     flags: [String], values: [String] }   # kind: numeric|flags|enum
    data BitFormat { fields: [BitField] }
}
```

The builder, on Save, emits to the log:

```
import Bits
perms = BitFormat(fields: [
    BitField(name: "owner", bits: 3, at: 6, kind: "flags", flags: ["r","w","x"], values: []),
    BitField(name: "mode",  bits: 2, at: 3, kind: "enum",  flags: [], values: ["idle","run","halt","max"]) ])
```

Typed, persisted, fully manipulable (`perms.fields`, edit a `BitField`, re-run),
and `BitFormat` never pollutes the global namespace. The engine reads a
`BitFormat` record into its `[FieldSpec]` (structurally, by field name), so the
host-neutral `BinaryView` stays sheet/workbook-agnostic.

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
     `tailStep`), qualified parameter-type dispatch, recursion. *(Constants and
     nesting still deferred.)*
   - ✅ **2b — imports** — `import NAME` brings a namespace's members into scope
     unqualified (a final-fallback resolution, so any builtin/user/host name
     wins); conflicts with a builtin/global/another import are a loud error at
     import time; re-import is idempotent. Session-scoped (persistence is 2c).
   - **2c — persistence** (workbook `namespaces`/`imports`, restore order).
3. **Builtins → modules + prelude** — registry reorg, `Module::name` aliases, prelude.
4. **The `Bits` module** — `BitFormat` / `BitField`.
5. **Binary editor** — builder emits/consumes `Bits::BitFormat`; viewer renders
   enum labels (the work paused for this design).

## Honest scope note

This is a multi-phase language platform, undertaken for a niche (programmer-mode)
feature but justified as general language growth. **Phase 1 is independently
valuable and the right place to start**; phases 2–4 are the heavy, foundational
ones and should be confirmed as you reach them rather than treated as a single
commitment. The binary editor (phase 5) is small once the platform exists.
