import Foundation

/// Time-value-of-money functions following the spreadsheet sign convention:
/// money you pay out is negative, money you receive is positive, payments due
/// at the end of each period.
///
/// `pv`, `fv`, `pmt`, `nper`, and `npv` are computed in BigDecimal (exact
/// powers, working-precision division). `rate` and `irr` are iterative
/// root-finding and run in the Double domain (~15 significant digits), which
/// is far inside any real-world rate tolerance.
let financeFunctions: [BuiltinFunction] = [
    // pmt(rate, nper, pv[, fv]) — periodic payment for a loan/annuity.
    BuiltinFunction(name: "pmt", category: .finance,
                    signature: "pmt(rate, nper, pv, fv = 0)",
                    summary: "Periodic payment for a loan or annuity (spreadsheet sign convention: money you pay out is negative).",
                    examples: ["pmt(0.05/12, 360, 200000)", "pmt(0, 12, 1200)"],
                    arity: 3...4) { args in
        let (rate, pv) = (args[0], args[2])
        let nper = try requireInt(args[1], "pmt nper")
        let fv = args.count > 3 ? args[3] : .zero
        if rate.isZero {
            return try -(pv + fv) / BigDecimal(nper)
        }
        let growth = try (BigDecimal.one + rate).power(nper)
        return try -(pv * growth + fv) * rate / (growth - .one)
    },

    // fv(rate, nper, pmt[, pv]) — future value.
    BuiltinFunction(name: "fv", category: .finance,
                    signature: "fv(rate, nper, pmt, pv = 0)",
                    summary: "Future value of regular payments (and an optional starting amount).",
                    examples: ["fv(0.06/12, 120, -100)"],
                    arity: 3...4) { args in
        let (rate, pmt) = (args[0], args[2])
        let nper = try requireInt(args[1], "fv nper")
        let pv = args.count > 3 ? args[3] : .zero
        if rate.isZero {
            return -(pv + pmt * BigDecimal(nper))
        }
        let growth = try (BigDecimal.one + rate).power(nper)
        return try -(pv * growth + pmt * ((growth - .one) / rate))
    },

    // pv(rate, nper, pmt[, fv]) — present value.
    BuiltinFunction(name: "pv", category: .finance,
                    signature: "pv(rate, nper, pmt, fv = 0)",
                    summary: "Present value of a stream of payments.",
                    examples: ["pv(0.04/12, 60, -500)"],
                    arity: 3...4) { args in
        let (rate, pmt) = (args[0], args[2])
        let nper = try requireInt(args[1], "pv nper")
        let fv = args.count > 3 ? args[3] : .zero
        if rate.isZero {
            return -(fv + pmt * BigDecimal(nper))
        }
        let growth = try (BigDecimal.one + rate).power(nper)
        return try -(pmt * ((growth - .one) / rate) + fv) / growth
    },

    // nper(rate, pmt, pv[, fv]) — number of periods.
    BuiltinFunction(name: "nper", category: .finance,
                    signature: "nper(rate, pmt, pv, fv = 0)",
                    summary: "Number of periods needed to pay off pv with the given payment.",
                    examples: ["nper(0.05/12, -1073.64, 200000)"],
                    arity: 3...4) { args in
        let (rate, pmt, pv) = (args[0].doubleValue, args[1].doubleValue, args[2].doubleValue)
        let fv = args.count > 3 ? args[3].doubleValue : 0
        if rate == 0 {
            guard pmt != 0 else { throw EngineError.domainError(message: "nper: pmt cannot be 0 when rate is 0") }
            guard let result = BigDecimal(-(pv + fv) / pmt) else {
                throw EngineError.domainError(message: "nper is undefined for these values")
            }
            return result
        }
        let numerator = pmt - fv * rate
        let denominator = pmt + pv * rate
        guard numerator / denominator > 0 else {
            throw EngineError.domainError(message: "nper is undefined for these values")
        }
        guard let result = BigDecimal(log(numerator / denominator) / log1p(rate)) else {
            throw EngineError.domainError(message: "nper is undefined for these values")
        }
        return result
    },

    // rate(nper, pmt, pv[, fv]) — periodic interest rate, found numerically.
    BuiltinFunction(name: "rate", category: .finance,
                    signature: "rate(nper, pmt, pv, fv = 0)",
                    summary: "Periodic interest rate, found numerically (Newton + bisection).",
                    examples: ["rate(360, -1073.64, 200000) * 12"],
                    arity: 3...4) { args in
        let nper = Double(try requireInt(args[0], "rate nper"))
        let (pmt, pv) = (args[1].doubleValue, args[2].doubleValue)
        let fv = args.count > 3 ? args[3].doubleValue : 0
        let value = try solveRate(domain: "rate") { r in
            if abs(r) < 1e-14 {
                return pv + pmt * nper + fv
            }
            let growth = Foundation.pow(1 + r, nper)
            return pv * growth + pmt * (growth - 1) / r + fv
        }
        return value
    },

    // npv(rate, cashflow1, cashflow2, ...) — flows at the END of periods 1..n.
    BuiltinFunction(name: "npv", category: .finance,
                    signature: "npv(rate, flow1, flow2, …)",
                    summary: "Net present value of cash flows at the END of periods 1, 2, … Accepts ranges.",
                    examples: ["npv(0.1, 3000, 4200, 6800)"],
                    arity: 2...Int.max) { args in
        let rate = args[0]
        let onePlus = BigDecimal.one + rate
        var total = BigDecimal.zero
        for (i, flow) in args.dropFirst().enumerated() {
            total = try total + flow / onePlus.power(i + 1)
        }
        return total
    },

    // irr(cashflow0, cashflow1, ...) — rate where NPV (flow 0 at t=0) is zero.
    BuiltinFunction(name: "irr", category: .finance,
                    signature: "irr(flow0, flow1, …)",
                    summary: "Internal rate of return; flow 0 happens today. Needs at least one inflow and one outflow.",
                    examples: ["irr(-70000, 12000, 15000, 18000, 21000, 26000)"],
                    arity: 2...Int.max) { args in
        let flows = args.map(\.doubleValue)
        guard flows.contains(where: { $0 > 0 }), flows.contains(where: { $0 < 0 }) else {
            throw EngineError.domainError(message: "irr needs both positive and negative cash flows")
        }
        return try solveRate(domain: "irr") { r in
            flows.enumerated().reduce(0) { sum, item in
                sum + item.element / Foundation.pow(1 + r, Double(item.offset))
            }
        }
    },

    // effectiveRate(nominal, periodsPerYear) — APR → effective annual rate.
    BuiltinFunction(name: "effectiveRate", category: .finance,
                    signature: "effectiveRate(nominal, periodsPerYear)",
                    summary: "Effective annual rate of a nominal APR compounded n times per year.",
                    examples: ["effectiveRate(0.06, 12)"],
                    arity: 2...2) { args in
        let m = try requireInt(args[1], "effectiveRate periods")
        guard m > 0 else {
            throw EngineError.domainError(message: "effectiveRate periods must be positive")
        }
        let perPeriod = try args[0] / BigDecimal(m)
        return try (BigDecimal.one + perPeriod).power(m) - .one
    },

    // nominal(effective, periodsPerYear) — effectiveRate's inverse.
    BuiltinFunction(name: "nominal", category: .finance,
                    signature: "nominal(effective, periodsPerYear)",
                    summary: "Nominal APR behind an effective annual rate compounded n times per year — effectiveRate's inverse.",
                    examples: ["nominal(0.0617, 12)"],
                    arity: 2...2) { args in
        let m = try requireInt(args[1], "nominal periods")
        guard m > 0 else {
            throw EngineError.domainError(message: "nominal periods must be positive")
        }
        guard args[0] > BigDecimal(-1) else {
            throw EngineError.domainError(message: "nominal needs an effective rate above -100%")
        }
        let perPeriod = try Functions.pow(.one + args[0], .one / BigDecimal(m)) - .one
        return perPeriod * BigDecimal(m)
    },

    // ipmt(rate, per, nper, pv[, fv]) — the interest share of payment `per`.
    BuiltinFunction(name: "ipmt", category: .finance,
                    signature: "ipmt(rate, per, nper, pv, fv = 0)",
                    summary: "Interest portion of the payment in period `per` (1-based) — pairs with ppmt to build amortization tables. Spreadsheet sign convention.",
                    examples: ["ipmt(0.05/12, 1, 360, 200000)", "ipmt(0.05/12, 360, 360, 200000)"],
                    arity: 4...5) { args in
        let split = try amortizationSplit(args, name: "ipmt")
        return split.interest
    },

    // ppmt(rate, per, nper, pv[, fv]) — the principal share.
    BuiltinFunction(name: "ppmt", category: .finance,
                    signature: "ppmt(rate, per, nper, pv, fv = 0)",
                    summary: "Principal portion of the payment in period `per` (1-based); ipmt + ppmt = pmt every period.",
                    examples: ["ppmt(0.05/12, 1, 360, 200000)"],
                    arity: 4...5) { args in
        let split = try amortizationSplit(args, name: "ppmt")
        return split.principal
    },

    // cumipmt(rate, nper, pv, startPer, endPer) — interest paid over a span.
    BuiltinFunction(name: "cumipmt", category: .finance,
                    signature: "cumipmt(rate, nper, pv, start, end)",
                    summary: "Total interest paid between periods start and end (inclusive, 1-based) — what a year of a mortgage costs in interest.",
                    examples: ["cumipmt(0.05/12, 360, 200000, 1, 12)"],
                    arity: 5...5) { args in
        try cumulative(args, name: "cumipmt").interest
    },

    BuiltinFunction(name: "cumprinc", category: .finance,
                    signature: "cumprinc(rate, nper, pv, start, end)",
                    summary: "Total principal paid between periods start and end (inclusive, 1-based).",
                    examples: ["cumprinc(0.05/12, 360, 200000, 1, 12)"],
                    arity: 5...5) { args in
        try cumulative(args, name: "cumprinc").principal
    },

    // Depreciation — the accounting trio.
    BuiltinFunction(name: "sln", category: .accounting,
                    signature: "sln(cost, salvage, life)",
                    summary: "Straight-line depreciation per period.",
                    examples: ["sln(30000, 7500, 10)"],
                    arity: 3...3) { args in
        guard !args[2].isZero else {
            throw EngineError.domainError(message: "sln life can't be 0")
        }
        return try (args[0] - args[1]) / args[2]
    },

    BuiltinFunction(name: "syd", category: .accounting,
                    signature: "syd(cost, salvage, life, per)",
                    summary: "Sum-of-years'-digits depreciation for period `per`.",
                    examples: ["syd(30000, 7500, 10, 1)", "syd(30000, 7500, 10, 10)"],
                    arity: 4...4) { args in
        let life = try requireInt(args[2], "syd life")
        let per = try requireInt(args[3], "syd per")
        guard life > 0, (1...life).contains(per) else {
            throw EngineError.domainError(message: "syd needs 1 ≤ per ≤ life")
        }
        let digits = try BigDecimal(life * (life + 1)) / BigDecimal(2)
        return try (args[0] - args[1]) * BigDecimal(life - per + 1) / digits
    },

    BuiltinFunction(name: "ddb", category: .accounting,
                    signature: "ddb(cost, salvage, life, per, factor = 2)",
                    summary: "Declining-balance depreciation for period `per` (factor 2 = double-declining). Never depreciates below salvage.",
                    examples: ["ddb(30000, 7500, 10, 1)", "ddb(30000, 7500, 10, 10)"],
                    arity: 4...5) { args in
        let life = try requireInt(args[2], "ddb life")
        let per = try requireInt(args[3], "ddb per")
        let factor = args.count > 4 ? args[4] : BigDecimal(2)
        guard life > 0, (1...life).contains(per) else {
            throw EngineError.domainError(message: "ddb needs 1 ≤ per ≤ life")
        }
        let rate = try factor / BigDecimal(life)
        var book = args[0]
        var depreciation = BigDecimal.zero
        for _ in 1...per {
            depreciation = book * rate
            // Never depreciate below salvage value.
            if book - depreciation < args[1] {
                depreciation = max(book - args[1], .zero)
            }
            book = book - depreciation
        }
        return depreciation
    },
]

// MARK: - Amortization helpers (exact powers, working-precision division)

/// The interest/principal split of one payment: balance entering the period
/// times the rate is the interest; the rest of the payment is principal.
private func amortizationSplit(
    _ args: [BigDecimal], name: String
) throws -> (interest: BigDecimal, principal: BigDecimal) {
    let rate = args[0]
    let per = try requireInt(args[1], "\(name) per")
    let nper = try requireInt(args[2], "\(name) nper")
    let pv = args[3]
    let fv = args.count > 4 ? args[4] : .zero
    guard nper >= 1, (1...nper).contains(per) else {
        throw EngineError.domainError(message: "\(name) needs 1 ≤ per ≤ nper")
    }
    let payment = try paymentAmount(rate: rate, nper: nper, pv: pv, fv: fv)
    if rate.isZero {
        return (.zero, payment)
    }
    // Balance after per−1 payments: pv·g + pmt·(g−1)/r, g = (1+r)^(per−1).
    let growth = try (BigDecimal.one + rate).power(per - 1)
    let balance = try pv * growth + payment * ((growth - .one) / rate)
    let interest = -(balance * rate)
    return (interest, payment - interest)
}

/// Sums the splits over start...end with a running balance — one pass, no
/// per-period power.
private func cumulative(
    _ args: [BigDecimal], name: String
) throws -> (interest: BigDecimal, principal: BigDecimal) {
    let rate = args[0]
    let nper = try requireInt(args[1], "\(name) nper")
    let pv = args[2]
    let start = try requireInt(args[3], "\(name) start")
    let end = try requireInt(args[4], "\(name) end")
    guard nper <= 100_000 else {
        throw EngineError.domainError(message: "\(name) nper is too large")
    }
    guard 1 <= start, start <= end, end <= nper else {
        throw EngineError.domainError(message: "\(name) needs 1 ≤ start ≤ end ≤ nper")
    }
    let payment = try paymentAmount(rate: rate, nper: nper, pv: pv, fv: .zero)
    var balance = pv
    var (interest, principal) = (BigDecimal.zero, BigDecimal.zero)
    for period in 1...end {
        let owed = balance * rate
        if period >= start {
            interest = interest - owed
            principal = principal + payment + owed
        }
        balance = balance + payment + owed
    }
    return (interest, principal)
}

/// pmt's formula, shared by the amortization functions.
private func paymentAmount(rate: BigDecimal, nper: Int,
                           pv: BigDecimal, fv: BigDecimal) throws -> BigDecimal {
    guard nper > 0 else {
        throw EngineError.domainError(message: "nper must be positive")
    }
    if rate.isZero {
        return try -(pv + fv) / BigDecimal(nper)
    }
    let growth = try (BigDecimal.one + rate).power(nper)
    return try -(pv * growth + fv) * rate / (growth - .one)
}

/// Newton–Raphson with numeric derivative, falling back to bisection over a
/// bracketing scan when Newton diverges. Roots below -100% are rejected.
/// Shared with xirr (DateFunctions.swift).
func solveRate(domain: String, _ f: (Double) -> Double) throws -> BigDecimal {
    let tolerance = 1e-12

    // Newton from a conventional starting guess.
    var r = 0.1
    for _ in 0..<60 {
        let value = f(r)
        if abs(value) < tolerance, r > -1 {
            guard let result = BigDecimal(r) else { break }
            return result
        }
        let h = max(abs(r), 1e-4) * 1e-7
        let slope = (f(r + h) - f(r - h)) / (2 * h)
        guard slope.isFinite, slope != 0 else { break }
        let next = r - value / slope
        guard next.isFinite, next > -1 else { break }
        r = next
    }

    // Bisection: scan for a sign change between -99.99% and 1000%.
    var lo = -0.9999
    var hi = lo
    var fLo = f(lo)
    var found = false
    while hi < 10 {
        hi = min(hi + 0.1, 10)
        let fHi = f(hi)
        if fLo.isFinite, fHi.isFinite, fLo.sign != fHi.sign {
            found = true
            break
        }
        (lo, fLo) = (hi, fHi)
    }
    guard found else {
        throw EngineError.domainError(message: "\(domain) did not converge")
    }
    for _ in 0..<200 {
        let mid = (lo + hi) / 2
        let fMid = f(mid)
        if abs(fMid) < tolerance || (hi - lo) / 2 < 1e-15 {
            guard let result = BigDecimal(mid) else {
                throw EngineError.domainError(message: "\(domain) did not converge")
            }
            return result
        }
        if fMid.sign == fLo.sign {
            (lo, fLo) = (mid, fMid)
        } else {
            hi = mid
        }
    }
    throw EngineError.domainError(message: "\(domain) did not converge")
}
