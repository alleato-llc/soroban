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
fn runaway_tail_recursion_hits_the_tail_iteration_cap() {
    // `f(x) = f(x)` is a TAIL call — it loops at constant stack, so the
    // MAX_TAIL_ITERATIONS guard (not the depth cap) is what cuts it off.
    // Both caps share one message contract: it names the symptom AND hints
    // at the missing base case.
    let mut calculator = Calculator::new();
    calculator.evaluate("f(x) = f(x)").expect("defines");
    let error = calculator
        .evaluate("f(1)")
        .expect_err("runaway recursion must be cut off")
        .to_string();
    assert!(error.contains("nested too deeply"), "{error}");
    assert!(error.contains("base case"), "{error}");
}

#[test]
fn runaway_non_tail_recursion_hits_the_call_depth_cap() {
    // `+ 1` after the call keeps this non-tail, so it stacks — and with no
    // base case it trips MAX_CALL_DEPTH (the other arm of the same guard)
    // rather than looping forever. Same message contract.
    let mut calculator = Calculator::new();
    calculator.evaluate("g(x) = g(x) + 1").expect("defines");
    let error = calculator
        .evaluate("g(1)")
        .expect_err("non-tail runaway must be cut off")
        .to_string();
    assert!(error.contains("nested too deeply"), "{error}");
    assert!(error.contains("base case"), "{error}");
    assert!(
        error.contains("g()"),
        "names the offending function: {error}"
    );
}

#[test]
fn tail_recursion_loops_at_constant_stack() {
    // An accumulator whose recursive call is the WHOLE taken branch is a
    // tail call: `apply_user` turns it into a loop iteration (TailStep::Call)
    // at constant stack. 500k iterations would blow any real stack without
    // TCO — and it stays under MAX_TAIL_ITERATIONS (1,000,000).
    let mut calculator = Calculator::new();
    calculator
        .evaluate("tally(n, acc) = if(n <= 0, acc, tally(n - 1, acc + 1))")
        .expect("defines");
    let outcome = calculator
        .evaluate("tally(500000, 0)")
        .expect("tail loop must run to completion");
    assert_eq!(outcome.to_string(), "500000");
}

#[test]
fn tail_recursion_walks_through_nested_conditionals() {
    // `tail_step` descends the taken branch of if() to find the tail call —
    // even through a nested conditional. The recursive call sits two if()
    // levels deep, so this only completes if the walker recurses into both.
    let mut calculator = Calculator::new();
    calculator
        .evaluate("walk(n) = if(n <= 0, 0, if(n > 1000000, 0, walk(n - 1)))")
        .expect("defines");
    let outcome = calculator
        .evaluate("walk(200000)")
        .expect("nested-conditional tail recursion completes");
    assert_eq!(outcome.to_string(), "0");
}
