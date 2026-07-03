//! Port of the programmer function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Base conversion and bitwise operations — for the CLI crowd. Everything
//! here is exact: bases ride BigInt (no 2^53 ceiling like a float-backed
//! HEX2DEC), and the bit operations work at arbitrary width.

use crate::eval::evaluator::require_int;
use crate::eval::fixed_int::FixedInt;
use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;
use num_traits::Signed;

/// A per-width fixed integer constructor — `Int32(x)` ≡ `Int(x, 32)`.
/// Implementations are plain `fn` pointers, so the macro stamps out one
/// named fn per width/signedness; the width set must mirror
/// `fixed_int::ALLOWED_WIDTHS` (8/16/32/64/128/256).
macro_rules! width_constructor {
    ($impl_name:ident, $name:literal, $base:literal, $signed_word:literal, $bits:literal, $signed:literal) => {{
        fn $impl_name(args: &[Value]) -> Result<Value, EngineError> {
            let number = args[0].as_number($name)?;
            let Some(value) = number.big_int_value() else {
                return Err(EngineError::domain(format!(
                    concat!($name, "() needs an integer value, got {}"),
                    number
                )));
            };
            Ok(Value::FixedInt(FixedInt::new(value, $bits, $signed)?))
        }
        BuiltinFunction {
            name: $name,
            category: FunctionCategory::Programmer,
            signature: concat!($name, "(value)"),
            summary: concat!(
                "A ",
                $signed_word,
                " ",
                $bits,
                "-bit fixed-width integer — ",
                $name,
                "(x) ≡ ",
                $base,
                "(x, ",
                $bits,
                "). Checked: overflow is an error, never a wraparound."
            ),
            examples: &[concat!($name, "(100)")],
            arity: 1..=1,
            implementation: Implementation::Values($impl_name),
        }
    }};
}

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        BuiltinFunction {
            name: "toBase",
            category: FunctionCategory::Programmer,
            signature: "toBase(n, base)",
            summary: "An integer rendered in another base (2–36), as a string — hex is toBase(n, 16), binary toBase(n, 2); uppercase digits, exact at any size. Want a toHex? Define it: toHex(n) = toBase(n, 16).",
            examples: &["toBase(255, 16)", "toBase(10, 2)"],
            arity: 2..=2,
            implementation: Implementation::Values(to_base_impl),
        },
        BuiltinFunction {
            name: "fromBase",
            category: FunctionCategory::Programmer,
            signature: "fromBase(text, base)",
            summary: "Parses digits in another base (2–36) into a decimal number — hex to decimal is fromBase(\"ff\", 16) → 255, binary fromBase(\"1010\", 2) → 10. toBase's inverse, exact at any size.",
            examples: &["fromBase(\"ff\", 16)", "fromBase(\"1010\", 2)"],
            arity: 2..=2,
            implementation: Implementation::Values(from_base_impl),
        },
        BuiltinFunction {
            name: "Int",
            category: FunctionCategory::Programmer,
            signature: "Int(value, bits)",
            summary: "A signed fixed-width integer of `bits` bits (8/16/32/64/128/256). Checked, not modular: arithmetic that overflows the range is an error, never a wraparound. The per-width forms Int8…Int256 take just the value: Int32(x) ≡ Int(x, 32).",
            examples: &["Int(27374, 32)", "Int(-1, 8)"],
            arity: 2..=2,
            implementation: Implementation::Values(int_impl),
        },
        BuiltinFunction {
            name: "UInt",
            category: FunctionCategory::Programmer,
            signature: "UInt(value, bits)",
            summary: "An unsigned fixed-width integer of `bits` bits (8/16/32/64/128/256). Checked: overflow or a negative value is an error, never a wraparound. Per-width forms: UInt8…UInt256, e.g. UInt8(x) ≡ UInt(x, 8).",
            examples: &["UInt(255, 8)", "UInt(1000, 16)"],
            arity: 2..=2,
            implementation: Implementation::Values(uint_impl),
        },
        BuiltinFunction {
            name: "bitAnd",
            category: FunctionCategory::Programmer,
            signature: "bitAnd(a, b, …)",
            summary: "Bitwise AND of non-negative integers — at any width. In Programmer mode, the `&` operator (a & b).",
            examples: &["bitAnd(12, 10)", "bitAnd(255, 51, 15)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Values(bit_and_impl),
        },
        BuiltinFunction {
            name: "bitOr",
            category: FunctionCategory::Programmer,
            signature: "bitOr(a, b, …)",
            summary: "Bitwise OR of non-negative integers. In Programmer mode, the `|` operator (a | b).",
            examples: &["bitOr(12, 10)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Values(bit_or_impl),
        },
        BuiltinFunction {
            name: "bitXor",
            category: FunctionCategory::Programmer,
            signature: "bitXor(a, b, …)",
            summary: "Bitwise XOR of non-negative integers. In Programmer mode, the `^` operator (a ^ b) — there ^ is XOR, not power.",
            examples: &["bitXor(12, 10)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Values(bit_xor_impl),
        },
        BuiltinFunction {
            name: "bitNot",
            category: FunctionCategory::Programmer,
            signature: "bitNot(x)",
            summary: "Bitwise NOT of a fixed-width integer, two's-complement over its width — bitNot(UInt(0, 8)) is UInt(255, 8), bitNot(Int(0, 8)) is Int(-1, 8). Needs a width, so it's defined on Int()/UInt() values (also the ~ operator in Programmer mode).",
            examples: &["bitNot(UInt8(0))", "bitNot(Int8(0))"],
            arity: 1..=1,
            implementation: Implementation::Values(bit_not_impl),
        },
        BuiltinFunction {
            name: "bitShift",
            category: FunctionCategory::Programmer,
            signature: "bitShift(n, by)",
            summary: "Shifts left (positive `by`) or right (negative). A plain integer shifts exactly at any width (bitShift(1, 100) is the full power of two); a fixed-width int is checked — a left shift whose bits leave the width is an overflow error. In Programmer mode, the `<<` / `>>` operators (a << n, a >> n).",
            examples: &["bitShift(1, 8)", "bitShift(256, -4)"],
            arity: 2..=2,
            implementation: Implementation::Values(bit_shift_impl),
        },
        // Per-width fixed integer constructors — Int8…Int256 / UInt8…UInt256.
        width_constructor!(int8_impl, "Int8", "Int", "signed", 8, true),
        width_constructor!(uint8_impl, "UInt8", "UInt", "unsigned", 8, false),
        width_constructor!(int16_impl, "Int16", "Int", "signed", 16, true),
        width_constructor!(uint16_impl, "UInt16", "UInt", "unsigned", 16, false),
        width_constructor!(int32_impl, "Int32", "Int", "signed", 32, true),
        width_constructor!(uint32_impl, "UInt32", "UInt", "unsigned", 32, false),
        width_constructor!(int64_impl, "Int64", "Int", "signed", 64, true),
        width_constructor!(uint64_impl, "UInt64", "UInt", "unsigned", 64, false),
        width_constructor!(int128_impl, "Int128", "Int", "signed", 128, true),
        width_constructor!(uint128_impl, "UInt128", "UInt", "unsigned", 128, false),
        width_constructor!(int256_impl, "Int256", "Int", "signed", 256, true),
        width_constructor!(uint256_impl, "UInt256", "UInt", "unsigned", 256, false),
    ]
}

// MARK: - Implementations

fn to_base_impl(args: &[Value]) -> Result<Value, EngineError> {
    let n = args[0].as_number("toBase's number")?;
    let base = require_int(&args[1].as_number("toBase's base")?, "toBase base")?;
    if !(2..=36).contains(&base) {
        return Err(EngineError::domain("toBase's base must be 2–36"));
    }
    let Some(value) = n.big_int_value() else {
        return Err(EngineError::domain("toBase needs an integer"));
    };
    Ok(Value::String(
        value.to_str_radix(base as u32).to_uppercase(),
    ))
}

fn from_base_impl(args: &[Value]) -> Result<Value, EngineError> {
    let Value::String(text) = &args[0] else {
        return Err(EngineError::domain(format!(
            "fromBase wants digits as a string, got {}",
            args[0].kind_name()
        )));
    };
    let base = require_int(&args[1].as_number("fromBase's base")?, "fromBase base")?;
    if !(2..=36).contains(&base) {
        return Err(EngineError::domain("fromBase's base must be 2–36"));
    }
    // Hand-rolled digit walk (like the Swift original): we want a typed
    // error for bad digits, not a parser's None.
    let mut digits = text.as_str();
    let negative = digits.starts_with('-');
    if negative {
        digits = &digits[1..];
    }
    if digits.is_empty() {
        return Err(EngineError::domain("fromBase needs at least one digit"));
    }
    let mut value = BigInt::from(0);
    for character in digits.chars() {
        let digit = character.to_digit(36).map(i64::from).filter(|d| *d < base);
        let Some(digit) = digit else {
            return Err(EngineError::domain(format!(
                "\"{text}\" is not a base-{base} number"
            )));
        };
        value = value * BigInt::from(base) + BigInt::from(digit);
    }
    Ok(Value::Number(BigDecimal::new(
        if negative { -value } else { value },
        0,
    )))
}

fn int_impl(args: &[Value]) -> Result<Value, EngineError> {
    make_fixed_int(args, true)
}

fn uint_impl(args: &[Value]) -> Result<Value, EngineError> {
    make_fixed_int(args, false)
}

fn bit_and_impl(args: &[Value]) -> Result<Value, EngineError> {
    bit_reduce(args, "bitAnd", |a, b| a & b)
}

fn bit_or_impl(args: &[Value]) -> Result<Value, EngineError> {
    bit_reduce(args, "bitOr", |a, b| a | b)
}

fn bit_xor_impl(args: &[Value]) -> Result<Value, EngineError> {
    bit_reduce(args, "bitXor", |a, b| a ^ b)
}

fn bit_not_impl(args: &[Value]) -> Result<Value, EngineError> {
    let Value::FixedInt(fixed) = &args[0] else {
        return Err(EngineError::domain(
            "bitNot needs a fixed-width integer (Int()/UInt()) — its width defines the complement",
        ));
    };
    Ok(Value::FixedInt(fixed.bitwise_not()?))
}

fn bit_shift_impl(args: &[Value]) -> Result<Value, EngineError> {
    let by = require_int(&args[1].as_number("bitShift amount")?, "bitShift amount")?;
    if by.abs() > 10_000 {
        return Err(EngineError::domain("bitShift amount is too large"));
    }
    // Fixed-width: shift within the width, range-checked (left overflow → error).
    if let Value::FixedInt(fixed) = &args[0] {
        let shifted = if by >= 0 {
            &fixed.value << (by as usize)
        } else {
            &fixed.value >> ((-by) as usize)
        };
        return Ok(Value::FixedInt(FixedInt::new(
            shifted,
            fixed.bits,
            fixed.signed,
        )?));
    }
    let n = require_bits(&args[0].as_number("bitShift")?, "bitShift")?;
    let result = if by >= 0 {
        n << (by as usize)
    } else {
        n >> ((-by) as usize)
    };
    Ok(Value::Number(BigDecimal::new(result, 0)))
}

// MARK: - Helpers

/// A variadic bitwise reduction. Plain numbers reduce over non-negative
/// BigInts (exact, any width). When any operand is a fixed-width int the
/// reduction is type-preserving — two's-complement over the (promoted)
/// width, signs must match (docs/FIXED-WIDTH.md).
fn bit_reduce(
    raw_arguments: &[Value],
    name: &str,
    op: fn(BigInt, BigInt) -> BigInt,
) -> Result<Value, EngineError> {
    let arguments = flatten_bitwise_operands(raw_arguments, name)?;
    if arguments.iter().any(|v| matches!(v, Value::FixedInt(_))) {
        let mut result = arguments[0].clone();
        for value in &arguments[1..] {
            result = FixedInt::apply_bitwise(&result, value, op)?;
        }
        return Ok(result);
    }
    let mut bits = Vec::with_capacity(arguments.len());
    for value in &arguments {
        bits.push(require_bits(&value.as_number(name)?, name)?);
    }
    let folded = bits[1..]
        .iter()
        .fold(bits[0].clone(), |a, b| op(a, b.clone()));
    Ok(Value::Number(BigDecimal::new(folded, 0)))
}

/// Flattens array arguments (preserving numbers and fixedInts) so a bitwise
/// reduction accepts `[a, b]` like a range; rejects non-integer kinds.
fn flatten_bitwise_operands(values: &[Value], name: &str) -> Result<Vec<Value>, EngineError> {
    let mut out = Vec::new();
    for value in values {
        match value {
            Value::Array(items) => out.extend(flatten_bitwise_operands(items, name)?),
            Value::Number(_) | Value::FixedInt(_) => out.push(value.clone()),
            _ => {
                return Err(EngineError::domain(format!(
                    "{name}() works on integers, got {}",
                    value.kind_name()
                )))
            }
        }
    }
    Ok(out)
}

/// Builds a `Value::FixedInt` for the `Int`/`UInt` constructors — integer
/// value, allowed width, in range (all enforced by `FixedInt::new`).
fn make_fixed_int(args: &[Value], signed: bool) -> Result<Value, EngineError> {
    let name = if signed { "Int" } else { "UInt" };
    let number = args[0].as_number(&format!("{name}'s value"))?;
    let Some(value) = number.big_int_value() else {
        return Err(EngineError::domain(format!(
            "{name}() needs an integer value, got {number}"
        )));
    };
    let bits = require_int(
        &args[1].as_number(&format!("{name}'s bit width"))?,
        &format!("{name} bit width"),
    )?;
    // A width outside u32 can't be allowed anyway — surface FixedInt::new's
    // exact wording for it.
    let bits = u32::try_from(bits).map_err(|_| {
        EngineError::domain(format!(
            "fixed-width needs a width of 8, 16, 32, 64, 128, or 256 — got {bits}"
        ))
    })?;
    Ok(Value::FixedInt(FixedInt::new(value, bits, signed)?))
}

/// Bit operations are defined here for non-negative integers (no width to
/// two's-complement against).
fn require_bits(value: &BigDecimal, name: &str) -> Result<BigInt, EngineError> {
    match value.big_int_value() {
        Some(bits) if !bits.is_negative() => Ok(bits),
        _ => Err(EngineError::domain(format!(
            "{name} needs non-negative integers"
        ))),
    }
}
