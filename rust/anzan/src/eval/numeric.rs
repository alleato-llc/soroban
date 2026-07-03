//! Shared numeric helpers used by both the operator table and the registry
//! (the Swift `enum Functions` in CoreFunctions.swift — ported piecemeal as
//! the function lists arrive).

use crate::{BigDecimal, EngineError};

/// `^` and `pow()`: exact for integer exponents, f64-domain otherwise.
pub(crate) fn pow(base: &BigDecimal, exponent: &BigDecimal) -> Result<BigDecimal, EngineError> {
    if let Some(n) = exponent.int_value() {
        return base.power(n);
    }
    if base.is_negative() {
        return Err(EngineError::domain(
            "negative base with non-integer exponent",
        ));
    }
    let result = libm::pow(base.to_f64(), exponent.to_f64());
    BigDecimal::from_f64(result).ok_or_else(|| EngineError::domain("pow result out of range"))
}
