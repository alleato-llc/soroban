//! Port of the controls function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Control expressions — functions whose CALLS double as interactive grid
//! controls in the app. Evaluation is ordinary and pure, so workbooks behave
//! identically headlessly: `slider(v, lo, hi)` is just `v` clamped into
//! range.

use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        BuiltinFunction {
            name: "slider",
            category: FunctionCategory::Controls,
            signature: "slider(value, min, max, step?)",
            summary: "A what-if slider. In a grid cell — ideally a definition like rate = slider(0.08, 0, 0.2) — it renders as a draggable control; dragging rewrites the value in place and recalculates everything that reads it. Evaluates to the value, clamped into min…max. Step defaults to (max−min)/100.",
            examples: &["slider(5, 0, 10)", "slider(15, 0, 10)", "slider(0.5, 0, 1, 0.25)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(slider_impl),
        },
        BuiltinFunction {
            name: "stepper",
            category: FunctionCategory::Controls,
            signature: "stepper(value, min, max, step?)",
            summary: "A discrete what-if control: − and + buttons move the value by step (default 1), clamped into min…max. n = stepper(5, 1, 20) in a cell renders the control; formulas read n.",
            examples: &["stepper(5, 1, 20)", "stepper(2.5, 0, 10, 2.5)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(stepper_impl),
        },
        BuiltinFunction {
            name: "checkbox",
            category: FunctionCategory::Controls,
            signature: "checkbox(state)",
            summary: "A toggle: flag = checkbox(true) renders as a checkbox; clicking flips it in place. Evaluates to 1 or 0 (the engine's truth values), so if(flag, …, …) and sum(…) over checkbox ranges both work.",
            examples: &["checkbox(true)", "checkbox(false)", "if(checkbox(true), 10, 20)"],
            arity: 1..=1,
            implementation: Implementation::Numeric(checkbox_impl),
        },
        BuiltinFunction {
            name: "dropdown",
            category: FunctionCategory::Controls,
            signature: "dropdown(value, [options])",
            summary: "A picker: region = dropdown(\"EU\", [\"EU\", \"US\", \"APAC\"]) renders as a menu; choosing rewrites the value in place. Evaluates to the selected value — strings compare with ==, numeric options behave as numbers.",
            examples: &["dropdown(\"EU\", [\"EU\", \"US\", \"APAC\"])", "dropdown(5, [1, 5, 10])"],
            arity: 2..=2,
            implementation: Implementation::Values(dropdown_impl),
        },
    ]
}

/// Shared slider/stepper body: validate min < max and a positive optional
/// step, then clamp the value into min…max.
fn clamped_control(args: &[BigDecimal], name: &str) -> Result<BigDecimal, EngineError> {
    let (minimum, maximum) = (&args[1], &args[2]);
    if minimum >= maximum {
        return Err(EngineError::domain(format!("{name}() needs min < max")));
    }
    if args.len() == 4 && args[3] <= BigDecimal::zero() {
        return Err(EngineError::domain(format!(
            "{name}() step must be positive"
        )));
    }
    let value = &args[0];
    Ok(if value < minimum {
        minimum.clone()
    } else if value > maximum {
        maximum.clone()
    } else {
        value.clone()
    })
}

fn slider_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    clamped_control(args, "slider")
}

fn stepper_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    clamped_control(args, "stepper")
}

fn checkbox_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args[0].is_zero() {
        BigDecimal::zero()
    } else {
        BigDecimal::one()
    })
}

fn dropdown_impl(args: &[Value]) -> Result<Value, EngineError> {
    if !matches!(args[1], Value::Array(_)) {
        return Err(EngineError::domain(format!(
            "dropdown() wants (value, [options]) — got {} second",
            args[1].kind_name()
        )));
    }
    Ok(args[0].clone())
}
