//! Indexing/subscripting and the binary/comparison operator application
//! (including the fixed-width integer and fixed-precision decimal hooks).

use super::{require_int, Evaluator};
use crate::ast::{BinaryOperator, ComparisonOperator};
use crate::eval::environment::EvaluationEnvironment;
use crate::eval::fixed_decimal::FixedDecimal;
use crate::eval::fixed_int::FixedInt;
use crate::eval::value::Value;
use crate::EngineError;
use std::rc::Rc;

impl Evaluator<'_> {
    /// `arr[0]` (0-based), `"abc"[0]`, `m["key"]`.
    pub(super) fn subscript_value(
        &self,
        environment: &mut EvaluationEnvironment,
        base: &Value,
        index: &Value,
    ) -> Result<Value, EngineError> {
        match base {
            Value::Array(items) => {
                let position = require_int(&index.as_number("an array index")?, "array index")?;
                let count = items.len();
                if position < 0 || position as usize >= count {
                    return Err(EngineError::domain(format!(
                        "index {position} is out of range (array has {count} element{})",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                Ok(items[position as usize].clone())
            }

            Value::String(text) => {
                let position = require_int(&index.as_number("a string index")?, "string index")?;
                let count = text.chars().count();
                if position < 0 || position as usize >= count {
                    return Err(EngineError::domain(format!(
                        "index {position} is out of range (string has {count} character{})",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                let ch = text.chars().nth(position as usize).expect("bounds checked");
                Ok(Value::String(ch.to_string()))
            }

            Value::Map(_) | Value::Record(_) => {
                let Value::String(key) = index else {
                    return Err(EngineError::domain(format!(
                        "map keys are strings — e.g. m[\"name\"], got {}",
                        index.kind_name()
                    )));
                };
                if let Some(value) = base.map_value(key) {
                    return Ok(value.clone());
                }
                if let Value::Record(record) = base {
                    return Err(EngineError::domain(format!(
                        "{} has no field '{key}' — it has {}",
                        record.type_name,
                        record
                            .entries
                            .iter()
                            .map(|e| e.key.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
                Err(EngineError::domain(format!("no key '{key}' in map")))
            }

            // Host handles define their own indexing (Worksheets[0] /
            // ["Budget"]).
            Value::Host(object) => {
                let object = Rc::clone(object);
                object.index((self, environment), index).ok_or_else(|| {
                    EngineError::domain(format!(
                        "{} can't be indexed by {}",
                        object.type_name(),
                        index.kind_name()
                    ))
                })
            }

            Value::Number(_) | Value::FixedInt(_) | Value::FixedDecimal(_) | Value::Function(_) => {
                Err(EngineError::domain(format!(
                    "{} can't be indexed",
                    base.kind_name()
                )))
            }
        }
    }

    pub(super) fn apply_op(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        // `+` concatenates as soon as either side is a string — "Q" + 1 is
        // "Q1".
        if op == BinaryOperator::Add
            && (matches!(lhs, Value::String(_)) || matches!(rhs, Value::String(_)))
        {
            return Ok(Value::String(format!(
                "{}{}",
                lhs.display_text(),
                rhs.display_text()
            )));
        }
        // Fixed-width integer arithmetic: the mixing matrix + checked
        // overflow (docs/FIXED-WIDTH.md). Numeric (non-FixedInt) operands
        // skip this and take the exact-decimal path below, unchanged.
        if FixedInt::is_involved(lhs, rhs) {
            return FixedInt::apply_binary(op, lhs, rhs);
        }
        // Fixed-precision decimal arithmetic — the money-type mixing matrix.
        if FixedDecimal::is_involved(lhs, rhs) {
            return FixedDecimal::apply_binary(op, lhs, rhs);
        }
        let a = lhs.as_number(op.symbol())?;
        let b = rhs.as_number(op.symbol())?;
        Ok(Value::Number(match op {
            BinaryOperator::Add => &a + &b,
            BinaryOperator::Subtract => &a - &b,
            BinaryOperator::Multiply => &a * &b,
            BinaryOperator::Divide => a.div(&b)?,
            BinaryOperator::Modulo => a.rem(&b)?,
            BinaryOperator::Power => crate::eval::numeric::pow(&a, &b)?,
        }))
    }

    /// `==`/`!=` are deep equality on any values; ordering needs numbers.
    pub(super) fn compare(
        op: ComparisonOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        match op {
            ComparisonOperator::Equal => Ok(Value::bool(lhs == rhs)),
            ComparisonOperator::NotEqual => Ok(Value::bool(lhs != rhs)),
            _ => {
                let a = lhs.as_number(op.symbol())?;
                let b = rhs.as_number(op.symbol())?;
                Ok(Value::bool(match op {
                    ComparisonOperator::Less => a < b,
                    ComparisonOperator::Greater => a > b,
                    ComparisonOperator::LessOrEqual => a <= b,
                    ComparisonOperator::GreaterOrEqual => a >= b,
                    ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
                        unreachable!("handled above")
                    }
                }))
            }
        }
    }
}
