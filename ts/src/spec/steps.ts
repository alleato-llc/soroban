// The parity harness steps: the language subset of the SHARED spec
// (../spec/anzan), the same feature files the Swift (PickleKit) and Rust
// (cucumber-rs, rust/engine/tests/gherkin.rs) suites run. This package has no
// hosting layer — no cells, sheets, workbooks, or formats — so only the
// pure-language features are wired up (cucumber.mjs lists them; the README
// records the split). One fresh Calculator per scenario.

import assert from "node:assert/strict";
import { Before, Given, Then, When } from "@cucumber/cucumber";
import { Calculator, type EvalOutcome, type Mode } from "../index.js";

interface AnzanWorld {
  calculator: Calculator;
  outcome: EvalOutcome | undefined;
}

Before(function (this: AnzanWorld) {
  this.calculator = new Calculator();
  this.outcome = undefined;
});

function success(world: AnzanWorld): Extract<EvalOutcome, { ok: true }> {
  const outcome = world.outcome;
  assert.ok(outcome, "expected a result, but nothing was calculated");
  assert.ok(outcome.ok, `expected a result, got error: ${outcome.ok ? "" : outcome.error}`);
  return outcome;
}

// Registration is keyword-agnostic in cucumber-js (Given/When/Then are
// interchangeable), so one definition serves every keyword the features use.
When(/^I calculate "(.*)"$/, function (this: AnzanWorld, expression: string) {
  this.outcome = this.calculator.evaluate(expression);
});

/** The engine path behind `.anzan` files and statement-aware pipes: the
 * docstring runs through the statement splitter + the calculator, halting at
 * the first error. `the result is` / `the log echoes` then assert the LAST
 * statement's outcome; a split error (unterminated block) or the first
 * failing statement becomes the outcome instead. */
When(/^I run the script:$/, function (this: AnzanWorld, source: string) {
  const run = this.calculator.runScript(source);
  this.outcome = run.results[run.results.length - 1];
});

Given(
  /^the calculator is in (normal|programmer|finance) mode$/,
  function (this: AnzanWorld, mode: string) {
    this.calculator.mode = mode as Mode;
  },
);

Then(/^the result is "(.*)"$/, function (this: AnzanWorld, expected: string) {
  // The CANONICAL description (what persists/recalls), not the echo.
  assert.equal(success(this).description, expected);
});

Then(/^the log echoes "(.*)"$/, function (this: AnzanWorld, expected: string) {
  // The human-facing echo (`display_description`) — how the log and CLI show
  // a result; differs from the canonical form for tagged types ($10.00 vs
  // Money(10, "USD")).
  assert.equal(success(this).displayDescription, expected);
});

/** Exact nearness: the canonical description is re-parseable, so the check
 * runs through the engine itself (a fresh normal-mode session) instead of
 * lossy float parsing — `abs((value) - (target)) <= bound`. */
function assertNear(world: AnzanWorld, bound: string, target: string): void {
  const outcome = success(world);
  assert.equal(outcome.kind, "value", `expected a numeric result, got ${outcome.kind}`);
  const checker = new Calculator();
  const check = checker.evaluate(`abs((${outcome.description}) - (${target})) <= ${bound}`);
  assert.ok(check.ok, `nearness check failed to evaluate: ${check.ok ? "" : check.error}`);
  assert.equal(
    check.description,
    "1",
    `${outcome.description} is not within ${bound} of ${target}`,
  );
}

Then(
  /^the result is within "(.*)" of "(.*)"$/,
  function (this: AnzanWorld, bound: string, target: string) {
    assertNear(this, bound, target);
  },
);

Then(/^the result is within "(.*)" of zero$/, function (this: AnzanWorld, bound: string) {
  assertNear(this, bound, "0");
});

Then(/^the calculation fails mentioning "(.*)"$/, function (this: AnzanWorld, fragment: string) {
  const outcome = this.outcome;
  assert.ok(outcome, "expected a failure, but nothing was calculated");
  assert.ok(!outcome.ok, `expected a failure, got: ${outcome.ok ? outcome.description : ""}`);
  assert.ok(
    outcome.error.includes(fragment),
    `error '${outcome.error}' doesn't mention '${fragment}'`,
  );
});

Then(/^documentation is shown mentioning "(.*)"$/, function (this: AnzanWorld, fragment: string) {
  const outcome = success(this);
  assert.equal(outcome.kind, "documentation", `expected documentation, got ${outcome.kind}`);
  // The description of a documentation outcome is signature + summary +
  // examples joined — the same text the native steps search.
  assert.ok(
    outcome.description.includes(fragment),
    `documentation doesn't mention '${fragment}': ${outcome.description}`,
  );
});
