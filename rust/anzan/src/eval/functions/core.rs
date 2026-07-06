//! Port of the core function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.

use crate::eval::registry::{Arity, BuiltinFunction, FunctionCategory, Implementation};
use crate::{BigDecimal, EngineError};

mod implementations;

pub(crate) use implementations::nth_root;
use implementations::*;

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
