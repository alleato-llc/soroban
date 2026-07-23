//! A currency amount — the payload of `Value::Money`, a first-class tagged type
//! alongside `FixedInt` (`Int32(…)`) and `FixedDecimal` (`Decimal(…)`). Written
//! as a literal (`$10`, `€10` — core grammar, any mode) or the constructor
//! `Money(10, "USD")`. See docs/MODES.md.
//!
//! The currency propagates through arithmetic the way `FixedDecimal`'s type
//! does: money in, money out, with a plain `Number` (or a grouped number)
//! absorbed. That is what makes `$10 * 5%` answer `$0.50` — `5%` has already
//! evaluated to a plain `0.05` by the time the multiply sees it. Two different
//! currencies are refused: there is no exchange rate to apply.

use super::currency::Currency;
use super::value::Value;
use crate::ast::BinaryOperator;
use crate::{BigDecimal, EngineError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub value: BigDecimal,
    pub currency: Currency,
}

impl Money {
    pub fn new(value: BigDecimal, currency: Currency) -> Self {
        Self { value, currency }
    }

    /// "USD amount" — for `kind_name` and error messages.
    pub fn type_name(&self) -> String {
        format!("{} amount", self.currency.code())
    }

    /// The display form: grouped, 2 decimals, symbol outside the sign
    /// (`-$1,234.50`, `CHF 10.00`) — matching the sheet's currency format.
    pub fn text(&self) -> String {
        let magnitude = if self.value.is_negative() {
            -&self.value
        } else {
            self.value.clone()
        };
        let sign = if self.value.is_negative() { "-" } else { "" };
        format!(
            "{sign}{}{}",
            self.currency.symbol(),
            magnitude.grouped_text(2)
        )
    }

    /// Canonical, re-parseable spelling — the constructor call `Money(10, "USD")`,
    /// which restores by evaluation (like `Decimal(…)` / a record) in ANY mode.
    /// The value is EXACT (not rounded to 2dp), so the round trip is lossless.
    pub fn description(&self) -> String {
        format!("Money({}, \"{}\")", self.value, self.currency.code())
    }
}

// MARK: - Typed arithmetic (the mixing matrix)

impl Money {
    /// True when money arithmetic applies — either operand is a currency amount.
    pub fn is_involved(lhs: &Value, rhs: &Value) -> bool {
        matches!(lhs, Value::Money(_)) || matches!(rhs, Value::Money(_))
    }

    /// `+ − × ÷` on money operands (docs/MODES.md). The currency propagates
    /// through all four, so a money input always reads back as money — the tag
    /// is a display contract, not a unit system, so it never models
    /// dimensionality (`$10 * $2` is `$20.00`). Two DIFFERENT currencies are
    /// refused. `^` and modulo refuse a currency (convert to a Number first).
    pub fn apply_binary(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        let currency = Self::resolved_currency(lhs, rhs)?;
        let a = Self::operand(lhs)?;
        let b = Self::operand(rhs)?;
        match op {
            BinaryOperator::Add => Ok(Value::Money(Money::new(&a + &b, currency))),
            BinaryOperator::Subtract => Ok(Value::Money(Money::new(&a - &b, currency))),
            BinaryOperator::Multiply => Ok(Value::Money(Money::new(&a * &b, currency))),
            // BigDecimal division errors on a zero divisor.
            BinaryOperator::Divide => Ok(Value::Money(Money::new(a.div(&b)?, currency))),
            BinaryOperator::Modulo | BinaryOperator::Power => {
                let what = if op == BinaryOperator::Power {
                    "^ (power)"
                } else {
                    "modulo"
                };
                Err(EngineError::domain(format!(
                    "a currency amount doesn't support {what} — convert it to a Number first"
                )))
            }
        }
    }

    /// The surviving currency. Two different currencies are a hard error; a
    /// plain Number or grouped number yields to the money operand's currency.
    fn resolved_currency(lhs: &Value, rhs: &Value) -> Result<Currency, EngineError> {
        match (lhs, rhs) {
            (Value::Money(a), Value::Money(b)) => {
                if a.currency != b.currency {
                    return Err(EngineError::domain(format!(
                        "can't mix currencies ({} and {}) — convert one first",
                        a.currency.code(),
                        b.currency.code()
                    )));
                }
                Ok(a.currency)
            }
            (Value::Money(a), _) => Ok(a.currency),
            (_, Value::Money(b)) => Ok(b.currency),
            _ => Err(EngineError::domain(
                "money arithmetic with no currency operand",
            )),
        }
    }

    /// An operand as an exact BigDecimal. Money uses its value; a plain Number
    /// or a grouped number is absorbed. Anything else — a fixed-width int or
    /// decimal — errors (cross-family; convert explicitly).
    fn operand(value: &Value) -> Result<BigDecimal, EngineError> {
        match value {
            Value::Money(m) => Ok(m.value.clone()),
            Value::Number(n) => Ok(n.clone()),
            Value::Grouped(n) => Ok(n.clone()),
            _ => Err(EngineError::domain(format!(
                "can't combine {} with a currency amount — convert it first",
                value.kind_name()
            ))),
        }
    }
}
