//! Thousands-grouped plain numbers (`138,561`) — the propagation rules for
//! `Value::Grouped`. Unlike `Money`, grouping is **pure presentation**:
//! `138,561` IS `138561`, with no currency, no unit, no arithmetic rules. It
//! carries only so the grouping ECHOES through a calculation (`138,561 * 9%` →
//! `12,470.49`). See docs/MODES.md.
//!
//! The tag therefore yields to anything with real meaning: a currency operand
//! absorbs it (Money dispatches first), and `^`/modulo drop it. It survives the
//! four ordinary operators, negation, and percent so the echo stays consistent.

use super::value::Value;
use crate::ast::BinaryOperator;
use crate::{BigDecimal, EngineError};

pub struct Grouped;

impl Grouped {
    /// True when either operand is a grouped number — checked AFTER the money /
    /// fixed-width hooks, so a "real" type always wins the dispatch.
    pub fn is_involved(lhs: &Value, rhs: &Value) -> bool {
        matches!(lhs, Value::Grouped(_)) || matches!(rhs, Value::Grouped(_))
    }

    /// `+ − × ÷` keep the grouping (the result is grouped); `^` and modulo drop
    /// it (the value survives as a plain number).
    pub fn apply_binary(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        let a = Self::operand(lhs)?;
        let b = Self::operand(rhs)?;
        match op {
            BinaryOperator::Add => Ok(Value::Grouped(&a + &b)),
            BinaryOperator::Subtract => Ok(Value::Grouped(&a - &b)),
            BinaryOperator::Multiply => Ok(Value::Grouped(&a * &b)),
            // BigDecimal division errors on a zero divisor.
            BinaryOperator::Divide => Ok(Value::Grouped(a.div(&b)?)),
            BinaryOperator::Power => Ok(Value::Number(crate::eval::numeric::pow(&a, &b)?)),
            BinaryOperator::Modulo => Ok(Value::Number(a.rem(&b)?)),
        }
    }

    /// A grouped or plain number as its exact value; anything else errors (but a
    /// typed operand never reaches here — it dispatches first).
    fn operand(value: &Value) -> Result<BigDecimal, EngineError> {
        match value {
            Value::Grouped(n) => Ok(n.clone()),
            Value::Number(n) => Ok(n.clone()),
            _ => Err(EngineError::domain(format!(
                "can't combine {} with a grouped number",
                value.kind_name()
            ))),
        }
    }
}
