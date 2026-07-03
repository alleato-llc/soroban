//! Exact powers & roots, the f64 bridge (transcendental fallback), and
//! formatting — the port of `BigDecimal+Math.swift`.

use super::{BigDecimal, PrecisionContext};
use crate::EngineError;
use num_bigint::BigInt;
use std::fmt;

// MARK: - Exact powers & roots

impl BigDecimal {
    /// Raises to an integer power. Exact for positive exponents; negative
    /// exponents divide at working precision.
    pub fn power(&self, n: i64) -> Result<Self, EngineError> {
        if n == 0 {
            if self.is_zero() {
                return Err(EngineError::domain("0^0 is undefined"));
            }
            return Ok(Self::one());
        }
        if n < 0 {
            return Self::one().div(&self.power(-n)?);
        }
        // Keep pathological inputs (9^999999999) from hanging the app.
        if self.digit_count().saturating_mul(n) > 1_000_000 {
            return Err(EngineError::domain("result of ^ is too large"));
        }
        Ok(Self::new(
            self.significand.pow(n as u32),
            self.exponent * n,
        ))
    }

    /// Square root via Newton iteration, to working precision.
    /// Exact when the root terminates (e.g. sqrt(2.25) == 1.5).
    pub fn square_root(&self) -> Result<Self, EngineError> {
        if self.is_negative() {
            return Err(EngineError::domain("sqrt of a negative number"));
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }

        let precision = PrecisionContext::current() as i64;
        // Work on an integer scaled so the integer sqrt carries enough digits:
        // value = sig × 10^exp; choose even shift s ≥ 0 with exp - s even,
        // then sqrt = isqrt(sig × 10^s) × 10^((exp - s) / 2).
        let mut shift = 2 * (precision + 2 - self.digit_count() / 2);
        if shift < 0 {
            shift = 0;
        }
        if (self.exponent - shift) % 2 != 0 {
            shift += 1;
        }

        let scaled = &self.significand * BigInt::from(10).pow(shift as u32);
        let root = scaled.sqrt(); // floor square root
        let exact = &root * &root == scaled;
        let result = Self::new(root, (self.exponent - shift) / 2);
        Ok(if exact {
            result
        } else {
            result.rounded_to_significant_digits(precision as usize)
        })
    }
}

// MARK: - f64 bridging (transcendental fallback)

impl BigDecimal {
    /// Lossy conversion for transcendental fallback and UI affordances.
    pub fn to_f64(&self) -> f64 {
        self.to_string().parse().unwrap_or(f64::NAN)
    }

    /// Converts a finite f64 exactly enough: parses the shortest decimal
    /// string that round-trips the double (Rust's `Display` guarantee), so
    /// artifacts of the binary representation (0.1000000000000000055511...)
    /// don't leak into results.
    pub fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        Self::parse(&shortest_string(value))
    }

    /// Applies an f64-domain function, round-tripping through ~15 significant
    /// digits. This is the single seam to replace with true
    /// arbitrary-precision series implementations later — callers won't
    /// change. New inexact functions must route through here, never do f64
    /// math elsewhere in the engine.
    pub(crate) fn via_double(
        name: &str,
        value: &BigDecimal,
        f: impl FnOnce(f64) -> f64,
    ) -> Result<Self, EngineError> {
        let result = f(value.to_f64());
        Self::from_f64(result)
            .ok_or_else(|| EngineError::domain(format!("{name} is undefined for {value}")))
    }
}

/// Shortest decimal string that round-trips the double. Mirrors the Swift
/// helper: integral values below 1e15 print without a fraction; everything
/// else uses the shortest round-trip form (Rust's `{}` for f64).
fn shortest_string(value: f64) -> String {
    if value == value.round() && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

// MARK: - Formatting

impl fmt::Display for BigDecimal {
    /// Plain decimal form: `-12.5`, `0.03`. Falls back to scientific notation
    /// when the plain form would be absurdly long.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.formatted(30))
    }
}

impl BigDecimal {
    pub fn formatted(&self, scientific_threshold: i64) -> String {
        if self.is_zero() {
            return "0".to_string();
        }

        let digits = self.significand.magnitude().to_string();
        let sign = if self.is_negative() { "-" } else { "" };
        // Position of the decimal point relative to the digit string.
        let point_position = digits.len() as i64 + self.exponent;

        // Too many digits either side → scientific notation.
        if point_position > scientific_threshold || point_position < -scientific_threshold {
            return self.scientific_description(&digits, sign);
        }

        if self.exponent >= 0 {
            return format!("{sign}{digits}{}", "0".repeat(self.exponent as usize));
        }
        if point_position <= 0 {
            return format!("{sign}0.{}{digits}", "0".repeat(-point_position as usize));
        }
        let (head, tail) = digits.split_at(point_position as usize);
        format!("{sign}{head}.{tail}")
    }

    fn scientific_description(&self, digits: &str, sign: &str) -> String {
        let exp = digits.len() as i64 + self.exponent - 1;
        let head = &digits[..1];
        let tail = &digits[1..];
        let mantissa = if tail.is_empty() {
            head.to_string()
        } else {
            format!("{head}.{tail}")
        };
        let plus = if exp >= 0 { "+" } else { "" };
        format!("{sign}{mantissa}e{plus}{exp}")
    }
}
