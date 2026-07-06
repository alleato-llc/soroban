//! Amortization helpers (exact powers, working-precision division) and the
//! shared numeric rate solver behind rate/irr/xirr.

use crate::eval::evaluator::require_int;
use crate::{BigDecimal, EngineError};

/// The interest/principal split of one payment: balance entering the period
/// times the rate is the interest; the rest of the payment is principal.
pub(super) fn amortization_split(
    args: &[BigDecimal],
    name: &str,
) -> Result<(BigDecimal, BigDecimal), EngineError> {
    let rate = &args[0];
    let per = require_int(&args[1], &format!("{name} per"))?;
    let nper = require_int(&args[2], &format!("{name} nper"))?;
    let pv = &args[3];
    let fv = args.get(4).cloned().unwrap_or_else(BigDecimal::zero);
    if !(nper >= 1 && (1..=nper).contains(&per)) {
        return Err(EngineError::domain(format!("{name} needs 1 ≤ per ≤ nper")));
    }
    let payment = payment_amount(rate, nper, pv, &fv)?;
    if rate.is_zero() {
        return Ok((BigDecimal::zero(), payment));
    }
    // Balance after per−1 payments: pv·g + pmt·(g−1)/r, g = (1+r)^(per−1).
    let growth = (&BigDecimal::one() + rate).power(per - 1)?;
    let annuity = (&growth - &BigDecimal::one()).div(rate)?;
    let balance = &(pv * &growth) + &(&payment * &annuity);
    let interest = -&(&balance * rate);
    let principal = &payment - &interest;
    Ok((interest, principal))
}

/// Sums the splits over start...end with a running balance — one pass, no
/// per-period power.
pub(super) fn cumulative(
    args: &[BigDecimal],
    name: &str,
) -> Result<(BigDecimal, BigDecimal), EngineError> {
    let rate = &args[0];
    let nper = require_int(&args[1], &format!("{name} nper"))?;
    let pv = &args[2];
    let start = require_int(&args[3], &format!("{name} start"))?;
    let end = require_int(&args[4], &format!("{name} end"))?;
    if nper > 100_000 {
        return Err(EngineError::domain(format!("{name} nper is too large")));
    }
    if !(1 <= start && start <= end && end <= nper) {
        return Err(EngineError::domain(format!(
            "{name} needs 1 ≤ start ≤ end ≤ nper"
        )));
    }
    let payment = payment_amount(rate, nper, pv, &BigDecimal::zero())?;
    let mut balance = pv.clone();
    let mut interest = BigDecimal::zero();
    let mut principal = BigDecimal::zero();
    for period in 1..=end {
        let owed = &balance * rate;
        if period >= start {
            interest = &interest - &owed;
            principal = &(&principal + &payment) + &owed;
        }
        balance = &(&balance + &payment) + &owed;
    }
    Ok((interest, principal))
}

/// pmt's formula, shared by the amortization functions.
fn payment_amount(
    rate: &BigDecimal,
    nper: i64,
    pv: &BigDecimal,
    fv: &BigDecimal,
) -> Result<BigDecimal, EngineError> {
    if nper <= 0 {
        return Err(EngineError::domain("nper must be positive"));
    }
    if rate.is_zero() {
        return (-&(pv + fv)).div(&BigDecimal::from_int(nper));
    }
    let growth = (&BigDecimal::one() + rate).power(nper)?;
    (&(-&(&(pv * &growth) + fv)) * rate).div(&(&growth - &BigDecimal::one()))
}

/// Newton–Raphson with numeric derivative, falling back to bisection over a
/// bracketing scan when Newton diverges. Roots below -100% are rejected.
/// Shared with xirr (dates.rs).
pub(crate) fn solve_rate(domain: &str, f: impl Fn(f64) -> f64) -> Result<BigDecimal, EngineError> {
    let tolerance = 1e-12;

    // Newton from a conventional starting guess.
    let mut r = 0.1;
    for _ in 0..60 {
        let value = f(r);
        if value.abs() < tolerance && r > -1.0 {
            match BigDecimal::from_f64(r) {
                Some(result) => return Ok(result),
                None => break,
            }
        }
        let h = r.abs().max(1e-4) * 1e-7;
        let slope = (f(r + h) - f(r - h)) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            break;
        }
        let next = r - value / slope;
        if !(next.is_finite() && next > -1.0) {
            break;
        }
        r = next;
    }

    // Bisection: scan for a sign change between -99.99% and 1000%.
    let mut lo = -0.9999;
    let mut hi = lo;
    let mut f_lo = f(lo);
    let mut found = false;
    while hi < 10.0 {
        hi = (hi + 0.1).min(10.0);
        let f_hi = f(hi);
        if f_lo.is_finite()
            && f_hi.is_finite()
            && f_lo.is_sign_negative() != f_hi.is_sign_negative()
        {
            found = true;
            break;
        }
        lo = hi;
        f_lo = f_hi;
    }
    if !found {
        return Err(EngineError::domain(format!("{domain} did not converge")));
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let f_mid = f(mid);
        if f_mid.abs() < tolerance || (hi - lo) / 2.0 < 1e-15 {
            return BigDecimal::from_f64(mid)
                .ok_or_else(|| EngineError::domain(format!("{domain} did not converge")));
        }
        if f_mid.is_sign_negative() == f_lo.is_sign_negative() {
            lo = mid;
            f_lo = f_mid;
        } else {
            hi = mid;
        }
    }
    Err(EngineError::domain(format!("{domain} did not converge")))
}
