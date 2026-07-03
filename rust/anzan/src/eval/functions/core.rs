//! Port of the core function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.

use crate::eval::evaluator::require_int;
use crate::eval::registry::{Applier, Arity, BuiltinFunction, FunctionCategory, Implementation};
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};
use num_bigint::{BigInt, Sign};
use num_integer::Integer;

/// Builds a numeric registry entry — the Rust spelling of the Swift `fn`
/// helper. Shared with the other category lists.
pub(crate) fn numeric(
    name: &'static str,
    category: FunctionCategory,
    arity: Arity,
    signature: &'static str,
    summary: &'static str,
    examples: &'static [&'static str],
    apply: fn(&[BigDecimal]) -> Result<BigDecimal, EngineError>,
) -> BuiltinFunction {
    BuiltinFunction {
        name,
        category,
        signature,
        summary,
        examples,
        arity,
        implementation: Implementation::Numeric(apply),
    }
}

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        numeric(
            "abs",
            FunctionCategory::Core,
            1..=1,
            "abs(x)",
            "Absolute value of x.",
            &["abs(-5)", "abs(3.2)"],
            abs,
        ),
        numeric(
            "min",
            FunctionCategory::Core,
            1..=usize::MAX,
            "min(x, y, …)",
            "Smallest of the arguments. Accepts cell ranges.",
            &["min(3, 1, 2)", "min(B:1..B:3)"],
            min,
        ),
        numeric(
            "max",
            FunctionCategory::Core,
            1..=usize::MAX,
            "max(x, y, …)",
            "Largest of the arguments. Accepts cell ranges.",
            &["max(3, 1, 2)", "max(B:1..B:3)"],
            max,
        ),
        numeric(
            "round",
            FunctionCategory::Core,
            1..=2,
            "round(x, places = 0)",
            "Rounds x to a number of decimal places (banker's rounding: halves go to the even neighbor). Negative places round left of the decimal point.",
            &["round(2.567, 2)", "round(2.5)", "round(1234, -2)"],
            round,
        ),
        numeric(
            "floor",
            FunctionCategory::Core,
            1..=1,
            "floor(x)",
            "Largest integer ≤ x.",
            &["floor(2.9)", "floor(-1.5)"],
            floor,
        ),
        numeric(
            "ceil",
            FunctionCategory::Core,
            1..=1,
            "ceil(x)",
            "Smallest integer ≥ x.",
            &["ceil(2.1)", "ceil(-1.5)"],
            ceil,
        ),
        numeric(
            "trunc",
            FunctionCategory::Core,
            1..=1,
            "trunc(x)",
            "Drops the fractional part (rounds toward zero).",
            &["trunc(2.9)", "trunc(-1.5)"],
            trunc,
        ),
        numeric(
            "sqrt",
            FunctionCategory::Core,
            1..=1,
            "sqrt(x)",
            "Square root, exact to 50 significant digits. √x is the same thing.",
            &["sqrt(16)", "√2"],
            sqrt,
        ),
        numeric(
            "cbrt",
            FunctionCategory::Core,
            1..=1,
            "cbrt(x)",
            "Cube root. Works for negative numbers.",
            &["cbrt(27)", "cbrt(-8)"],
            cbrt,
        ),
        numeric(
            "root",
            FunctionCategory::Core,
            2..=2,
            "root(x, n)",
            "nth root of x. Odd roots of negatives are real.",
            &["root(32, 5)", "root(-27, 3)"],
            root,
        ),
        numeric(
            "pow",
            FunctionCategory::Core,
            2..=2,
            "pow(x, y)",
            "x raised to y. Exact for integer exponents. Equivalent to the ^ operator — except in Programmer mode, where ^ is XOR, so pow is how you write a power there.",
            &["pow(2, 10)", "pow(4, 0.5)"],
            pow,
        ),
        numeric(
            "mod",
            FunctionCategory::Core,
            2..=2,
            "mod(x, y)",
            "Remainder of x ÷ y, with the sign of x (exact). In the default dialect modulo is this function and the postfix % means percent (3% → 0.03); in Programmer mode the `%` operator is modulo (a % b). See man modes.",
            &["mod(10, 3)", "mod(-7, 3)"],
            modulo,
        ),
        numeric(
            "fact",
            FunctionCategory::Core,
            1..=1,
            "fact(n)",
            "Factorial of a non-negative integer, computed exactly.",
            &["fact(5)", "fact(20)"],
            fact,
        ),
        numeric(
            "choose",
            FunctionCategory::Core,
            2..=2,
            "choose(n, k)",
            "Binomial coefficient — n choose k, computed exactly (choose(100, 50) keeps all 30 digits). k > n is 0.",
            &["choose(5, 2)", "choose(52, 5)"],
            choose,
        ),
        numeric(
            "perm",
            FunctionCategory::Core,
            2..=2,
            "perm(n, k)",
            "Permutations — ordered selections of k from n, computed exactly. k > n is 0.",
            &["perm(5, 2)", "perm(10, 10)"],
            perm,
        ),
        numeric(
            "gcd",
            FunctionCategory::Core,
            2..=usize::MAX,
            "gcd(a, b, …)",
            "Greatest common divisor of integers.",
            &["gcd(12, 18)", "gcd(12, 18, 24)"],
            gcd,
        ),
        numeric(
            "lcm",
            FunctionCategory::Core,
            2..=usize::MAX,
            "lcm(a, b, …)",
            "Least common multiple of integers.",
            &["lcm(4, 6)", "lcm(2, 3, 5)"],
            lcm,
        ),
        numeric(
            "percent",
            FunctionCategory::Core,
            1..=1,
            "percent(x)",
            "x divided by 100 — handy for rates: tax = percent(8.25).",
            &["percent(8.25)", "200 * percent(15)"],
            percent,
        ),
        numeric(
            "not",
            FunctionCategory::Logic,
            1..=1,
            "not(x)",
            "Logical negation: 1 when x is 0, otherwise 0.",
            &["not(0)", "not(5)"],
            not,
        ),
        numeric(
            "and",
            FunctionCategory::Logic,
            2..=usize::MAX,
            "and(a, b, …)",
            "1 when every argument is nonzero. Use for combined conditions — comparisons can't chain.",
            &["and(1 < 2, 2 < 3)", "and(1, 0)"],
            and,
        ),
        numeric(
            "or",
            FunctionCategory::Logic,
            2..=usize::MAX,
            "or(a, b, …)",
            "1 when any argument is nonzero.",
            &["or(0, 0, 4)", "or(1 > 2, 2 > 1)"],
            or,
        ),
        numeric(
            "exp",
            FunctionCategory::Core,
            1..=1,
            "exp(x)",
            "e raised to x (≈15 significant digits).",
            &["exp(0)", "exp(1)"],
            exp,
        ),
        numeric(
            "ln",
            FunctionCategory::Core,
            1..=1,
            "ln(x)",
            "Natural logarithm (base e, ≈15 significant digits).",
            &["ln(e)", "ln(10)"],
            ln,
        ),
        numeric(
            "log10",
            FunctionCategory::Core,
            1..=1,
            "log10(x)",
            "Base-10 logarithm.",
            &["log10(1000)", "log10(2)"],
            log10,
        ),
        numeric(
            "log",
            FunctionCategory::Core,
            2..=2,
            "log(base, x)",
            "Logarithm of x in an arbitrary base.",
            &["log(2, 8)", "log(5, 125)"],
            log,
        ),
        // Goal seek as a formula: find x with f(x) = target. Newton with a
        // numeric derivative from the guess, expanding-bracket bisection as
        // the fallback — the same regime as rate()/irr(), and like them it
        // works in the f64 domain (~15 significant digits).
        BuiltinFunction {
            name: "solve",
            category: FunctionCategory::Core,
            signature: "solve(f, target = 0, guess = 1)",
            summary: "Finds x where f(x) = target, numerically (Newton + bisection, ~15 significant digits). Pass a lambda or function name: solve(x -> x^2, 2) is √2; solve(r -> npv(r, …), 0, 0.1) is goal seek.",
            examples: &["solve(x -> x^2, 2)", "solve(cos, 0, 1)"],
            arity: 1..=3,
            implementation: Implementation::HigherOrder(solve),
        },
        // The spreadsheet's #REF! adapted: deleting a referenced row/column
        // splices this call over the dead reference, so the formula errors
        // loudly instead of silently reading shifted neighbors. Registry slot
        // justified as arrival vocabulary — the name appears in rewritten
        // formulas, so man(refError) must answer for it.
        numeric(
            "refError",
            FunctionCategory::Core,
            0..=0,
            "refError()",
            "Always errors: marks a reference whose row or column was deleted. Replace it with the cell you meant.",
            &["if(true, 42, refError())"],
            ref_error,
        ),
    ]
}

// MARK: - Implementations

fn abs(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args[0].is_negative() {
        -&args[0]
    } else {
        args[0].clone()
    })
}

fn min(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().min().cloned().expect("arity checked"))
}

fn max(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().max().cloned().expect("arity checked"))
}

fn round(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let places = if args.len() == 2 {
        require_int(&args[1], "round places")?
    } else {
        0
    };
    Ok(args[0].rounded_to_places(places))
}

fn floor(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::Down))
}

fn ceil(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::Up))
}

fn trunc(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(rounded_directional(&args[0], Direction::TowardZero))
}

fn sqrt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].square_root()
}

fn cbrt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    nth_root(&args[0], 3)
}

fn root(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    nth_root(&args[0], require_int(&args[1], "root degree")?)
}

fn pow(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    crate::eval::numeric::pow(&args[0], &args[1])
}

fn modulo(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].rem(&args[1])
}

fn fact(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    factorial(&args[0])
}

fn choose(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    combinations(
        require_int(&args[0], "choose n")?,
        require_int(&args[1], "choose k")?,
    )
}

fn perm(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    permutations(
        require_int(&args[0], "perm n")?,
        require_int(&args[1], "perm k")?,
    )
}

fn gcd(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut acc: i64 = 0;
    for arg in args {
        let n = require_int(arg, "gcd")?;
        acc = gcd_i64(acc.abs(), n.abs());
    }
    Ok(BigDecimal::from_int(acc))
}

fn lcm(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
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

fn percent(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    args[0].div(&BigDecimal::from_int(100))
}

fn not(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args[0].is_zero() {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

fn and(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args.iter().all(|a| !a.is_zero()) {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

fn or(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(if args.iter().any(|a| !a.is_zero()) {
        BigDecimal::one()
    } else {
        BigDecimal::zero()
    })
}

fn exp(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    BigDecimal::via_double("exp", &args[0], libm::exp)
}

fn ln(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    logarithm("ln", &args[0], libm::log)
}

fn log10(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    logarithm("log10", &args[0], libm::log10)
}

fn log(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
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

fn solve(arguments: &[Value], applier: Applier<'_>) -> Result<Value, EngineError> {
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

fn ref_error(_args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
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
