//! Port of the finance function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Time-value-of-money functions following the spreadsheet sign convention:
//! money you pay out is negative, money you receive is positive, payments due
//! at the end of each period.
//!
//! `pv`, `fv`, `pmt`, `nper`, and `npv` are computed in BigDecimal (exact
//! powers, working-precision division). `rate` and `irr` are iterative
//! root-finding and run in the f64 domain (~15 significant digits), which
//! is far inside any real-world rate tolerance.

use crate::eval::evaluator::require_int;
use crate::eval::numeric;
use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::{BigDecimal, EngineError};

mod helpers;

pub(crate) use helpers::solve_rate;
use helpers::{amortization_split, cumulative};

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        // pmt(rate, nper, pv[, fv]) — periodic payment for a loan/annuity.
        BuiltinFunction {
            name: "pmt",
            category: FunctionCategory::Finance,
            signature: "pmt(rate, nper, pv, fv = 0)",
            summary: "Periodic payment for a loan or annuity (spreadsheet sign convention: money you pay out is negative).",
            examples: &["pmt(0.05/12, 360, 200000)", "pmt(0, 12, 1200)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(pmt),
        },
        // fv(rate, nper, pmt[, pv]) — future value.
        BuiltinFunction {
            name: "fv",
            category: FunctionCategory::Finance,
            signature: "fv(rate, nper, pmt, pv = 0)",
            summary: "Future value of regular payments (and an optional starting amount).",
            examples: &["fv(0.06/12, 120, -100)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(fv),
        },
        // pv(rate, nper, pmt[, fv]) — present value.
        BuiltinFunction {
            name: "pv",
            category: FunctionCategory::Finance,
            signature: "pv(rate, nper, pmt, fv = 0)",
            summary: "Present value of a stream of payments.",
            examples: &["pv(0.04/12, 60, -500)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(pv),
        },
        // nper(rate, pmt, pv[, fv]) — number of periods.
        BuiltinFunction {
            name: "nper",
            category: FunctionCategory::Finance,
            signature: "nper(rate, pmt, pv, fv = 0)",
            summary: "Number of periods needed to pay off pv with the given payment.",
            examples: &["nper(0.05/12, -1073.64, 200000)"],
            arity: 3..=4,
            implementation: Implementation::Numeric(nper),
        },
        // rate(nper, pmt, pv[, fv]) — periodic interest rate, found numerically.
        BuiltinFunction {
            name: "rate",
            category: FunctionCategory::Finance,
            signature: "rate(nper, pmt, pv, fv = 0)",
            summary: "Periodic interest rate, found numerically (Newton + bisection).",
            examples: &["rate(360, -1073.64, 200000) * 12"],
            arity: 3..=4,
            implementation: Implementation::Numeric(rate),
        },
        // npv(rate, cashflow1, cashflow2, ...) — flows at the END of periods 1..n.
        BuiltinFunction {
            name: "npv",
            category: FunctionCategory::Finance,
            signature: "npv(rate, flow1, flow2, …)",
            summary: "Net present value of cash flows at the END of periods 1, 2, … Accepts ranges.",
            examples: &["npv(0.1, 3000, 4200, 6800)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(npv),
        },
        // irr(cashflow0, cashflow1, ...) — rate where NPV (flow 0 at t=0) is zero.
        BuiltinFunction {
            name: "irr",
            category: FunctionCategory::Finance,
            signature: "irr(flow0, flow1, …)",
            summary: "Internal rate of return; flow 0 happens today. Needs at least one inflow and one outflow.",
            examples: &["irr(-70000, 12000, 15000, 18000, 21000, 26000)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(irr),
        },
        // effectiveRate(nominal, periodsPerYear) — APR → effective annual rate.
        BuiltinFunction {
            name: "effectiveRate",
            category: FunctionCategory::Finance,
            signature: "effectiveRate(nominal, periodsPerYear)",
            summary: "Effective annual rate of a nominal APR compounded n times per year.",
            examples: &["effectiveRate(0.06, 12)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(effective_rate),
        },
        // nominal(effective, periodsPerYear) — effectiveRate's inverse.
        BuiltinFunction {
            name: "nominal",
            category: FunctionCategory::Finance,
            signature: "nominal(effective, periodsPerYear)",
            summary: "Nominal APR behind an effective annual rate compounded n times per year — effectiveRate's inverse.",
            examples: &["nominal(0.0617, 12)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(nominal),
        },
        // ipmt(rate, per, nper, pv[, fv]) — the interest share of payment `per`.
        BuiltinFunction {
            name: "ipmt",
            category: FunctionCategory::Finance,
            signature: "ipmt(rate, per, nper, pv, fv = 0)",
            summary: "Interest portion of the payment in period `per` (1-based) — pairs with ppmt to build amortization tables. Spreadsheet sign convention.",
            examples: &["ipmt(0.05/12, 1, 360, 200000)", "ipmt(0.05/12, 360, 360, 200000)"],
            arity: 4..=5,
            implementation: Implementation::Numeric(ipmt),
        },
        // ppmt(rate, per, nper, pv[, fv]) — the principal share.
        BuiltinFunction {
            name: "ppmt",
            category: FunctionCategory::Finance,
            signature: "ppmt(rate, per, nper, pv, fv = 0)",
            summary: "Principal portion of the payment in period `per` (1-based); ipmt + ppmt = pmt every period.",
            examples: &["ppmt(0.05/12, 1, 360, 200000)"],
            arity: 4..=5,
            implementation: Implementation::Numeric(ppmt),
        },
        // cumipmt(rate, nper, pv, startPer, endPer) — interest paid over a span.
        BuiltinFunction {
            name: "cumipmt",
            category: FunctionCategory::Finance,
            signature: "cumipmt(rate, nper, pv, start, end)",
            summary: "Total interest paid between periods start and end (inclusive, 1-based) — what a year of a mortgage costs in interest.",
            examples: &["cumipmt(0.05/12, 360, 200000, 1, 12)"],
            arity: 5..=5,
            implementation: Implementation::Numeric(cumipmt),
        },
        BuiltinFunction {
            name: "cumprinc",
            category: FunctionCategory::Finance,
            signature: "cumprinc(rate, nper, pv, start, end)",
            summary: "Total principal paid between periods start and end (inclusive, 1-based).",
            examples: &["cumprinc(0.05/12, 360, 200000, 1, 12)"],
            arity: 5..=5,
            implementation: Implementation::Numeric(cumprinc),
        },
        // Depreciation — the accounting trio (accounting category, like Swift).
        BuiltinFunction {
            name: "sln",
            category: FunctionCategory::Accounting,
            signature: "sln(cost, salvage, life)",
            summary: "Straight-line depreciation per period.",
            examples: &["sln(30000, 7500, 10)"],
            arity: 3..=3,
            implementation: Implementation::Numeric(sln),
        },
        BuiltinFunction {
            name: "syd",
            category: FunctionCategory::Accounting,
            signature: "syd(cost, salvage, life, per)",
            summary: "Sum-of-years'-digits depreciation for period `per`.",
            examples: &["syd(30000, 7500, 10, 1)", "syd(30000, 7500, 10, 10)"],
            arity: 4..=4,
            implementation: Implementation::Numeric(syd),
        },
        BuiltinFunction {
            name: "ddb",
            category: FunctionCategory::Accounting,
            signature: "ddb(cost, salvage, life, per, factor = 2)",
            summary: "Declining-balance depreciation for period `per` (factor 2 = double-declining). Never depreciates below salvage.",
            examples: &["ddb(30000, 7500, 10, 1)", "ddb(30000, 7500, 10, 10)"],
            arity: 4..=5,
            implementation: Implementation::Numeric(ddb),
        },
    ]
}

// MARK: - Implementations

fn pmt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (rate, pv) = (&args[0], &args[2]);
    let nper = require_int(&args[1], "pmt nper")?;
    let fv = args.get(3).cloned().unwrap_or_else(BigDecimal::zero);
    if rate.is_zero() {
        return (-&(pv + &fv)).div(&BigDecimal::from_int(nper));
    }
    let growth = (&BigDecimal::one() + rate).power(nper)?;
    (&(-&(&(pv * &growth) + &fv)) * rate).div(&(&growth - &BigDecimal::one()))
}

fn fv(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (rate, pmt) = (&args[0], &args[2]);
    let nper = require_int(&args[1], "fv nper")?;
    let pv = args.get(3).cloned().unwrap_or_else(BigDecimal::zero);
    if rate.is_zero() {
        return Ok(-&(&pv + &(pmt * &BigDecimal::from_int(nper))));
    }
    let growth = (&BigDecimal::one() + rate).power(nper)?;
    let annuity = (&growth - &BigDecimal::one()).div(rate)?;
    Ok(-&(&(&pv * &growth) + &(pmt * &annuity)))
}

fn pv(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (rate, pmt) = (&args[0], &args[2]);
    let nper = require_int(&args[1], "pv nper")?;
    let fv = args.get(3).cloned().unwrap_or_else(BigDecimal::zero);
    if rate.is_zero() {
        return Ok(-&(&fv + &(pmt * &BigDecimal::from_int(nper))));
    }
    let growth = (&BigDecimal::one() + rate).power(nper)?;
    let annuity = (&growth - &BigDecimal::one()).div(rate)?;
    (-&(&(pmt * &annuity) + &fv)).div(&growth)
}

fn nper(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (rate, pmt, pv) = (args[0].to_f64(), args[1].to_f64(), args[2].to_f64());
    let fv = args.get(3).map(|v| v.to_f64()).unwrap_or(0.0);
    if rate == 0.0 {
        if pmt == 0.0 {
            return Err(EngineError::domain("nper: pmt cannot be 0 when rate is 0"));
        }
        return BigDecimal::from_f64(-(pv + fv) / pmt)
            .ok_or_else(|| EngineError::domain("nper is undefined for these values"));
    }
    let numerator = pmt - fv * rate;
    let denominator = pmt + pv * rate;
    // NaN must fail this guard too — a flipped `<= 0.0` would let NaN through.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(numerator / denominator > 0.0) {
        return Err(EngineError::domain("nper is undefined for these values"));
    }
    BigDecimal::from_f64(libm::log(numerator / denominator) / libm::log1p(rate))
        .ok_or_else(|| EngineError::domain("nper is undefined for these values"))
}

fn rate(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let nper = require_int(&args[0], "rate nper")? as f64;
    let (pmt, pv) = (args[1].to_f64(), args[2].to_f64());
    let fv = args.get(3).map(|v| v.to_f64()).unwrap_or(0.0);
    solve_rate("rate", |r| {
        if r.abs() < 1e-14 {
            return pv + pmt * nper + fv;
        }
        let growth = libm::pow(1.0 + r, nper);
        pv * growth + pmt * (growth - 1.0) / r + fv
    })
}

fn npv(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let rate = &args[0];
    let one_plus = &BigDecimal::one() + rate;
    let mut total = BigDecimal::zero();
    for (i, flow) in args[1..].iter().enumerate() {
        total = &total + &flow.div(&one_plus.power(i as i64 + 1)?)?;
    }
    Ok(total)
}

fn irr(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let flows: Vec<f64> = args.iter().map(|v| v.to_f64()).collect();
    if !(flows.iter().any(|f| *f > 0.0) && flows.iter().any(|f| *f < 0.0)) {
        return Err(EngineError::domain(
            "irr needs both positive and negative cash flows",
        ));
    }
    solve_rate("irr", |r| {
        flows.iter().enumerate().fold(0.0, |sum, (offset, flow)| {
            sum + flow / libm::pow(1.0 + r, offset as f64)
        })
    })
}

fn effective_rate(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let m = require_int(&args[1], "effectiveRate periods")?;
    if m <= 0 {
        return Err(EngineError::domain(
            "effectiveRate periods must be positive",
        ));
    }
    let per_period = args[0].div(&BigDecimal::from_int(m))?;
    Ok(&(&BigDecimal::one() + &per_period).power(m)? - &BigDecimal::one())
}

fn nominal(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let m = require_int(&args[1], "nominal periods")?;
    if m <= 0 {
        return Err(EngineError::domain("nominal periods must be positive"));
    }
    if args[0] <= BigDecimal::from_int(-1) {
        return Err(EngineError::domain(
            "nominal needs an effective rate above -100%",
        ));
    }
    let exponent = BigDecimal::one().div(&BigDecimal::from_int(m))?;
    let per_period =
        &numeric::pow(&(&BigDecimal::one() + &args[0]), &exponent)? - &BigDecimal::one();
    Ok(&per_period * &BigDecimal::from_int(m))
}

fn ipmt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (interest, _principal) = amortization_split(args, "ipmt")?;
    Ok(interest)
}

fn ppmt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (_interest, principal) = amortization_split(args, "ppmt")?;
    Ok(principal)
}

fn cumipmt(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (interest, _principal) = cumulative(args, "cumipmt")?;
    Ok(interest)
}

fn cumprinc(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (_interest, principal) = cumulative(args, "cumprinc")?;
    Ok(principal)
}

fn sln(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    if args[2].is_zero() {
        return Err(EngineError::domain("sln life can't be 0"));
    }
    (&args[0] - &args[1]).div(&args[2])
}

fn syd(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let life = require_int(&args[2], "syd life")?;
    let per = require_int(&args[3], "syd per")?;
    if !(life > 0 && (1..=life).contains(&per)) {
        return Err(EngineError::domain("syd needs 1 ≤ per ≤ life"));
    }
    let digits = (&BigDecimal::from_int(life) * &BigDecimal::from_int(life + 1))
        .div(&BigDecimal::from_int(2))?;
    (&(&args[0] - &args[1]) * &BigDecimal::from_int(life - per + 1)).div(&digits)
}

fn ddb(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let life = require_int(&args[2], "ddb life")?;
    let per = require_int(&args[3], "ddb per")?;
    let factor = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| BigDecimal::from_int(2));
    if !(life > 0 && (1..=life).contains(&per)) {
        return Err(EngineError::domain("ddb needs 1 ≤ per ≤ life"));
    }
    let rate = factor.div(&BigDecimal::from_int(life))?;
    let mut book = args[0].clone();
    let mut depreciation = BigDecimal::zero();
    for _ in 1..=per {
        depreciation = &book * &rate;
        // Never depreciate below salvage value.
        if &book - &depreciation < args[1] {
            depreciation = (&book - &args[1]).max(BigDecimal::zero());
        }
        book = &book - &depreciation;
    }
    Ok(depreciation)
}
