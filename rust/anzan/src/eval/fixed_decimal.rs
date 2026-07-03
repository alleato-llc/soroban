//! A bounded, checked fixed-precision decimal — the payload of
//! `Value::FixedDecimal`, built by `Decimal(value, precision, scale[,
//! Rounding.X])`. SQL `DECIMAL(p, s)`: at most `precision` significant
//! digits, exactly `scale` fractional digits. The value is rounded to
//! `scale`; exceeding `precision` is an overflow error (the checked-range
//! contract, like `Int`/`UInt`). See docs/DECIMAL.md.

use super::value::Value;
use crate::ast::BinaryOperator;
use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;
use num_traits::Signed;

/// How a `Decimal` rounds its value to scale — a constructor option, carried
/// with the value so it governs every later rounding too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalRounding {
    /// Round half to even (the engine's standard, the default).
    Bankers,
    /// Round half away from zero (Java HALF_UP).
    HalfUp,
}

impl DecimalRounding {
    /// The raw-value spelling used in mixing-error messages ("bankers",
    /// "halfUp" — the Swift enum's rawValue).
    pub fn raw_name(&self) -> &'static str {
        match self {
            Self::Bankers => "bankers",
            Self::HalfUp => "halfUp",
        }
    }
}

/// The largest precision a Decimal may declare (matches PostgreSQL's declared
/// NUMERIC ceiling). Since `scale <= precision`, this is also the maximum
/// scale. The cap keeps the `10^precision` range check bounded and gives a
/// coherent upper limit on declared digits.
pub const MAX_PRECISION: i64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedDecimal {
    /// Already rounded to `scale` and within `precision`.
    pub value: BigDecimal,
    pub precision: i64,
    pub scale: i64,
    pub rounding: DecimalRounding,
}

impl FixedDecimal {
    pub fn new(
        raw_value: BigDecimal,
        precision: i64,
        scale: i64,
        rounding: DecimalRounding,
    ) -> Result<Self, EngineError> {
        if !(1..=MAX_PRECISION).contains(&precision) {
            return Err(EngineError::domain(format!(
                "Decimal precision must be between 1 and {MAX_PRECISION}, got {precision}"
            )));
        }
        if scale < 0 || scale > precision {
            return Err(EngineError::domain(format!(
                "Decimal scale must be between 0 and the precision ({precision}), got {scale}"
            )));
        }
        let rounded = match rounding {
            DecimalRounding::HalfUp => raw_value.rounded_half_up_to_places(scale),
            DecimalRounding::Bankers => raw_value.rounded_to_places(scale),
        };
        // unscaled = rounded × 10^scale (an integer); must fit `precision`
        // digits.
        let unscaled =
            rounded.significand() * BigInt::from(10).pow((rounded.exponent() + scale) as u32);
        if *unscaled.magnitude() >= *BigInt::from(10).pow(precision as u32).magnitude() {
            return Err(EngineError::domain(format!(
                "{} exceeds Decimal({precision}, {scale}) — more than {precision} digits",
                Self::render(&rounded, scale)
            )));
        }
        Ok(Self {
            value: rounded,
            precision,
            scale,
            rounding,
        })
    }

    /// e.g. "Decimal(5, 2)" — for `kind_name` / error messages.
    pub fn type_name(&self) -> String {
        format!("Decimal({}, {})", self.precision, self.scale)
    }

    /// The value padded to exactly `scale` fractional digits — "10.50",
    /// "0.05".
    pub fn text(&self) -> String {
        Self::render(&self.value, self.scale)
    }

    fn render(value: &BigDecimal, scale: i64) -> String {
        let unscaled =
            value.significand() * BigInt::from(10).pow((value.exponent() + scale) as u32);
        let negative = unscaled.is_negative();
        let mut digits = unscaled.magnitude().to_string();
        if scale > 0 {
            let scale = scale as usize;
            if digits.len() <= scale {
                digits = "0".repeat(scale - digits.len() + 1) + &digits;
            }
            let cut = digits.len() - scale;
            digits = format!("{}.{}", &digits[..cut], &digits[cut..]);
        }
        format!("{}{digits}", if negative { "-" } else { "" })
    }

    /// Canonical, re-parseable constructor spelling (restores by evaluation,
    /// like a record) — the SHORTEST form that round-trips. A max-precision,
    /// banker's-rounded value hides the precision (it's the default): the
    /// 1-arg `Decimal(0.5)` when the scale is the value's own, else the 2-arg
    /// `Decimal(0.50, 2)`. Everything else is the full form; the rounding arg
    /// appears only when non-default.
    pub fn description(&self) -> String {
        if self.precision == MAX_PRECISION && self.rounding == DecimalRounding::Bankers {
            // The value's own number of decimal places (scale can only be ≥
            // this, since the value was rounded to scale on construction).
            let natural_scale = 0.max(-self.value.exponent());
            if self.scale == natural_scale {
                return format!("Decimal({})", self.text());
            }
            return format!("Decimal({}, {})", self.text(), self.scale);
        }
        let mode = if self.rounding == DecimalRounding::HalfUp {
            ", Rounding.HalfUp"
        } else {
            ""
        };
        format!(
            "Decimal({}, {}, {}{mode})",
            self.text(),
            self.precision,
            self.scale
        )
    }
}

// MARK: - Typed arithmetic (the mixing matrix + checked overflow)

type ResolvedType = (i64, i64, DecimalRounding); // (precision, scale, rounding)

impl FixedDecimal {
    /// True when fixed-precision arithmetic applies — either operand is a
    /// decimal.
    pub fn is_involved(lhs: &Value, rhs: &Value) -> bool {
        matches!(lhs, Value::FixedDecimal(_)) || matches!(rhs, Value::FixedDecimal(_))
    }

    /// `+ − × ÷` on fixed-precision operands (docs/DECIMAL.md): scale and
    /// precision promote to the widest; rounding never reconciles (mismatch →
    /// error); a plain `Number` is absorbed and rounded to the decimal's
    /// scale; the result is range-checked and **errors rather than wraps**.
    pub fn apply_binary(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        let (precision, scale, rounding) = Self::resolved_type(lhs, rhs)?;
        let a = Self::operand(lhs)?;
        let b = Self::operand(rhs)?;
        let raw = match op {
            BinaryOperator::Add => &a + &b,
            BinaryOperator::Subtract => &a - &b,
            BinaryOperator::Multiply => &a * &b,
            // BigDecimal division errors on a zero divisor.
            BinaryOperator::Divide => a.div(&b)?,
            BinaryOperator::Modulo | BinaryOperator::Power => {
                let what = if op == BinaryOperator::Power {
                    "^ (power)"
                } else {
                    "modulo"
                };
                return Err(EngineError::domain(format!(
                    "a fixed-precision decimal doesn't support {what} — convert it to a Number first"
                )));
            }
        };
        Ok(Value::FixedDecimal(FixedDecimal::new(
            raw, precision, scale, rounding,
        )?))
    }

    fn resolved_type(lhs: &Value, rhs: &Value) -> Result<ResolvedType, EngineError> {
        match (lhs, rhs) {
            (Value::FixedDecimal(a), Value::FixedDecimal(b)) => {
                if a.rounding != b.rounding {
                    return Err(EngineError::domain(format!(
                        "can't mix decimals with different rounding ({} and {}) — cast one",
                        a.rounding.raw_name(),
                        b.rounding.raw_name()
                    )));
                }
                Ok((
                    a.precision.max(b.precision),
                    a.scale.max(b.scale),
                    a.rounding,
                ))
            }
            (Value::FixedDecimal(a), _) => Ok((a.precision, a.scale, a.rounding)),
            (_, Value::FixedDecimal(b)) => Ok((b.precision, b.scale, b.rounding)),
            _ => Err(EngineError::domain(
                "fixed-precision arithmetic with no decimal operand",
            )),
        }
    }

    /// An operand as an exact BigDecimal. A decimal uses its value; a plain
    /// Number is absorbed exactly (rounded to scale on the result). Anything
    /// else — including a fixed-width int — errors (cross-family; cast
    /// explicitly).
    fn operand(value: &Value) -> Result<BigDecimal, EngineError> {
        match value {
            Value::FixedDecimal(d) => Ok(d.value.clone()),
            Value::Number(n) => Ok(n.clone()),
            _ => Err(EngineError::domain(format!(
                "can't combine {} with a fixed-precision decimal — cast it (e.g. Decimal(…))",
                value.kind_name()
            ))),
        }
    }
}
