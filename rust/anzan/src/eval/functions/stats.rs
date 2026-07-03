//! Port of the stats function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.

use super::core::nth_root;
use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;
use num_integer::Integer;

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        BuiltinFunction {
            name: "sum",
            category: FunctionCategory::Stats,
            signature: "sum(x, y, …)",
            summary: "Adds the arguments. ∑(…) is the same; accepts cell ranges.",
            examples: &["sum(1, 2, 3)", "sum(B:1..B:3)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(sum),
        },
        // ∏(…) lands here, the way ∑(…) lands on sum.
        BuiltinFunction {
            name: "product",
            category: FunctionCategory::Stats,
            signature: "product(x, y, …)",
            summary: "Multiplies the arguments. ∏(…) is the same; accepts cell ranges.",
            examples: &["product(2, 3, 4)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(product),
        },
        // How many numbers — meaningful with ranges: count(A:1..A:99) skips
        // empty/text cells during expansion. Zero when a range expands empty.
        BuiltinFunction {
            name: "count",
            category: FunctionCategory::Stats,
            signature: "count(…)",
            summary: "How many numbers — over a range, empty and text cells are skipped.",
            examples: &["count(B:1..B:9)"],
            arity: 0..=usize::MAX,
            implementation: Implementation::Numeric(count),
        },
        BuiltinFunction {
            name: "avg",
            category: FunctionCategory::Stats,
            signature: "avg(x, y, …)",
            summary: "Arithmetic mean. Over a range, empty/text cells are skipped.",
            examples: &["avg(1, 2, 3, 4)", "avg(B:1..B:3)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(avg),
        },
        BuiltinFunction {
            name: "median",
            category: FunctionCategory::Stats,
            signature: "median(x, y, …)",
            summary: "Middle value (mean of the two middle values for even counts).",
            examples: &["median(5, 1, 3)", "median(4, 1, 3, 2)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(median),
        },
        // Sample standard deviation (n − 1 denominator, like spreadsheet STDEV).
        BuiltinFunction {
            name: "stdev",
            category: FunctionCategory::Stats,
            signature: "stdev(x, y, …)",
            summary: "Sample standard deviation (n − 1 denominator, like spreadsheet STDEV).",
            examples: &["stdev(2, 4, 4, 4, 5, 5, 7, 9)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(stdev),
        },
        // stdev's square — same sample convention.
        BuiltinFunction {
            name: "variance",
            category: FunctionCategory::Stats,
            signature: "variance(x, y, …)",
            summary: "Sample variance (n − 1 denominator — stdev squared).",
            examples: &["variance(2, 4, 4, 4, 5, 5, 7, 9)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(variance),
        },
        BuiltinFunction {
            name: "mode",
            category: FunctionCategory::Stats,
            signature: "mode(x, y, …)",
            summary: "The most frequent value; ties go to the first one seen. Errors when nothing repeats.",
            examples: &["mode(1, 2, 2, 3, 3, 3)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(mode),
        },
        // The trailing argument is p — the rest is the data, so a range reads
        // naturally: percentile(A:1..A:99, 0.9).
        BuiltinFunction {
            name: "percentile",
            category: FunctionCategory::Stats,
            signature: "percentile(data…, p)",
            summary: "The value below which a fraction p (0–1) of the data falls, with linear interpolation (spreadsheet PERCENTILE.INC). The LAST argument is p; everything before it is the data.",
            examples: &["percentile(1, 2, 3, 4, 0.75)", "percentile(15, 20, 35, 40, 50, 0.4)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(percentile),
        },
        BuiltinFunction {
            name: "geomean",
            category: FunctionCategory::Stats,
            signature: "geomean(x, y, …)",
            summary: "Geometric mean — the n-th root of the product; all values must be positive. The right average for growth rates.",
            examples: &["geomean(4, 9)", "geomean(2, 4, 8)"],
            arity: 1..=usize::MAX,
            implementation: Implementation::Numeric(geomean),
        },
        // Paired-series functions split their arguments evenly — the xnpv/xirr
        // convention, so two equal-length ranges read naturally.
        BuiltinFunction {
            name: "correl",
            category: FunctionCategory::Stats,
            signature: "correl(xs…, ys…)",
            summary: "Pearson correlation of two equal-length series — the x values then the y values, split evenly (pass two equal ranges).",
            examples: &["correl(1, 2, 3, 2, 4, 6)"],
            arity: 4..=usize::MAX,
            implementation: Implementation::Numeric(correl),
        },
        BuiltinFunction {
            name: "slope",
            category: FunctionCategory::Stats,
            signature: "slope(ys…, xs…)",
            summary: "Slope of the least-squares line through (x, y) points — y values first, then x values (spreadsheet argument order), split evenly.",
            examples: &["slope(2, 4, 6, 1, 2, 3)"],
            arity: 4..=usize::MAX,
            implementation: Implementation::Numeric(slope),
        },
        BuiltinFunction {
            name: "intercept",
            category: FunctionCategory::Stats,
            signature: "intercept(ys…, xs…)",
            summary: "Intercept of the least-squares line — y values first, then x values, split evenly.",
            examples: &["intercept(3, 5, 7, 1, 2, 3)"],
            arity: 4..=usize::MAX,
            implementation: Implementation::Numeric(intercept),
        },
        BuiltinFunction {
            name: "forecast",
            category: FunctionCategory::Stats,
            signature: "forecast(x, ys…, xs…)",
            summary: "Predicts y at x from the least-squares line through the data — x first, then the y values, then the x values (split evenly).",
            examples: &["forecast(4, 2, 4, 6, 1, 2, 3)"],
            arity: 5..=usize::MAX,
            implementation: Implementation::Numeric(forecast),
        },
        // Excel's classic, with both calling shapes: arrays (sumproduct(a, b))
        // or one flat even list (two equal ranges expand to exactly that).
        BuiltinFunction {
            name: "sumproduct",
            category: FunctionCategory::Stats,
            signature: "sumproduct(xs…, ys…)",
            summary: "Sum of elementwise products of two equal-length series — split evenly, so sumproduct(A:1..A:9, B:1..B:9) is the classic. Arrays work too: sumproduct(prices, quantities).",
            examples: &["sumproduct(1, 2, 3, 4, 5, 6)", "sumproduct([2, 3], [10, 100])"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(sumproduct),
        },
    ]
}

// MARK: - Implementations

fn sum(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().fold(BigDecimal::zero(), |acc, x| &acc + x))
}

fn product(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(args.iter().fold(BigDecimal::one(), |acc, x| &acc * x))
}

fn count(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(BigDecimal::from_int(args.len() as i64))
}

fn avg(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    mean(args)
}

fn median(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut sorted = args.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        return (&sorted[mid - 1] + &sorted[mid]).div(&BigDecimal::from_int(2));
    }
    Ok(sorted[mid].clone())
}

fn stdev(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    squared_deviations(args)?
        .div(&BigDecimal::from_int(args.len() as i64 - 1))?
        .square_root()
}

fn variance(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    squared_deviations(args)?.div(&BigDecimal::from_int(args.len() as i64 - 1))
}

fn mode(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut best: Option<(&BigDecimal, usize)> = None;
    for value in args {
        let count = args.iter().filter(|x| *x == value).count();
        if count > best.map_or(1, |b| b.1) {
            best = Some((value, count));
        }
    }
    match best {
        Some((value, _)) => Ok(value.clone()),
        None => Err(EngineError::domain("mode: no value repeats")),
    }
}

fn percentile(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let p = &args[args.len() - 1];
    let mut data = args[..args.len() - 1].to_vec();
    data.sort();
    if p.is_negative() || *p > BigDecimal::one() {
        return Err(EngineError::domain(
            "percentile's p must be between 0 and 1",
        ));
    }
    // rank = p(n − 1); interpolate between the straddling values.
    let rank = p * &BigDecimal::from_int(data.len() as i64 - 1);
    let lower = floored(&rank);
    let index = lower
        .int_value()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or_else(|| EngineError::domain("percentile is undefined here"))?;
    let fraction = &rank - &lower;
    if fraction.is_zero() || index + 1 >= data.len() {
        return Ok(data[index].clone());
    }
    Ok(&data[index] + &(&(&data[index + 1] - &data[index]) * &fraction))
}

fn geomean(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    for value in args {
        if value.is_zero() || value.is_negative() {
            return Err(EngineError::domain("geomean needs positive values"));
        }
    }
    let product = args.iter().fold(BigDecimal::one(), |acc, x| &acc * x);
    nth_root(&product, args.len() as i64)
}

fn correl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (xs, ys) = split_pairs(args, "correl")?;
    let (mx, my) = (mean(xs)?, mean(ys)?);
    let mut sxy = BigDecimal::zero();
    let mut sxx = BigDecimal::zero();
    let mut syy = BigDecimal::zero();
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - &mx;
        let dy = y - &my;
        sxy = &sxy + &(&dx * &dy);
        sxx = &sxx + &(&dx * &dx);
        syy = &syy + &(&dy * &dy);
    }
    if sxx.is_zero() || syy.is_zero() {
        return Err(EngineError::domain(
            "correl is undefined when a series is constant",
        ));
    }
    sxy.div(&(&sxx * &syy).square_root()?)
}

fn slope(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (ys, xs) = split_pairs(args, "slope")?;
    Ok(regression(xs, ys)?.0)
}

fn intercept(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (ys, xs) = split_pairs(args, "intercept")?;
    Ok(regression(xs, ys)?.1)
}

fn forecast(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (ys, xs) = split_pairs(&args[1..], "forecast")?;
    let (slope, intercept) = regression(xs, ys)?;
    Ok(&intercept + &(&slope * &args[0]))
}

fn sumproduct(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (xs, ys) = split_pairs(args, "sumproduct")?;
    let mut sum = BigDecimal::zero();
    for (x, y) in xs.iter().zip(ys.iter()) {
        sum = &sum + &(x * y);
    }
    Ok(sum)
}

// MARK: - Helpers

/// Splits an even-length argument list into its two series (the xnpv/xirr
/// convention for paired data: pass two equal-length ranges).
fn split_pairs<'a>(
    args: &'a [BigDecimal],
    name: &str,
) -> Result<(&'a [BigDecimal], &'a [BigDecimal]), EngineError> {
    if !args.len().is_multiple_of(2) {
        return Err(EngineError::domain(format!(
            "{name} wants two equal-length series — got {} values",
            args.len()
        )));
    }
    let half = args.len() / 2;
    Ok((&args[..half], &args[half..]))
}

fn mean(values: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    values
        .iter()
        .fold(BigDecimal::zero(), |acc, x| &acc + x)
        .div(&BigDecimal::from_int(values.len() as i64))
}

/// Σ(x − mean)² — the shared core of stdev and variance.
fn squared_deviations(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mean = mean(args)?;
    let mut squares = BigDecimal::zero();
    for x in args {
        let d = x - &mean;
        squares = &squares + &(&d * &d);
    }
    Ok(squares)
}

/// Least-squares line through the points; exact sums, working-precision
/// division. Returns (slope, intercept).
fn regression(
    xs: &[BigDecimal],
    ys: &[BigDecimal],
) -> Result<(BigDecimal, BigDecimal), EngineError> {
    let (mx, my) = (mean(xs)?, mean(ys)?);
    let mut sxy = BigDecimal::zero();
    let mut sxx = BigDecimal::zero();
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - &mx;
        sxy = &sxy + &(&dx * &(y - &my));
        sxx = &sxx + &(&dx * &dx);
    }
    if sxx.is_zero() {
        return Err(EngineError::domain(
            "the x values are constant — the line is vertical",
        ));
    }
    let slope = sxy.div(&sxx)?;
    let intercept = &my - &(&slope * &mx);
    Ok((slope, intercept))
}

/// Floor to an integer (the Swift `rounded(.down)`).
fn floored(value: &BigDecimal) -> BigDecimal {
    if value.is_integer() {
        return value.clone();
    }
    let scale = BigInt::from(10).pow((-value.exponent()) as u32);
    BigDecimal::new(value.significand().div_floor(&scale), 0)
}
