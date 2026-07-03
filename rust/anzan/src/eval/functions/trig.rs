//! Port of the trig function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.

use super::core::numeric;
use crate::eval::environment::constants;
use crate::eval::registry::{BuiltinFunction, FunctionCategory};
use crate::{BigDecimal, EngineError};

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        numeric(
            "sin",
            FunctionCategory::Trig,
            1..=1,
            "sin(x)",
            "Sine of x (radians).",
            &["sin(0)", "sin(pi / 2)"],
            sin,
        ),
        numeric(
            "cos",
            FunctionCategory::Trig,
            1..=1,
            "cos(x)",
            "Cosine of x (radians).",
            &["cos(0)", "cos(pi)"],
            cos,
        ),
        numeric(
            "tan",
            FunctionCategory::Trig,
            1..=1,
            "tan(x)",
            "Tangent of x (radians).",
            &["tan(0)", "tan(pi / 4)"],
            tan,
        ),
        numeric(
            "asin",
            FunctionCategory::Trig,
            1..=1,
            "asin(x)",
            "Inverse sine, in radians. x must be in [-1, 1].",
            &["asin(1)", "asin(0.5)"],
            asin,
        ),
        numeric(
            "acos",
            FunctionCategory::Trig,
            1..=1,
            "acos(x)",
            "Inverse cosine, in radians. x must be in [-1, 1].",
            &["acos(1)", "acos(0)"],
            acos,
        ),
        numeric(
            "atan",
            FunctionCategory::Trig,
            1..=1,
            "atan(x)",
            "Inverse tangent, in radians.",
            &["atan(1)", "atan(0)"],
            atan,
        ),
        numeric(
            "atan2",
            FunctionCategory::Trig,
            2..=2,
            "atan2(y, x)",
            "Angle of the point (x, y), in radians — the quadrant-aware inverse tangent.",
            &["atan2(1, 1)", "atan2(1, 0)"],
            atan2,
        ),
        numeric(
            "sinh",
            FunctionCategory::Trig,
            1..=1,
            "sinh(x)",
            "Hyperbolic sine.",
            &["sinh(0)", "sinh(1)"],
            sinh,
        ),
        numeric(
            "cosh",
            FunctionCategory::Trig,
            1..=1,
            "cosh(x)",
            "Hyperbolic cosine.",
            &["cosh(0)", "cosh(1)"],
            cosh,
        ),
        numeric(
            "tanh",
            FunctionCategory::Trig,
            1..=1,
            "tanh(x)",
            "Hyperbolic tangent.",
            &["tanh(0)", "tanh(1)"],
            tanh,
        ),
        numeric(
            "asinh",
            FunctionCategory::Trig,
            1..=1,
            "asinh(x)",
            "Inverse hyperbolic sine.",
            &["asinh(0)", "asinh(1)"],
            asinh,
        ),
        numeric(
            "acosh",
            FunctionCategory::Trig,
            1..=1,
            "acosh(x)",
            "Inverse hyperbolic cosine. x must be ≥ 1.",
            &["acosh(1)", "acosh(2)"],
            acosh,
        ),
        numeric(
            "atanh",
            FunctionCategory::Trig,
            1..=1,
            "atanh(x)",
            "Inverse hyperbolic tangent. |x| must be < 1.",
            &["atanh(0)", "atanh(0.5)"],
            atanh,
        ),
        // Pure BigDecimal — π is the 60-digit constant, so deg(pi) is exactly
        // 180 and rad survives round-trips at full working precision.
        numeric(
            "deg",
            FunctionCategory::Trig,
            1..=1,
            "deg(x)",
            "Radians → degrees, at full precision (deg(pi) is exactly 180).",
            &["deg(pi)", "deg(pi / 4)"],
            deg,
        ),
        numeric(
            "rad",
            FunctionCategory::Trig,
            1..=1,
            "rad(x)",
            "Degrees → radians, at full precision.",
            &["rad(180)", "sin(rad(90))"],
            rad,
        ),
    ]
}

// MARK: - Implementations

fn sin(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("sin", &args[0], libm::sin)
}

fn cos(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("cos", &args[0], libm::cos)
}

fn tan(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("tan", &args[0], libm::tan)
}

fn asin(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("asin", &args[0], libm::asin)
}

fn acos(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("acos", &args[0], libm::acos)
}

fn atan(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("atan", &args[0], libm::atan)
}

fn atan2(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let result = libm::atan2(args[0].to_f64(), args[1].to_f64());
    BigDecimal::from_f64(result)
        .ok_or_else(|| EngineError::domain("atan2 is undefined for these arguments"))
}

fn sinh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("sinh", &args[0], libm::sinh)
}

fn cosh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("cosh", &args[0], libm::cosh)
}

fn tanh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("tanh", &args[0], libm::tanh)
}

fn asinh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("asinh", &args[0], libm::asinh)
}

fn acosh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("acosh", &args[0], libm::acosh)
}

fn atanh(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("atanh", &args[0], libm::atanh)
}

fn deg(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    (&args[0] * &BigDecimal::from_int(180)).div(&constants::pi())
}

fn rad(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    (&args[0] * &constants::pi()).div(&BigDecimal::from_int(180))
}
