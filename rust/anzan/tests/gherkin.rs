//! The parity harness: runs every scenario in `spec/anzan/*.feature` — the
//! SAME feature files the Swift implementation runs through PickleKit. One
//! fresh world per scenario (cucumber's default), mirroring SorobanSteps'
//! reset-in-init pattern.
//!
//! Only the LOG steps live here — grid / worksheet / formatting / persistence
//! steps belong to the engine crate (Phase 2) and their scenarios skip until
//! it exists. Patterns are greedy `(.*)` regexes exactly like PickleKit's, so
//! arguments may embed quotes (`the result is "Person(name: "Ada", …)"`).

use anzan::{Calculator, EngineError, EvalOutcome, LanguageMode};
use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
pub struct AnzanWorld {
    calculator: Calculator,
    outcome: Option<Result<EvalOutcome, EngineError>>,
}

// `I calculate` appears under Given, When, and And in the features; PickleKit
// matches on the pattern alone, so register it for both keyword kinds.
#[given(regex = r#"^I calculate "(.*)"$"#)]
#[when(regex = r#"^I calculate "(.*)"$"#)]
fn calculate(world: &mut AnzanWorld, expression: String) {
    world.outcome = Some(world.calculator.evaluate(&expression));
}

#[given(regex = r"^the calculator is in (normal|programmer|finance) mode$")]
fn set_mode(world: &mut AnzanWorld, mode: String) {
    world.calculator.mode = LanguageMode::from_name(&mode).expect("gated by the regex");
}

#[then(regex = r#"^the result is "(.*)"$"#)]
fn result_is(world: &mut AnzanWorld, expected: String) {
    match &world.outcome {
        Some(Ok(outcome)) => {
            let shown = outcome.to_string();
            assert_eq!(shown, expected, "expected {expected}, got {shown}");
        }
        other => panic!("expected a result, got {other:?}"),
    }
}

#[then(regex = r#"^the result is within "(.*)" of "(.*)"$"#)]
fn result_near_target(world: &mut AnzanWorld, bound: String, target: String) {
    near(world, &bound, &target);
}

#[then(regex = r#"^the result is within "(.*)" of zero$"#)]
fn result_near_zero(world: &mut AnzanWorld, bound: String) {
    near(world, &bound, "0");
}

fn near(world: &mut AnzanWorld, bound: &str, target: &str) {
    // Tolerance comparison in BigDecimal once the number core lands; the
    // f64 placeholder keeps the harness honest for now (tolerances in the
    // features are all well inside f64 range).
    let value = match &world.outcome {
        Some(Ok(outcome)) => outcome
            .numeric_value()
            .unwrap_or_else(|| panic!("expected a numeric result, got {outcome}")),
        other => panic!("expected a numeric result, got {other:?}"),
    };
    let bound: f64 = bound.parse().expect("a numeric bound");
    let target: f64 = target.parse().expect("a numeric target");
    assert!(
        (value - target).abs() <= bound,
        "{value} is not within {bound} of {target}"
    );
}

#[then(regex = r#"^the calculation fails mentioning "(.*)"$"#)]
fn calculation_fails(world: &mut AnzanWorld, fragment: String) {
    match &world.outcome {
        Some(Err(error)) => {
            let text = error.to_string();
            assert!(
                text.contains(&fragment),
                "error '{text}' doesn't mention '{fragment}'"
            );
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[then(regex = r#"^documentation is shown mentioning "(.*)"$"#)]
fn documentation_shown(world: &mut AnzanWorld, fragment: String) {
    match &world.outcome {
        Some(Ok(outcome)) => {
            // Grows a real `.documentation` arm with the FunctionDoc port.
            let text = outcome.to_string();
            assert!(
                text.contains(&fragment),
                "documentation doesn't mention '{fragment}': {text}"
            );
        }
        other => panic!("expected documentation, got {other:?}"),
    }
}

#[tokio::main]
async fn main() {
    AnzanWorld::cucumber()
        .max_concurrent_scenarios(1) // serialized, like the Swift suite
        .run_and_exit("../../spec/anzan")
        .await;
}
