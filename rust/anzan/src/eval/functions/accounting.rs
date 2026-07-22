//! Port of the accounting function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Margin/markup conventions:
//!   markup is relative to COST, margin is relative to PRICE.

use crate::eval::currency::Currency;
use crate::eval::evaluator::require_int;
use crate::eval::fixed_decimal::{DecimalRounding, FixedDecimal, MAX_PRECISION};
use crate::eval::money::Money;
use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        // markup(cost, pct) → selling price after marking cost up by pct percent.
        BuiltinFunction {
            name: "markup",
            category: FunctionCategory::Accounting,
            signature: "markup(cost, pct)",
            summary: "Selling price after marking cost up by pct percent (markup is relative to cost).",
            examples: &["markup(80, 25)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(markup),
        },
        // margin(price, cost) → gross margin as a percent of price.
        BuiltinFunction {
            name: "margin",
            category: FunctionCategory::Accounting,
            signature: "margin(price, cost)",
            summary: "Gross margin as a percent of price (margin is relative to price).",
            examples: &["margin(100, 80)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(margin),
        },
        // percentOf(part, whole) → part as a percent of whole.
        BuiltinFunction {
            name: "percentOf",
            category: FunctionCategory::Accounting,
            signature: "percentOf(part, whole)",
            summary: "part as a percentage of whole.",
            examples: &["percentOf(30, 120)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(percent_of),
        },
        // percentChange(old, new) → relative change as a percent.
        BuiltinFunction {
            name: "percentChange",
            category: FunctionCategory::Accounting,
            signature: "percentChange(old, new)",
            summary: "Relative change from old to new, as a percent.",
            examples: &["percentChange(80, 100)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(percent_change),
        },
        BuiltinFunction {
            name: "Decimal",
            category: FunctionCategory::Accounting,
            signature: "Decimal(value[, [precision,] scale[, rounding]])",
            summary: "A fixed-precision decimal — SQL DECIMAL(p,s): at most `precision` significant digits, exactly `scale` fractional. Rounds to scale; exceeding the precision is an error (never silent). Decimal(10.5, 5, 2) → 10.50. Short forms: Decimal(value) captures the value exactly at max precision (1000), and Decimal(value, scale) pins the scale with precision defaulting to max. The optional last arg of the full form is the rounding mode: Rounding.Bankers (default) or Rounding.HalfUp.",
            examples: &[
                "Decimal(0.5)",
                "Decimal(0.5, 2)",
                "Decimal(10.5, 5, 2)",
                "Decimal(1.005, 5, 2, Rounding.HalfUp)",
            ],
            arity: 1..=4,
            implementation: Implementation::Values(make_fixed_decimal),
        },
        BuiltinFunction {
            name: "Money",
            category: FunctionCategory::Accounting,
            signature: "Money(value, code)",
            summary: "A currency amount — the mode-agnostic form of the finance-mode $10 literal. `code` is an ISO currency code string (case-insensitive): USD, EUR, GBP, JPY, CNY, INR, KRW, RUB, CHF, BTC. Renders grouped to 2 decimals with the currency symbol (Money(1234.5, \"USD\") → $1,234.50). The currency propagates through arithmetic; mixing two currencies is an error.",
            examples: &["Money(10, \"USD\")", "Money(1234.5, \"EUR\")"],
            arity: 2..=2,
            implementation: Implementation::Values(make_money),
        },
    ]
}

/// Builds a `Value::Money` for the `Money(value, code)` constructor — a number
/// and a currency code string. Unknown code → error.
fn make_money(arguments: &[Value]) -> Result<Value, EngineError> {
    let value = arguments[0].as_number("Money's value")?;
    let Value::String(code) = &arguments[1] else {
        return Err(EngineError::domain(
            "Money's 2nd argument is a currency code string — e.g. Money(10, \"USD\")",
        ));
    };
    let Some(currency) = Currency::from_code(code) else {
        let codes: Vec<&str> = Currency::ALL.iter().map(|c| c.code()).collect();
        return Err(EngineError::domain(format!(
            "unknown currency '{code}' — use one of {}",
            codes.join(", ")
        )));
    };
    Ok(Value::Money(Money::new(value, currency)))
}

// MARK: - Implementations

fn markup(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let pct = args[1].div(&BigDecimal::from_int(100))?;
    Ok(&args[0] * &(&BigDecimal::one() + &pct))
}

fn margin(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    if args[0].is_zero() {
        return Err(EngineError::domain("margin: price cannot be 0"));
    }
    Ok(&(&args[0] - &args[1]).div(&args[0])? * &BigDecimal::from_int(100))
}

fn percent_of(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(&args[0].div(&args[1])? * &BigDecimal::from_int(100))
}

fn percent_change(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    if args[0].is_zero() {
        return Err(EngineError::domain("percentChange: old value cannot be 0"));
    }
    Ok(&(&args[1] - &args[0]).div(&args[0])? * &BigDecimal::from_int(100))
}

/// Builds a `Value::FixedDecimal` for the `Decimal` constructor. The arity
/// drives the shape:
///   Decimal(value)                         — scale from the value, precision = max
///   Decimal(value, scale)                  — that scale, precision = max
///   Decimal(value, precision, scale)       — both declared
///   Decimal(value, precision, scale, mode) — + rounding mode
fn make_fixed_decimal(arguments: &[Value]) -> Result<Value, EngineError> {
    let value = arguments[0].as_number("Decimal's value")?;
    let precision: i64;
    let scale: i64;
    let mut rounding = DecimalRounding::Bankers;
    match arguments.len() {
        1 => {
            // Default: capture the value exactly (its own decimal places) with
            // the max precision — lossless, and roomy enough that ordinary
            // arithmetic won't overflow. The big precision is hidden when it
            // recalls.
            precision = MAX_PRECISION;
            scale = 0.max(-value.exponent());
        }
        2 => {
            scale = require_int(&arguments[1].as_number("Decimal scale")?, "Decimal scale")?;
            precision = MAX_PRECISION;
        }
        _ => {
            // 3 or 4
            precision = require_int(
                &arguments[1].as_number("Decimal precision")?,
                "Decimal precision",
            )?;
            scale = require_int(&arguments[2].as_number("Decimal scale")?, "Decimal scale")?;
            if arguments.len() == 4 {
                let Value::String(mode) = &arguments[3] else {
                    return Err(EngineError::domain(
                        "Decimal's 4th argument is the rounding mode — Rounding.Bankers or Rounding.HalfUp",
                    ));
                };
                match mode.to_lowercase().as_str() {
                    "bankers" => rounding = DecimalRounding::Bankers,
                    "halfup" => rounding = DecimalRounding::HalfUp,
                    _ => {
                        return Err(EngineError::domain(format!(
                            "unknown rounding '{mode}' — use Rounding.Bankers or Rounding.HalfUp"
                        )))
                    }
                }
            }
        }
    }
    Ok(Value::FixedDecimal(FixedDecimal::new(
        value, precision, scale, rounding,
    )?))
}
