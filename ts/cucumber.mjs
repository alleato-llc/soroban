// The shared-spec runner config (`npm run spec`): cucumber-js over the SHARED
// ../spec/anzan feature files — the same files the Swift (PickleKit) and Rust
// (cucumber-rs) suites run; never a copy.
//
// This package wraps the LANGUAGE only — there is no hosting layer — so the
// paths list is the pure-language subset. The excluded features need host
// steps this package cannot honor (cells, sheets, workbooks, cell formats):
//   anzan.feature        — "cell A:1 contains" (pinned cell references)
//   datatypes.feature    — cell-scoped data declarations
//   formatting.feature   — cell formats ("is formatted as"/"displays")
//   library.feature      — "the sheet contains:" (sheet-fed statistics)
//   modules.feature      — "the workbook is saved and reopened"
//   reflection.feature   — sheets, cells, and workbook reflection throughout
//   spreadsheet.feature  — the grid itself
// The full table lives in ts/README.md.
//
// One further scenario is wasm-excluded (by name, not by file):
// functions.feature "Deep recursion is bounded by memory, not a counter".
// The native engines grow the stack for deep non-tail recursion (stacker /
// continueOnFreshStack); wasm has no growable stack, so the engine's wasm
// build answers with its clean depth-cap error instead ("nested too deeply"
// — pinned by src/index.test.ts). The negative-lookahead name filter keeps
// every other scenario running.
export default {
  name: ["^(?!Deep recursion is bounded by memory, not a counter$)"],
  paths: [
    "../spec/anzan/calculation.feature",
    "../spec/anzan/decimal.feature",
    "../spec/anzan/fixedwidth.feature",
    "../spec/anzan/functions.feature",
    "../spec/anzan/mathematics.feature",
    "../spec/anzan/modes.feature",
    "../spec/anzan/scripting.feature",
    "../spec/anzan/structures.feature",
  ],
  import: ["src/spec/steps.ts"],
};
