//! The bodies behind the core/logic registry entries, plus the exact
//! combinatorial helpers and directional rounding they lean on.

use crate::eval::evaluator::require_int;
use crate::eval::registry::Applier;
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};
use num_bigint::{BigInt, Sign};
use num_integer::Integer;

// MARK: - Implementations

pub(super) fn abs(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args[0].is_negative() {
        -&args[0]
    } else {
        args[0].clone()
    })
}

pub(super) fn min(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().min().cloned().expect("arity checked"))
}

pub(super) fn max(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().max().cloned().expect("arity checked"))
}

pub(super) fn round(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let places = if args.len() == 2 {
        require_int(&args[1], "round places")?
    } else {
        0
    };
    Ok(args[0].rounded_to_places(places))
}

pub(super) fn floor(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::Down))
}

pub(super) fn ceil(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::Up))
}

pub(super) fn trunc(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::TowardZero))
}

pub(super) fn sqrt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].square_root()
}

pub(super) fn cbrt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    nth_root(&args[0], 3)
}

pub(super) fn root(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    nth_root(&args[0], require_int(&args[1], "root degree")?)
}

pub(super) fn pow(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    crate::eval::numeric::pow(&args[0], &args[1])
}

pub(super) fn modulo(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].rem(&args[1])
}

pub(super) fn fact(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    factorial(&args[0])
}

pub(super) fn choose(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    combinations(
        require_int(&args[0], "choose n")?,
        require_int(&args[1], "choose k")?,
    )
}

pub(super) fn perm(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    permutations(
        require_int(&args[0], "perm n")?,
        require_int(&args[1], "perm k")?,
    )
}

pub(super) fn gcd(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut acc: i64 = 0;
    for arg in args {
        let n = require_int(arg, "gcd")?;
        acc = gcd_i64(acc.abs(), n.abs());
    }
    Ok(BigDecimal::from_int(acc))
}

pub(super) fn lcm(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut acc: i64 = 1;
    for arg in args {
        let b = require_int(arg, "lcm")?;
        if acc == 0 && b == 0 {
            acc = 0;
            continue;
        }
        let g = gcd_i64(acc.abs(), b.abs());
        if g == 0 {
            acc = 0;
            continue;
        }
        let result = (acc / g)
            .checked_mul(b)
            .ok_or_else(|| EngineError::domain("lcm overflow"))?;
        acc = result.abs();
    }
    Ok(BigDecimal::from_int(acc))
}

pub(super) fn percent(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].div(&BigDecimal::from_int(100))
}

pub(super) fn not(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args[0].is_zero() {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

pub(super) fn and(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args.iter().all(|a| !a.is_zero()) {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

pub(super) fn or(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args.iter().any(|a| !a.is_zero()) {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

pub(super) fn exp(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("exp", &args[0], libm::exp)
}

pub(super) fn ln(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    logarithm("ln", &args[0], libm::log)
}

pub(super) fn log10(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    logarithm("log10", &args[0], libm::log10)
}

pub(super) fn log(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    // The ratio is taken in the f64 domain so clean cases like log(2, 8)
    // come out exact instead of 2.999…97.
    for arg in args {
        if arg.is_negative() || arg.is_zero() {
            return Err(EngineError::domain("log needs positive arguments"));
        }
    }
    let result = libm::log(args[1].to_f64()) / libm::log(args[0].to_f64());
    BigDecimal::from_f64(result)
        .ok_or_else(|| EngineError::domain("log is undefined for these arguments"))
}

pub(super) fn solve(arguments: &[Value], applier: Applier<'_>) -> Result<Value, EngineError> {
    let target = if arguments.len() > 1 {
        arguments[1].as_number("solve's target")?.to_f64()
    } else {
        0.0
    };
    let guess = if arguments.len() > 2 {
        arguments[2].as_number("solve's guess")?.to_f64()
    } else {
        1.0
    };

    let mut g = |x: f64| -> Result<f64, EngineError> {
        let input = BigDecimal::from_f64(x)
            .ok_or_else(|| EngineError::domain("solve() left the number line"))?;
        Ok(applier(&arguments[0], &[Value::Number(input)])?
            .as_number("f's result in solve()")?
            .to_f64()
            - target)
    };

    let tolerance = 1e-12;

    // Newton from the guess.
    let mut x = guess;
    for _ in 0..60 {
        let value = g(x)?;
        if value.abs() < tolerance {
            if let Some(result) = BigDecimal::from_f64(x) {
                return Ok(Value::Number(result));
            }
        }
        let h = x.abs().max(1e-4) * 1e-7;
        let slope = (g(x + h)? - g(x - h)?) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            break;
        }
        let next = x - value / slope;
        if !next.is_finite() {
            break;
        }
        x = next;
    }

    // Bisection over an expanding bracket around the guess.
    let mut radius = guess.abs().max(1.0);
    while radius <= 1e9 {
        let (lo, hi) = (guess - radius, guess + radius);
        let (f_lo, f_hi) = (g(lo)?, g(hi)?);
        if f_lo.is_finite()
            && f_hi.is_finite()
            && f_lo.is_sign_positive() != f_hi.is_sign_positive()
        {
            let (mut a, mut b, mut f_a) = (lo, hi, f_lo);
            for _ in 0..200 {
                let mid = (a + b) / 2.0;
                let f_mid = g(mid)?;
                if f_mid.abs() < tolerance || (b - a) / 2.0 < 1e-15 {
                    let Some(result) = BigDecimal::from_f64(mid) else {
                        break;
                    };
                    return Ok(Value::Number(result));
                }
                if f_mid.is_sign_positive() == f_a.is_sign_positive() {
                    a = mid;
                    f_a = f_mid;
                } else {
                    b = mid;
                }
            }
            break;
        }
        radius *= 4.0;
    }
    Err(EngineError::domain(
        "solve() did not converge — try a different guess",
    ))
}

pub(super) fn ref_error(_args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Err(EngineError::domain("refers to a deleted cell"))
}

// MARK: - Helpers

fn factorial(value: &BigDecimal) -> Result<BigDecimal, EngineError> {
    let n = match value.int_value() {
        Some(n) if n >= 0 => n,
        _ => return Err(EngineError::domain("fact() needs a non-negative integer")),
    };
    if n > 10_000 {
        return Err(EngineError::domain(format!("fact({n}) is too large")));
    }
    let mut result = BigInt::from(1);
    if n >= 2 {
        for i in 2..=n {
            result *= BigInt::from(i);
        }
    }
    Ok(BigDecimal::new(result, 0))
}

/// n choose k, exactly — every intermediate division in the multiplicative
/// formula is itself exact, so the BigInt never carries a remainder. k > n is
/// 0 ways (the combinatorial convention).
fn combinations(n: i64, k: i64) -> Result<BigDecimal, EngineError> {
    if n < 0 || k < 0 {
        return Err(EngineError::domain("choose() needs non-negative integers"));
    }
    if k > n {
        return Ok(BigDecimal::zero());
    }
    let smaller = k.min(n - k);
    if smaller > 10_000 {
        return Err(EngineError::domain(format!(
            "choose({n}, {k}) is too large"
        )));
    }
    let mut result = BigInt::from(1);
    for i in 0..smaller {
        result = result * BigInt::from(n - i) / BigInt::from(i + 1);
    }
    Ok(BigDecimal::new(result, 0))
}

/// Ordered selections of k from n — the falling factorial, exact.
fn permutations(n: i64, k: i64) -> Result<BigDecimal, EngineError> {
    if n < 0 || k < 0 {
        return Err(EngineError::domain("perm() needs non-negative integers"));
    }
    if k > n {
        return Ok(BigDecimal::zero());
    }
    if k > 10_000 {
        return Err(EngineError::domain(format!("perm({n}, {k}) is too large")));
    }
    let mut result = BigInt::from(1);
    if k > 0 {
        for i in (n - k + 1)..=n.max(1) {
            result *= BigInt::from(i);
        }
    }
    Ok(BigDecimal::new(result, 0))
}

/// Internal, not private: geomean (the stats list) shares it.
pub(crate) fn nth_root(value: &BigDecimal, degree: i64) -> Result<BigDecimal, EngineError> {
    if degree <= 0 {
        return Err(EngineError::domain("root degree must be positive"));
    }
    if degree == 1 {
        return Ok(value.clone());
    }
    if degree == 2 {
        return value.square_root();
    }
    if value.is_negative() {
        // Odd roots of negatives are real.
        if degree % 2 != 1 {
            return Err(EngineError::domain("even root of a negative number"));
        }
        return Ok(-&nth_root(&-value, degree)?);
    }
    BigDecimal::via_double("root", value, |x| libm::pow(x, 1.0 / degree as f64))
}

fn logarithm(
    name: &str,
    value: &BigDecimal,
    f: impl FnOnce(f64) -> f64,
) -> Result<BigDecimal, EngineError> {
    if value.is_negative() || value.is_zero() {
        return Err(EngineError::domain(format!(
            "{name} needs a positive argument"
        )));
    }
    BigDecimal::via_double(name, value, f)
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

// MARK: - Directional rounding (the Swift BigDecimal.rounded(_ direction:))

enum Direction {
    Down,
    Up,
    TowardZero,
}

/// floor/ceil/trunc to an integer.
fn rounded_directional(value: &BigDecimal, direction: Direction) -> BigDecimal {
    if value.is_integer() {
        return value.clone();
    }
    let scale = BigInt::from(10).pow((-value.exponent()) as u32);
    let (q, r) = value.significand().div_rem(&scale);
    let mut result = q;
    match direction {
        Direction::TowardZero => {}
        Direction::Down => {
            if r.sign() == Sign::Minus {
                result -= BigInt::from(1);
            }
        }
        Direction::Up => {
            if r.sign() == Sign::Plus {
                result += BigInt::from(1);
            }
        }
    }
    BigDecimal::new(result, 0)
}
