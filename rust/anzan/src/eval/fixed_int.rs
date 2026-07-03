//! A bounded, checked integer value — the payload of `Value::FixedInt`,
//! built by the `Int8…Int256` / `UInt8…UInt256` per-width constructors (or
//! `Int(value, bits)` / `UInt(value, bits)`). Exact like the rest of the
//! engine, but with a declared width: arithmetic that leaves the range is an
//! ERROR, never a wraparound. See `docs/FIXED-WIDTH.md`.

use super::value::Value;
use crate::ast::BinaryOperator;
use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedInt {
    pub value: BigInt,
    pub bits: u32,
    pub signed: bool,
}

/// Allowed widths. Cheap underneath (everything is `BigInt`); the set is a
/// deliberate, documented choice rather than a representational limit.
pub const ALLOWED_WIDTHS: [u32; 6] = [8, 16, 32, 64, 128, 256];

impl FixedInt {
    /// Validating constructor: rejects a non-allowed width or an out-of-range
    /// value (the checked-range contract — there is no silent truncation).
    pub fn new(value: BigInt, bits: u32, signed: bool) -> Result<Self, EngineError> {
        if !ALLOWED_WIDTHS.contains(&bits) {
            return Err(EngineError::domain(format!(
                "fixed-width needs a width of 8, 16, 32, 64, 128, or 256 — got {bits}"
            )));
        }
        let lo = Self::min_value(bits, signed);
        let hi = Self::max_value(bits, signed);
        if value < lo || value > hi {
            return Err(EngineError::domain(format!(
                "{value} is out of range for {} ({lo} … {hi})",
                Self::type_name_for(bits, signed)
            )));
        }
        Ok(Self {
            value,
            bits,
            signed,
        })
    }

    pub(crate) fn min_value(bits: u32, signed: bool) -> BigInt {
        if signed {
            -(BigInt::from(1) << (bits - 1))
        } else {
            BigInt::from(0)
        }
    }

    pub(crate) fn max_value(bits: u32, signed: bool) -> BigInt {
        if signed {
            (BigInt::from(1) << (bits - 1)) - 1
        } else {
            (BigInt::from(1) << bits) - 1
        }
    }

    pub(crate) fn type_name_for(bits: u32, signed: bool) -> String {
        format!("{}{bits}", if signed { "Int" } else { "UInt" })
    }

    /// e.g. "Int32", "UInt8" — for error messages and `kind_name`.
    pub fn type_name(&self) -> String {
        Self::type_name_for(self.bits, self.signed)
    }

    /// Canonical, re-parseable constructor spelling — the per-width form
    /// `Int32(27374)` / `UInt8(255)`. Restores by *evaluation* (like a
    /// record); the parameterized `Int(v, bits)` form re-parses to it too.
    pub fn description(&self) -> String {
        format!("{}({})", self.type_name(), self.value)
    }

    /// The plain decimal value — for comparison, truthiness, and numeric
    /// coercion *outside* typed arithmetic (typed arithmetic stays in
    /// `apply`).
    pub fn decimal(&self) -> BigDecimal {
        BigDecimal::new(self.value.clone(), 0)
    }
}

// MARK: - Typed arithmetic (the mixing matrix + checked overflow)

type ResolvedType = (u32, bool); // (bits, signed)

impl FixedInt {
    /// True when fixed-width arithmetic applies — either operand is a
    /// FixedInt. The evaluator routes to `apply_binary` in that case, before
    /// numeric coercion.
    pub fn is_involved(lhs: &Value, rhs: &Value) -> bool {
        matches!(lhs, Value::FixedInt(_)) || matches!(rhs, Value::FixedInt(_))
    }

    /// `+ − × ÷` and `^`-power on fixed-width operands
    /// (docs/FIXED-WIDTH.md): width promotes toward the widest type present;
    /// sign never promotes (mismatch → error); a `Decimal` non-integer never
    /// mixes; every result is range-checked and **errors rather than wraps**.
    /// Precondition: `is_involved(lhs, rhs)`.
    pub fn apply_binary(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<Value, EngineError> {
        // Power is special: the exponent is a COUNT (exempt from the matrix);
        // the result follows the base. A numeric base with a fixed-width
        // exponent is just ordinary numeric power.
        if op == BinaryOperator::Power {
            return Self::apply_power(lhs, rhs);
        }

        let (bits, signed) = Self::resolved_type(lhs, rhs)?;
        let a = Self::operand(lhs, (bits, signed))?;
        let b = Self::operand(rhs, (bits, signed))?;
        let raw = match op {
            BinaryOperator::Add => a + b,
            BinaryOperator::Subtract => a - b,
            BinaryOperator::Multiply => a * b,
            BinaryOperator::Divide => {
                if b.is_zero() {
                    return Err(EngineError::DivisionByZero);
                }
                a / b // truncating toward zero, like C/Rust
            }
            BinaryOperator::Modulo => {
                if b.is_zero() {
                    return Err(EngineError::DivisionByZero);
                }
                a % b
            }
            BinaryOperator::Power => unreachable!("handled above"),
        };
        Ok(Value::FixedInt(FixedInt::new(raw, bits, signed)?))
    }

    fn apply_power(lhs: &Value, rhs: &Value) -> Result<Value, EngineError> {
        let exponent_value = rhs.as_number("^")?;
        let Value::FixedInt(base) = lhs else {
            // Numeric base, fixed-width exponent → ordinary numeric power.
            return Ok(Value::Number(super::numeric::pow(
                &lhs.as_number("^")?,
                &exponent_value,
            )?));
        };
        let exponent = exponent_value
            .big_int_value()
            .filter(|e| !e.is_negative())
            .and_then(|e| u32::try_from(e).ok())
            .ok_or_else(|| {
                EngineError::domain("a fixed-width base needs a non-negative integer exponent")
            })?;
        Ok(Value::FixedInt(FixedInt::new(
            base.value.pow(exponent),
            base.bits,
            base.signed,
        )?))
    }

    /// Result type: largest width wins; sign never promotes (mismatch →
    /// error).
    fn resolved_type(lhs: &Value, rhs: &Value) -> Result<ResolvedType, EngineError> {
        match (lhs, rhs) {
            (Value::FixedInt(a), Value::FixedInt(b)) => {
                if a.signed != b.signed {
                    return Err(EngineError::domain(format!(
                        "can't mix {} and {} — signed and unsigned never combine; cast one explicitly",
                        a.type_name(),
                        b.type_name()
                    )));
                }
                Ok((a.bits.max(b.bits), a.signed))
            }
            (Value::FixedInt(a), _) => Ok((a.bits, a.signed)),
            (_, Value::FixedInt(b)) => Ok((b.bits, b.signed)),
            _ => Err(EngineError::domain(
                "fixed-width arithmetic with no fixed-width operand",
            )),
        }
    }

    /// An operand as a BigInt in the result type. A FixedInt uses its value
    /// (same sign guaranteed; a smaller width fits the larger). A plain
    /// number must be a whole number and **adopts** the type — range-checked,
    /// so an out-of-range or fractional literal **errors** (no silent
    /// truncation, no decimal mixing).
    fn operand(value: &Value, (bits, signed): ResolvedType) -> Result<BigInt, EngineError> {
        match value {
            Value::FixedInt(f) => Ok(f.value.clone()),
            Value::Number(n) => {
                let Some(i) = n.big_int_value() else {
                    return Err(EngineError::domain(format!(
                        "fixed-width arithmetic needs whole numbers — {n} isn't an integer"
                    )));
                };
                Ok(FixedInt::new(i, bits, signed)?.value)
            }
            _ => Err(EngineError::domain(format!(
                "can't combine {} with a fixed-width integer",
                value.kind_name()
            ))),
        }
    }
}

// MARK: - Bitwise (two's-complement over the width)

impl FixedInt {
    /// AND/OR/XOR on fixed-width operands, type-preserving. Signed values
    /// operate in two's-complement over the (promoted) width; the result is
    /// in range by construction. Shares the arithmetic mixing matrix (largest
    /// width, sign must match, a plain number adopts the type). `op` is the
    /// BigInt bit operation.
    pub fn apply_bitwise(
        lhs: &Value,
        rhs: &Value,
        op: impl FnOnce(BigInt, BigInt) -> BigInt,
    ) -> Result<Value, EngineError> {
        let ty = Self::resolved_type(lhs, rhs)?;
        let result = op(Self::bit_pattern(lhs, ty)?, Self::bit_pattern(rhs, ty)?);
        Ok(Value::FixedInt(FixedInt::new(
            Self::decode(result, ty),
            ty.0,
            ty.1,
        )?))
    }

    /// Bitwise NOT over the width: `~x` flips every bit. `~Int8(0)` →
    /// `Int8(-1)` (= −x−1); `~UInt8(0)` → `UInt8(255)`. In range by
    /// construction.
    pub fn bitwise_not(&self) -> Result<FixedInt, EngineError> {
        let width = BigInt::from(1) << self.bits;
        let pat = if self.value.is_negative() {
            &self.value + &width
        } else {
            self.value.clone()
        };
        let complement = (width - 1) ^ pat;
        FixedInt::new(
            Self::decode(complement, (self.bits, self.signed)),
            self.bits,
            self.signed,
        )
    }

    /// A value's unsigned two's-complement bit pattern in [0, 2^bits). A
    /// negative signed value wraps into range; a plain number adopts + is
    /// range-checked.
    fn bit_pattern(value: &Value, ty: ResolvedType) -> Result<BigInt, EngineError> {
        let raw = Self::operand(value, ty)?;
        Ok(if raw.is_negative() {
            raw + (BigInt::from(1) << ty.0)
        } else {
            raw
        })
    }

    /// Reverses `bit_pattern`: a high bit means negative for a signed type.
    fn decode(pattern: BigInt, (bits, signed): ResolvedType) -> BigInt {
        if signed && pattern >= (BigInt::from(1) << (bits - 1)) {
            return pattern - (BigInt::from(1) << bits);
        }
        pattern
    }
}
