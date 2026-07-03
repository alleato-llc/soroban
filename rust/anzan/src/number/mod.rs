//! Arbitrary-precision base-10 numbers — the engine's core invariant.
//! `BigDecimal` = `BigInt` significand × 10^exponent, always normalized (no
//! trailing zeros), so equality is structural. `+ − ×`, integer `^`, and `%`
//! are exact; `/` and `sqrt` round to `PrecisionContext::current()`
//! significant digits (default 50, banker's rounding). Transcendentals
//! round-trip through f64 — deliberately confined to `via_double` in
//! `math.rs`; route any new inexact function through that seam.

mod math;

use crate::EngineError;
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::Zero;
use std::cell::Cell;
use std::cmp::Ordering;
use std::ops::{Add, Mul, Neg, Sub};

/// Arbitrary-precision base-10 number: `significand × 10^exponent`.
///
/// Addition, subtraction, and multiplication are exact. Division and roots are
/// computed to `PrecisionContext::current()` significant digits. Values are
/// kept normalized (no trailing zeros in the significand) so equality is
/// structural.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BigDecimal {
    significand: BigInt,
    exponent: i64,
}

impl BigDecimal {
    pub fn new(significand: BigInt, exponent: i64) -> Self {
        let mut value = Self {
            significand,
            exponent,
        };
        value.normalize();
        value
    }

    pub fn from_int(value: i64) -> Self {
        Self::new(BigInt::from(value), 0)
    }

    pub fn zero() -> Self {
        Self::from_int(0)
    }

    pub fn one() -> Self {
        Self::from_int(1)
    }

    pub fn significand(&self) -> &BigInt {
        &self.significand
    }

    pub fn exponent(&self) -> i64 {
        self.exponent
    }

    pub fn is_zero(&self) -> bool {
        self.significand.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.significand.sign() == Sign::Minus
    }

    /// True when the value has no fractional part.
    pub fn is_integer(&self) -> bool {
        self.exponent >= 0
    }

    /// Strips trailing zeros from the significand into the exponent; zero
    /// gets exponent 0.
    fn normalize(&mut self) {
        if self.significand.is_zero() {
            self.exponent = 0;
            return;
        }
        let ten = BigInt::from(10);
        loop {
            let (q, r) = self.significand.div_rem(&ten);
            if !r.is_zero() {
                break;
            }
            self.significand = q;
            self.exponent += 1;
        }
    }

    /// Number of significant decimal digits in the significand.
    pub(crate) fn digit_count(&self) -> i64 {
        if self.significand.is_zero() {
            return 1;
        }
        self.significand.magnitude().to_string().len() as i64
    }
}

// MARK: - Precision context

thread_local! {
    static PRECISION: Cell<usize> = const { Cell::new(50) };
}

/// Working precision for inexact operations (division, roots,
/// transcendentals). The Swift side scopes this with a task-local; evaluation
/// is single-threaded by discipline in both worlds, so a thread-local with a
/// scoped override is the same contract.
pub struct PrecisionContext;

impl PrecisionContext {
    /// Significant digits carried by inexact operations.
    pub fn current() -> usize {
        PRECISION.with(Cell::get)
    }

    /// Runs `body` with the working precision set to `digits`, restoring the
    /// previous value afterwards (even on panic-free early return).
    pub fn with<R>(digits: usize, body: impl FnOnce() -> R) -> R {
        struct Restore(usize);
        impl Drop for Restore {
            fn drop(&mut self) {
                PRECISION.with(|cell| cell.set(self.0));
            }
        }
        let _restore = Restore(PRECISION.with(|cell| cell.replace(digits)));
        body()
    }
}

// MARK: - Parsing

impl BigDecimal {
    /// Parses a literal: `123`, `-1.5`, `1_000`, `2.5e-3`.
    pub fn parse(string: &str) -> Option<Self> {
        let mut mantissa = string.to_string();
        let mut exp10: i64 = 0;

        // Split exponent part.
        if let Some(e_index) = mantissa.find(['e', 'E']) {
            let exp_part = &mantissa[e_index + 1..];
            exp10 = exp_part.parse().ok()?;
            mantissa.truncate(e_index);
        }

        mantissa.retain(|c| c != '_');
        if mantissa.is_empty() {
            return None;
        }

        // Split fractional part.
        if let Some(dot_index) = mantissa.find('.') {
            let fraction = &mantissa[dot_index + 1..];
            if fraction.contains('.') {
                return None;
            }
            exp10 -= fraction.len() as i64;
            mantissa.remove(dot_index);
        }

        if mantissa.is_empty() || mantissa == "-" || mantissa == "+" {
            return None;
        }
        let significand: BigInt = mantissa.parse().ok()?;
        Some(Self::new(significand, exp10))
    }
}

// MARK: - Comparison

impl BigDecimal {
    /// Rescales both values to a common exponent and returns the significands.
    fn aligned(lhs: &Self, rhs: &Self) -> (BigInt, BigInt) {
        let common = lhs.exponent.min(rhs.exponent);
        let ten = BigInt::from(10);
        let l = &lhs.significand * ten.pow((lhs.exponent - common) as u32);
        let r = &rhs.significand * ten.pow((rhs.exponent - common) as u32);
        (l, r)
    }
}

impl PartialOrd for BigDecimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigDecimal {
    fn cmp(&self, other: &Self) -> Ordering {
        // Normalization makes structural equality correct; ordering aligns.
        let (l, r) = Self::aligned(self, other);
        l.cmp(&r)
    }
}

// MARK: - Exact arithmetic

impl Add for &BigDecimal {
    type Output = BigDecimal;
    fn add(self, rhs: &BigDecimal) -> BigDecimal {
        let common = self.exponent.min(rhs.exponent);
        let (l, r) = BigDecimal::aligned(self, rhs);
        BigDecimal::new(l + r, common)
    }
}

impl Sub for &BigDecimal {
    type Output = BigDecimal;
    fn sub(self, rhs: &BigDecimal) -> BigDecimal {
        self + &(-rhs)
    }
}

impl Neg for &BigDecimal {
    type Output = BigDecimal;
    fn neg(self) -> BigDecimal {
        BigDecimal {
            significand: -&self.significand,
            exponent: self.exponent,
        }
    }
}

impl Mul for &BigDecimal {
    type Output = BigDecimal;
    fn mul(self, rhs: &BigDecimal) -> BigDecimal {
        BigDecimal::new(
            &self.significand * &rhs.significand,
            self.exponent + rhs.exponent,
        )
    }
}

macro_rules! forward_owned_binop {
    ($trait:ident, $method:ident) => {
        impl $trait for BigDecimal {
            type Output = BigDecimal;
            fn $method(self, rhs: BigDecimal) -> BigDecimal {
                (&self).$method(&rhs)
            }
        }
    };
}
forward_owned_binop!(Add, add);
forward_owned_binop!(Sub, sub);
forward_owned_binop!(Mul, mul);

impl Neg for BigDecimal {
    type Output = BigDecimal;
    fn neg(self) -> BigDecimal {
        -&self
    }
}

// MARK: - Rounding & division

impl BigDecimal {
    /// Rounds so that at most `digits` significant digits remain (banker's
    /// rounding).
    pub fn rounded_to_significant_digits(&self, digits: usize) -> Self {
        let excess = self.digit_count() - digits as i64;
        if excess <= 0 {
            return self.clone();
        }
        let scale = BigInt::from(10).pow(excess as u32);
        let (q, r) = self.significand.div_rem(&scale);
        Self::new(Self::round_half_even(q, &r, &scale), self.exponent + excess)
    }

    /// Rounds to `places` decimal places (banker's rounding). Negative
    /// `places` rounds left of the decimal point (`round(1234, -2)` → `1200`).
    pub fn rounded_to_places(&self, places: i64) -> Self {
        if self.exponent >= -places {
            return self.clone();
        }
        let scale = BigInt::from(10).pow((-places - self.exponent) as u32);
        let (q, r) = self.significand.div_rem(&scale);
        Self::new(Self::round_half_even(q, &r, &scale), -places)
    }

    /// Banker's rounding of `quotient` given the discarded `remainder`.
    fn round_half_even(quotient: BigInt, remainder: &BigInt, divisor: &BigInt) -> BigInt {
        if remainder.is_zero() {
            return quotient;
        }
        let twice = remainder.magnitude() * 2u32;
        let bump = match twice.cmp(divisor.magnitude()) {
            Ordering::Greater => true,
            Ordering::Less => false,
            // Exactly half: round to even.
            Ordering::Equal => quotient.is_odd(),
        };
        if !bump {
            return quotient;
        }
        let step = if remainder.sign() == Sign::Minus {
            -1
        } else {
            1
        };
        quotient + step
    }

    /// Rounds to `places` decimal places with half **away from zero** (Java
    /// HALF_UP) — the fixed-precision `decimal` type's `Rounding.HalfUp` mode.
    pub fn rounded_half_up_to_places(&self, places: i64) -> Self {
        if self.exponent >= -places {
            return self.clone();
        }
        let scale = BigInt::from(10).pow((-places - self.exponent) as u32);
        let (q, r) = self.significand.div_rem(&scale);
        Self::new(Self::round_half_up(q, &r, &scale), -places)
    }

    /// Half-or-more rounds away from zero (no even tie-break).
    fn round_half_up(quotient: BigInt, remainder: &BigInt, divisor: &BigInt) -> BigInt {
        if remainder.is_zero() {
            return quotient;
        }
        if remainder.magnitude() * 2u32 < *divisor.magnitude() {
            return quotient;
        }
        let step = if remainder.sign() == Sign::Minus {
            -1
        } else {
            1
        };
        quotient + step
    }

    /// Division to `PrecisionContext::current()` significant digits.
    /// Exact when the quotient terminates within the working precision.
    pub fn div(&self, rhs: &Self) -> Result<Self, EngineError> {
        if rhs.is_zero() {
            return Err(EngineError::DivisionByZero);
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }

        let precision = PrecisionContext::current() as i64;
        // Scale the dividend so the integer quotient carries `precision` +
        // guard digits.
        let shift = rhs.digit_count() - self.digit_count() + precision + 2;
        let mut numerator = self.significand.clone();
        let mut exponent = self.exponent - rhs.exponent;
        if shift > 0 {
            numerator *= BigInt::from(10).pow(shift as u32);
            exponent -= shift;
        }
        let (q, r) = numerator.div_rem(&rhs.significand);
        let quotient = Self::round_half_even(q, &r, &rhs.significand);
        Ok(Self::new(quotient, exponent).rounded_to_significant_digits(precision as usize))
    }

    /// Truncated integer division remainder, matching the sign of the
    /// dividend.
    pub fn rem(&self, rhs: &Self) -> Result<Self, EngineError> {
        if rhs.is_zero() {
            return Err(EngineError::DivisionByZero);
        }
        let common = self.exponent.min(rhs.exponent);
        let (l, r) = Self::aligned(self, rhs);
        Ok(Self::new(l % r, common))
    }
}

#[cfg(test)]
mod tests;

// MARK: - Integer extraction (the Swift intValue / bigIntValue helpers)

impl BigDecimal {
    /// The exact `i64` value of an integer within i64 range; `None` otherwise.
    pub fn int_value(&self) -> Option<i64> {
        if !self.is_integer() || self.exponent > 18 {
            return None;
        }
        self.formatted(i64::MAX).parse().ok()
    }

    /// The exact BigInt value of an integer — unlike `int_value`, no 2^63
    /// ceiling (a normalized 1e40 has significand 1, exponent 40).
    pub fn big_int_value(&self) -> Option<BigInt> {
        if !self.is_integer() || self.exponent < 0 {
            return None;
        }
        if self.exponent > 10_000 {
            return None; // refuse absurd widths
        }
        Some(&self.significand * BigInt::from(10).pow(self.exponent as u32))
    }
}
