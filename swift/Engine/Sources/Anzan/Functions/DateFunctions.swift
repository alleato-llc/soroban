import Foundation

/// Dates are integer **day serial numbers** (days since 1970-01-01), so they
/// are ordinary BigDecimals: subtract for day counts, compare, aggregate over
/// ranges. Conversion uses pure civil-calendar integer math (Howard Hinnant's
/// algorithms) — deterministic, no Calendar/timezone involvement except in
/// `today()`. Pretty date *display* arrives with cell formatting; for now
/// serials read as numbers.
package enum CivilDate {
    /// y/m/d → days since 1970-01-01 (proleptic Gregorian).
    static func serial(year: Int, month: Int, day: Int) -> Int {
        let y = year - (month <= 2 ? 1 : 0)
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400                                   // [0, 399]
        let doy = (153 * (month + (month > 2 ? -3 : 9)) + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy           // [0, 146096]
        return era * 146097 + doe - 719468
    }

    /// days since 1970-01-01 → (y, m, d).
    package static func civil(fromSerial serial: Int) -> (year: Int, month: Int, day: Int) {
        let z = serial + 719468
        let era = (z >= 0 ? z : z - 146096) / 146097
        let doe = z - era * 146097                                // [0, 146096]
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
        let y = yoe + era * 400
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)         // [0, 365]
        let mp = (5 * doy + 2) / 153                              // [0, 11]
        let day = doy - (153 * mp + 2) / 5 + 1
        let month = mp + (mp < 10 ? 3 : -9)
        return (y + (month <= 2 ? 1 : 0), month, day)
    }

    static func lastDay(year: Int, month: Int) -> Int {
        let firstOfNext = month == 12 ? serial(year: year + 1, month: 1, day: 1)
                                      : serial(year: year, month: month + 1, day: 1)
        return civil(fromSerial: firstOfNext - 1).day
    }

    /// Adds months, clamping to the target month's end (Jan 31 + 1mo → Feb 28/29).
    static func addingMonths(_ months: Int, toSerial serial: Int) -> Int {
        let date = civil(fromSerial: serial)
        let zeroBased = date.year * 12 + (date.month - 1) + months
        let year = zeroBased >= 0 ? zeroBased / 12 : (zeroBased - 11) / 12
        let month = zeroBased - year * 12 + 1
        let day = min(date.day, lastDay(year: year, month: month))
        return Self.serial(year: year, month: month, day: day)
    }
}

private func serialArgument(_ value: BigDecimal, _ what: String) throws -> Int {
    try requireInt(value, what)
}

let dateFunctions: [BuiltinFunction] = [
    // date(2026, 6, 6) → day serial. Validates the civil date is real.
    BuiltinFunction(name: "date", category: .dates,
                    signature: "date(year, month, day)",
                    summary: "A calendar date as an exact day number (days since 1970-01-01). Subtract dates for day counts.",
                    examples: ["date(2026, 6, 6)", "date(2026, 6, 6) - date(2026, 1, 1)"],
                    arity: 3...3) { args in
        let year = try requireInt(args[0], "date year")
        let month = try requireInt(args[1], "date month")
        let day = try requireInt(args[2], "date day")
        guard (1...12).contains(month),
              (1...CivilDate.lastDay(year: year, month: month)).contains(day) else {
            throw EngineError.domainError(message: "\(year)-\(month)-\(day) is not a valid date")
        }
        return BigDecimal(CivilDate.serial(year: year, month: month, day: day))
    },

    // Today's date in the local calendar, as a serial.
    BuiltinFunction(name: "today", category: .dates,
                    signature: "today()",
                    summary: "Today's date as a day number.",
                    examples: ["year(today())"],
                    arity: 0...0) { _ in
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        return BigDecimal(CivilDate.serial(year: parts.year ?? 1970,
                                           month: parts.month ?? 1,
                                           day: parts.day ?? 1))
    },

    BuiltinFunction(name: "year", category: .dates,
                    signature: "year(date)",
                    summary: "The year of a date.",
                    examples: ["year(date(2026, 6, 6))"],
                    arity: 1...1) { args in
        BigDecimal(CivilDate.civil(fromSerial: try serialArgument(args[0], "year")).year)
    },
    BuiltinFunction(name: "month", category: .dates,
                    signature: "month(date)",
                    summary: "The month (1–12) of a date.",
                    examples: ["month(date(2026, 6, 6))"],
                    arity: 1...1) { args in
        BigDecimal(CivilDate.civil(fromSerial: try serialArgument(args[0], "month")).month)
    },
    BuiltinFunction(name: "day", category: .dates,
                    signature: "day(date)",
                    summary: "The day of month of a date.",
                    examples: ["day(date(2026, 6, 6))"],
                    arity: 1...1) { args in
        BigDecimal(CivilDate.civil(fromSerial: try serialArgument(args[0], "day")).day)
    },

    // 1 = Monday … 7 = Sunday (ISO). 1970-01-01 was a Thursday (4).
    BuiltinFunction(name: "weekday", category: .dates,
                    signature: "weekday(date)",
                    summary: "Day of week: 1 = Monday … 7 = Sunday.",
                    examples: ["weekday(date(2026, 6, 6))"],
                    arity: 1...1) { args in
        let serial = try serialArgument(args[0], "weekday")
        let iso = (serial + 3) % 7 // 0 = Monday
        return BigDecimal((iso + 7) % 7 + 1)
    },

    // edate(serial, months) — month arithmetic with end-of-month clamping.
    BuiltinFunction(name: "edate", category: .dates,
                    signature: "edate(date, months)",
                    summary: "Shifts a date by whole months, clamping to month end (Jan 31 + 1 → Feb 28/29).",
                    examples: ["edate(date(2024, 1, 31), 1) == date(2024, 2, 29)"],
                    arity: 2...2) { args in
        BigDecimal(CivilDate.addingMonths(try requireInt(args[1], "edate months"),
                                          toSerial: try serialArgument(args[0], "edate")))
    },

    // eomonth(serial, months) — last day of the month `months` away.
    BuiltinFunction(name: "eomonth", category: .dates,
                    signature: "eomonth(date, months)",
                    summary: "Last day of the month that is `months` away.",
                    examples: ["eomonth(date(2026, 6, 6), 0) == date(2026, 6, 30)"],
                    arity: 2...2) { args in
        let shifted = CivilDate.addingMonths(try requireInt(args[1], "eomonth months"),
                                             toSerial: try serialArgument(args[0], "eomonth"))
        let date = CivilDate.civil(fromSerial: shifted)
        return BigDecimal(CivilDate.serial(year: date.year, month: date.month,
                                           day: CivilDate.lastDay(year: date.year, month: date.month)))
    },

    // days(later, earlier) — sugar for subtraction.
    BuiltinFunction(name: "days", category: .dates,
                    signature: "days(later, earlier)",
                    summary: "Days between two dates (just subtraction).",
                    examples: ["days(date(2026, 2, 1), date(2026, 1, 1))"],
                    arity: 2...2) { args in
        args[0] - args[1]
    },

    // xnpv(rate, dates…, flows…) — irregular cash flows. Arguments after the
    // rate split evenly: first half dates, second half flows (supply two
    // equal ranges: xnpv(0.1, A:1..A:5, B:1..B:5)).
    BuiltinFunction(name: "xnpv", category: .dates,
                    signature: "xnpv(rate, dates…, flows…)",
                    summary: "Net present value of cash flows on specific dates. Pass dates then flows, same count each — typically two equal ranges.",
                    examples: ["xnpv(0.09, date(2026,1,1), date(2026,7,1), -1000, 1100)"],
                    arity: 3...Int.max) { args in
        let (dates, flows) = try splitDatesAndFlows(Array(args.dropFirst()), function: "xnpv")
        let rate = args[0].doubleValue
        guard rate > -1 else {
            throw EngineError.domainError(message: "xnpv rate must be greater than -100%")
        }
        guard let result = BigDecimal(xnpvValue(rate: rate, dates: dates, flows: flows)) else {
            throw EngineError.domainError(message: "xnpv is undefined for these values")
        }
        return result
    },

    // xirr(dates…, flows…) — the rate where xnpv is zero.
    BuiltinFunction(name: "xirr", category: .dates,
                    signature: "xirr(dates…, flows…)",
                    summary: "Internal rate of return for dated cash flows (dates first, flows second, equal counts).",
                    examples: ["xirr(date(2025,1,1), date(2025,12,31), -1000, 1100)"],
                    arity: 2...Int.max) { args in
        let (dates, flows) = try splitDatesAndFlows(args, function: "xirr")
        guard flows.contains(where: { $0 > 0 }), flows.contains(where: { $0 < 0 }) else {
            throw EngineError.domainError(message: "xirr needs both positive and negative cash flows")
        }
        return try solveRate(domain: "xirr") { rate in
            xnpvValue(rate: rate, dates: dates, flows: flows)
        }
    },

    BuiltinFunction(name: "quarter", category: .dates,
                    signature: "quarter(date)",
                    summary: "Calendar quarter of a date: 1–4.",
                    examples: ["quarter(date(2026, 6, 6))", "quarter(date(2026, 11, 1))"],
                    arity: 1...1) { args in
        let serial = try serialArgument(args[0], "quarter")
        return BigDecimal((CivilDate.civil(fromSerial: serial).month - 1) / 3 + 1)
    },

    BuiltinFunction(name: "weeknum", category: .dates,
                    signature: "weeknum(date)",
                    summary: "Week of the year, 1-based — the week containing January 1 is week 1, weeks start on Sunday (spreadsheet system 1).",
                    examples: ["weeknum(date(2026, 1, 1))", "weeknum(date(2026, 1, 4))"],
                    arity: 1...1) { args in
        let serial = try serialArgument(args[0], "weeknum")
        let year = CivilDate.civil(fromSerial: serial).year
        let jan1 = CivilDate.serial(year: year, month: 1, day: 1)
        let sundayBased = ((jan1 % 7) + 7 + 4) % 7 // serial 0 = Thursday = 4
        return BigDecimal((serial - jan1 + sundayBased) / 7 + 1)
    },

    // workday(start, days[, holidays…]) — business-day arithmetic. Weekends
    // are Saturday/Sunday; extra arguments (or a range of date serials) are
    // holidays to skip.
    BuiltinFunction(name: "workday", category: .dates,
                    signature: "workday(start, days, holidays…)",
                    summary: "The date `days` business days from start (negative goes backward). Weekends skip; pass holiday dates (or a range of them) to skip those too.",
                    examples: ["workday(date(2026, 6, 5), 1)", "workday(date(2026, 6, 1), 10)"],
                    arity: 2...Int.max) { args in
        var serial = try serialArgument(args[0], "workday start")
        let days = try requireInt(args[1], "workday days")
        guard abs(days) <= 100_000 else {
            throw EngineError.domainError(message: "workday spans too many days")
        }
        let holidays = Set(try args.dropFirst(2).map { try serialArgument($0, "workday holiday") })
        let step = days < 0 ? -1 : 1
        var remaining = abs(days)
        while remaining > 0 {
            serial += step
            if isBusinessDay(serial, holidays: holidays) {
                remaining -= 1
            }
        }
        return BigDecimal(serial)
    },

    BuiltinFunction(name: "networkdays", category: .dates,
                    signature: "networkdays(start, end, holidays…)",
                    summary: "Business days from start to end, INCLUSIVE of both (negative when end is before start). Weekends skip; extra arguments are holidays.",
                    examples: ["networkdays(date(2026, 6, 1), date(2026, 6, 5))", "networkdays(date(2026, 6, 1), date(2026, 6, 30))"],
                    arity: 2...Int.max) { args in
        let a = try serialArgument(args[0], "networkdays start")
        let b = try serialArgument(args[1], "networkdays end")
        let (lo, hi) = (Swift.min(a, b), Swift.max(a, b))
        guard hi - lo <= 1_000_000 else {
            throw EngineError.domainError(message: "networkdays spans too many days")
        }
        let holidays = Set(try args.dropFirst(2).map { try serialArgument($0, "networkdays holiday") })
        var count = 0
        for day in lo...hi where isBusinessDay(day, holidays: holidays) {
            count += 1
        }
        return BigDecimal(a <= b ? count : -count)
    },
]

/// Monday–Friday and not a listed holiday. Serial 0 (1970-01-01) was a
/// Thursday, so Sunday-based weekday = (serial + 4) mod 7.
private func isBusinessDay(_ serial: Int, holidays: Set<Int>) -> Bool {
    let sundayBased = ((serial % 7) + 7 + 4) % 7
    return sundayBased != 0 && sundayBased != 6 && !holidays.contains(serial)
}

/// Splits trailing arguments evenly into (dates, flows).
private func splitDatesAndFlows(_ args: [BigDecimal],
                                function: String) throws -> ([Double], [Double]) {
    guard args.count >= 2, args.count.isMultiple(of: 2) else {
        throw EngineError.domainError(
            message: "\(function) needs matching dates and flows — e.g. \(function)(A:1..A:5, B:1..B:5)")
    }
    let half = args.count / 2
    let dates = args[..<half].map(\.doubleValue)
    let flows = args[half...].map(\.doubleValue)
    return (dates, flows)
}

private func xnpvValue(rate: Double, dates: [Double], flows: [Double]) -> Double {
    let t0 = dates[0]
    return zip(dates, flows).reduce(0) { total, item in
        total + item.1 / pow(1 + rate, (item.0 - t0) / 365)
    }
}
