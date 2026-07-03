//! Port of the dates function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Dates are integer **day serial numbers** (days since 1970-01-01), so they
//! are ordinary BigDecimals: subtract for day counts, compare, aggregate over
//! ranges. Conversion uses pure civil-calendar integer math (Howard
//! Hinnant's algorithms) — deterministic, no timezone involvement except in
//! `today()`.

use super::finance::solve_rate;
use crate::eval::evaluator::require_int;
use crate::eval::registry::{BuiltinFunction, FunctionCategory, Implementation};
use crate::{BigDecimal, EngineError};
use std::collections::HashSet;

// MARK: - CivilDate (Howard Hinnant's civil-calendar algorithms)

/// y/m/d → days since 1970-01-01 (proleptic Gregorian).
fn serial(year: i64, month: i64, day: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// days since 1970-01-01 → (y, m, d).
fn civil(serial: i64) -> (i64, i64, i64) {
    let z = serial + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (y + if month <= 2 { 1 } else { 0 }, month, day)
}

fn last_day(year: i64, month: i64) -> i64 {
    let first_of_next = if month == 12 {
        serial(year + 1, 1, 1)
    } else {
        serial(year, month + 1, 1)
    };
    civil(first_of_next - 1).2
}

/// Adds months, clamping to the target month's end (Jan 31 + 1mo → Feb 28/29).
fn adding_months(months: i64, to_serial: i64) -> i64 {
    let (y, m, d) = civil(to_serial);
    let zero_based = y * 12 + (m - 1) + months;
    let year = if zero_based >= 0 {
        zero_based / 12
    } else {
        (zero_based - 11) / 12
    };
    let month = zero_based - year * 12 + 1;
    let day = d.min(last_day(year, month));
    serial(year, month, day)
}

fn serial_argument(value: &BigDecimal, what: &str) -> Result<i64, EngineError> {
    require_int(value, what)
}

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        // date(2026, 6, 6) → day serial. Validates the civil date is real.
        BuiltinFunction {
            name: "date",
            category: FunctionCategory::Dates,
            signature: "date(year, month, day)",
            summary: "A calendar date as an exact day number (days since 1970-01-01). Subtract dates for day counts.",
            examples: &["date(2026, 6, 6)", "date(2026, 6, 6) - date(2026, 1, 1)"],
            arity: 3..=3,
            implementation: Implementation::Numeric(date_impl),
        },
        // Today's date, as a serial.
        BuiltinFunction {
            name: "today",
            category: FunctionCategory::Dates,
            signature: "today()",
            summary: "Today's date as a day number.",
            examples: &["year(today())"],
            arity: 0..=0,
            implementation: Implementation::Numeric(today_impl),
        },
        BuiltinFunction {
            name: "year",
            category: FunctionCategory::Dates,
            signature: "year(date)",
            summary: "The year of a date.",
            examples: &["year(date(2026, 6, 6))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(year_impl),
        },
        BuiltinFunction {
            name: "month",
            category: FunctionCategory::Dates,
            signature: "month(date)",
            summary: "The month (1–12) of a date.",
            examples: &["month(date(2026, 6, 6))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(month_impl),
        },
        BuiltinFunction {
            name: "day",
            category: FunctionCategory::Dates,
            signature: "day(date)",
            summary: "The day of month of a date.",
            examples: &["day(date(2026, 6, 6))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(day_impl),
        },
        // 1 = Monday … 7 = Sunday (ISO). 1970-01-01 was a Thursday (4).
        BuiltinFunction {
            name: "weekday",
            category: FunctionCategory::Dates,
            signature: "weekday(date)",
            summary: "Day of week: 1 = Monday … 7 = Sunday.",
            examples: &["weekday(date(2026, 6, 6))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(weekday_impl),
        },
        // edate(serial, months) — month arithmetic with end-of-month clamping.
        BuiltinFunction {
            name: "edate",
            category: FunctionCategory::Dates,
            signature: "edate(date, months)",
            summary: "Shifts a date by whole months, clamping to month end (Jan 31 + 1 → Feb 28/29).",
            examples: &["edate(date(2024, 1, 31), 1) == date(2024, 2, 29)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(edate_impl),
        },
        // eomonth(serial, months) — last day of the month `months` away.
        BuiltinFunction {
            name: "eomonth",
            category: FunctionCategory::Dates,
            signature: "eomonth(date, months)",
            summary: "Last day of the month that is `months` away.",
            examples: &["eomonth(date(2026, 6, 6), 0) == date(2026, 6, 30)"],
            arity: 2..=2,
            implementation: Implementation::Numeric(eomonth_impl),
        },
        // days(later, earlier) — sugar for subtraction.
        BuiltinFunction {
            name: "days",
            category: FunctionCategory::Dates,
            signature: "days(later, earlier)",
            summary: "Days between two dates (just subtraction).",
            examples: &["days(date(2026, 2, 1), date(2026, 1, 1))"],
            arity: 2..=2,
            implementation: Implementation::Numeric(days_impl),
        },
        // xnpv(rate, dates…, flows…) — irregular cash flows. Arguments after
        // the rate split evenly: first half dates, second half flows (supply
        // two equal ranges: xnpv(0.1, A:1..A:5, B:1..B:5)).
        BuiltinFunction {
            name: "xnpv",
            category: FunctionCategory::Dates,
            signature: "xnpv(rate, dates…, flows…)",
            summary: "Net present value of cash flows on specific dates. Pass dates then flows, same count each — typically two equal ranges.",
            examples: &["xnpv(0.09, date(2026,1,1), date(2026,7,1), -1000, 1100)"],
            arity: 3..=usize::MAX,
            implementation: Implementation::Numeric(xnpv_impl),
        },
        // xirr(dates…, flows…) — the rate where xnpv is zero.
        BuiltinFunction {
            name: "xirr",
            category: FunctionCategory::Dates,
            signature: "xirr(dates…, flows…)",
            summary: "Internal rate of return for dated cash flows (dates first, flows second, equal counts).",
            examples: &["xirr(date(2025,1,1), date(2025,12,31), -1000, 1100)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(xirr_impl),
        },
        BuiltinFunction {
            name: "quarter",
            category: FunctionCategory::Dates,
            signature: "quarter(date)",
            summary: "Calendar quarter of a date: 1–4.",
            examples: &["quarter(date(2026, 6, 6))", "quarter(date(2026, 11, 1))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(quarter_impl),
        },
        BuiltinFunction {
            name: "weeknum",
            category: FunctionCategory::Dates,
            signature: "weeknum(date)",
            summary: "Week of the year, 1-based — the week containing January 1 is week 1, weeks start on Sunday (spreadsheet system 1).",
            examples: &["weeknum(date(2026, 1, 1))", "weeknum(date(2026, 1, 4))"],
            arity: 1..=1,
            implementation: Implementation::Numeric(weeknum_impl),
        },
        // workday(start, days[, holidays…]) — business-day arithmetic.
        // Weekends are Saturday/Sunday; extra arguments (or a range of date
        // serials) are holidays to skip.
        BuiltinFunction {
            name: "workday",
            category: FunctionCategory::Dates,
            signature: "workday(start, days, holidays…)",
            summary: "The date `days` business days from start (negative goes backward). Weekends skip; pass holiday dates (or a range of them) to skip those too.",
            examples: &["workday(date(2026, 6, 5), 1)", "workday(date(2026, 6, 1), 10)"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(workday_impl),
        },
        BuiltinFunction {
            name: "networkdays",
            category: FunctionCategory::Dates,
            signature: "networkdays(start, end, holidays…)",
            summary: "Business days from start to end, INCLUSIVE of both (negative when end is before start). Weekends skip; extra arguments are holidays.",
            examples: &[
                "networkdays(date(2026, 6, 1), date(2026, 6, 5))",
                "networkdays(date(2026, 6, 1), date(2026, 6, 30))",
            ],
            arity: 2..=usize::MAX,
            implementation: Implementation::Numeric(networkdays_impl),
        },
    ]
}

// MARK: - Implementations

fn date_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let year = require_int(&args[0], "date year")?;
    let month = require_int(&args[1], "date month")?;
    let day = require_int(&args[2], "date day")?;
    if !(1..=12).contains(&month) || !(1..=last_day(year, month)).contains(&day) {
        return Err(EngineError::domain(format!(
            "{year}-{month}-{day} is not a valid date"
        )));
    }
    Ok(BigDecimal::from_int(serial(year, month, day)))
}

fn today_impl(_args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    // Swift reads the local calendar via Foundation; here the system clock
    // is the one sanctioned impurity (mirroring that Foundation use).
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(BigDecimal::from_int(seconds.div_euclid(86_400)))
}

fn year_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(BigDecimal::from_int(
        civil(serial_argument(&args[0], "year")?).0,
    ))
}

fn month_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(BigDecimal::from_int(
        civil(serial_argument(&args[0], "month")?).1,
    ))
}

fn day_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(BigDecimal::from_int(
        civil(serial_argument(&args[0], "day")?).2,
    ))
}

fn weekday_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let serial = serial_argument(&args[0], "weekday")?;
    let iso = (serial + 3) % 7; // 0 = Monday
    Ok(BigDecimal::from_int((iso + 7) % 7 + 1))
}

fn edate_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let months = require_int(&args[1], "edate months")?;
    Ok(BigDecimal::from_int(adding_months(
        months,
        serial_argument(&args[0], "edate")?,
    )))
}

fn eomonth_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let shifted = adding_months(
        require_int(&args[1], "eomonth months")?,
        serial_argument(&args[0], "eomonth")?,
    );
    let (year, month, _) = civil(shifted);
    Ok(BigDecimal::from_int(serial(
        year,
        month,
        last_day(year, month),
    )))
}

fn days_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    Ok(&args[0] - &args[1])
}

fn xnpv_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (dates, flows) = split_dates_and_flows(&args[1..], "xnpv")?;
    let rate = args[0].to_f64();
    // NaN must fail this guard too — `rate <= -1.0` would let NaN through.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(rate > -1.0) {
        return Err(EngineError::domain("xnpv rate must be greater than -100%"));
    }
    BigDecimal::from_f64(xnpv_value(rate, &dates, &flows))
        .ok_or_else(|| EngineError::domain("xnpv is undefined for these values"))
}

fn xirr_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let (dates, flows) = split_dates_and_flows(args, "xirr")?;
    if !(flows.iter().any(|f| *f > 0.0) && flows.iter().any(|f| *f < 0.0)) {
        return Err(EngineError::domain(
            "xirr needs both positive and negative cash flows",
        ));
    }
    solve_rate("xirr", |rate| xnpv_value(rate, &dates, &flows))
}

fn quarter_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let serial = serial_argument(&args[0], "quarter")?;
    Ok(BigDecimal::from_int((civil(serial).1 - 1) / 3 + 1))
}

fn weeknum_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let day = serial_argument(&args[0], "weeknum")?;
    let year = civil(day).0;
    let jan1 = serial(year, 1, 1);
    let sunday_based = ((jan1 % 7) + 7 + 4) % 7; // serial 0 = Thursday = 4
    Ok(BigDecimal::from_int((day - jan1 + sunday_based) / 7 + 1))
}

fn workday_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let mut serial = serial_argument(&args[0], "workday start")?;
    let days = require_int(&args[1], "workday days")?;
    if days.abs() > 100_000 {
        return Err(EngineError::domain("workday spans too many days"));
    }
    let mut holidays = HashSet::new();
    for value in &args[2..] {
        holidays.insert(serial_argument(value, "workday holiday")?);
    }
    let step = if days < 0 { -1 } else { 1 };
    let mut remaining = days.abs();
    while remaining > 0 {
        serial += step;
        if is_business_day(serial, &holidays) {
            remaining -= 1;
        }
    }
    Ok(BigDecimal::from_int(serial))
}

fn networkdays_impl(args: &[BigDecimal]) -> Result<BigDecimal, EngineError> {
    let a = serial_argument(&args[0], "networkdays start")?;
    let b = serial_argument(&args[1], "networkdays end")?;
    let (lo, hi) = (a.min(b), a.max(b));
    if hi - lo > 1_000_000 {
        return Err(EngineError::domain("networkdays spans too many days"));
    }
    let mut holidays = HashSet::new();
    for value in &args[2..] {
        holidays.insert(serial_argument(value, "networkdays holiday")?);
    }
    let mut count = 0i64;
    for day in lo..=hi {
        if is_business_day(day, &holidays) {
            count += 1;
        }
    }
    Ok(BigDecimal::from_int(if a <= b { count } else { -count }))
}

// MARK: - Helpers

/// Monday–Friday and not a listed holiday. Serial 0 (1970-01-01) was a
/// Thursday, so Sunday-based weekday = (serial + 4) mod 7.
fn is_business_day(serial: i64, holidays: &HashSet<i64>) -> bool {
    let sunday_based = ((serial % 7) + 7 + 4) % 7;
    sunday_based != 0 && sunday_based != 6 && !holidays.contains(&serial)
}

/// Splits trailing arguments evenly into (dates, flows).
fn split_dates_and_flows(
    args: &[BigDecimal],
    function: &str,
) -> Result<(Vec<f64>, Vec<f64>), EngineError> {
    if args.len() < 2 || !args.len().is_multiple_of(2) {
        return Err(EngineError::domain(format!(
            "{function} needs matching dates and flows — e.g. {function}(A:1..A:5, B:1..B:5)"
        )));
    }
    let half = args.len() / 2;
    Ok((
        args[..half].iter().map(BigDecimal::to_f64).collect(),
        args[half..].iter().map(BigDecimal::to_f64).collect(),
    ))
}

fn xnpv_value(rate: f64, dates: &[f64], flows: &[f64]) -> f64 {
    let t0 = dates[0];
    dates.iter().zip(flows).fold(0.0, |total, (date, flow)| {
        total + flow / (1.0 + rate).powf((date - t0) / 365.0)
    })
}
