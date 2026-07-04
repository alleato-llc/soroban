//! The recursion canary — the port of the Swift suite's SIGBUS guard. The
//! Gherkin features prove countdown(2000) and a 500k tail loop; this canary
//! drives NON-tail recursion far past a single stack segment so a regression
//! in the `stacker::maybe_grow` seam (Swift's `continueOnFreshStack`) shows
//! up as a crash here, not in a user's session.

use anzan::Calculator;

#[test]
fn deep_non_tail_recursion_completes_on_segmented_stacks() {
    // `+ 1` AFTER the recursive call keeps this honestly non-tail: every
    // level holds a live frame. 9,000 levels is well past one stack segment
    // in a debug build while staying under MAX_CALL_DEPTH (10,000) — the
    // runaway sanity cap shared with Swift, which bounds even honest
    // recursion.
    let mut calculator = Calculator::new();
    calculator
        .evaluate("countdown(n) = if(n <= 0, 0, countdown(n - 1) + 1)")
        .expect("defines");
    let outcome = calculator
        .evaluate("countdown(9000)")
        .expect("deep recursion must complete, not crash");
    assert_eq!(outcome.to_string(), "9000");
}

#[test]
fn runaway_recursion_errors_with_the_base_case_hint() {
    // Both caps (call depth, tail iterations) share one message contract:
    // it names the symptom AND hints at the missing base case.
    let mut calculator = Calculator::new();
    calculator.evaluate("f(x) = f(x)").expect("defines");
    let error = calculator
        .evaluate("f(1)")
        .expect_err("runaway recursion must be cut off")
        .to_string();
    assert!(error.contains("nested too deeply"), "{error}");
    assert!(error.contains("base case"), "{error}");
}
