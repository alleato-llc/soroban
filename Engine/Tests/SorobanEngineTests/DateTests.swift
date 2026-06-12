import Testing
import Foundation
@testable import Anzan
@testable import SorobanEngine

@Suite("Dates")
struct DateTests {
    private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
        try calc.evaluate(source).get().description
    }

    @Test func epochAnchorsAndKnownSerials() throws {
        #expect(try eval("date(1970, 1, 1)") == "0")
        #expect(try eval("date(1970, 1, 2)") == "1")
        #expect(try eval("date(1969, 12, 31)") == "-1")
        #expect(try eval("date(2000, 3, 1)") == "11017")     // known anchor
        #expect(try eval("date(2026, 6, 6)") == "20610")
    }

    @Test func civilRoundTripAcrossLeapRules() throws {
        // Leap year (2024), century non-leap (1900), 400-year leap (2000).
        for (y, m, d) in [(2024, 2, 29), (1900, 2, 28), (2000, 2, 29),
                          (2026, 6, 6), (1969, 7, 20), (2100, 12, 31)] {
            let calc = Calculator()
            _ = calc.evaluate("s = date(\(y), \(m), \(d))")
            #expect(try eval("year(s)", calc) == "\(y)")
            #expect(try eval("month(s)", calc) == "\(m)")
            #expect(try eval("day(s)", calc) == "\(d)")
        }
    }

    @Test func invalidDatesAreRejected() {
        let calc = Calculator()
        #expect(calc.evaluate("date(2025, 2, 29)").isFailure) // not a leap year
        #expect(calc.evaluate("date(1900, 2, 29)").isFailure) // century rule
        #expect(calc.evaluate("date(2026, 13, 1)").isFailure)
        #expect(calc.evaluate("date(2026, 0, 1)").isFailure)
        #expect(calc.evaluate("date(2026, 6, 31)").isFailure)
        #expect(calc.evaluate("date(2026, 6, 6.5)").isFailure)
    }

    @Test func dayArithmeticAndComparison() throws {
        // Days in 2025 (not a leap year).
        #expect(try eval("date(2026, 1, 1) - date(2025, 1, 1)") == "365")
        #expect(try eval("days(date(2024, 3, 1), date(2024, 2, 1))") == "29") // leap February
        #expect(try eval("date(2026, 6, 6) > date(2026, 1, 1)") == "1")
    }

    @Test func weekdays() throws {
        #expect(try eval("weekday(date(1970, 1, 1))") == "4")  // Thursday
    }

    @Test func monthArithmeticClampsToMonthEnd() throws {
        let calc = Calculator()
        // Jan 31 + 1 month → Feb 29 in a leap year, Feb 28 otherwise.
        #expect(try eval("edate(date(2024, 1, 31), 1) == date(2024, 2, 29)", calc) == "1")
        #expect(try eval("edate(date(2025, 1, 31), 1) == date(2025, 2, 28)", calc) == "1")
        // Backwards and across years.
        #expect(try eval("edate(date(2026, 1, 15), -2) == date(2025, 11, 15)", calc) == "1")
        #expect(try eval("edate(date(2025, 11, 30), 15) == date(2027, 2, 28)", calc) == "1")
        // eomonth.
        #expect(try eval("eomonth(date(2026, 6, 6), 0) == date(2026, 6, 30)", calc) == "1")
        #expect(try eval("eomonth(date(2024, 1, 15), 1) == date(2024, 2, 29)", calc) == "1")
    }

    @Test func todayIsAValidSerial() throws {
        let calc = Calculator()
        // Round-trips through year/month/day and lands in a sane decade.
        #expect(try eval("year(today()) >= 2026", calc) == "1")
        #expect(try eval("and(month(today()) >= 1, month(today()) <= 12)", calc) == "1")
    }

    @Test func xnpvAndXirrAgainstSpreadsheetValues() throws {
        let calc = Calculator()
        // Classic Excel example: -10000 on 2008-01-01, then four inflows.
        _ = calc.evaluate(
            "v = xirr(date(2008,1,1), date(2008,3,1), date(2008,10,30), date(2009,2,15), date(2009,4,1), -10000, 2750, 4250, 3250, 2750)")
        let xirr = try #require(BigDecimal(string: try eval("v", calc)))
        let xirrDiff = xirr - BigDecimal(string: "0.373")!
        #expect((xirrDiff.isNegative ? -xirrDiff : xirrDiff) < BigDecimal(string: "0.001")!)

        // xnpv at 9%: ≈ 2086.65 (Excel's documented result).
        _ = calc.evaluate(
            "n = xnpv(0.09, date(2008,1,1), date(2008,3,1), date(2008,10,30), date(2009,2,15), date(2009,4,1), -10000, 2750, 4250, 3250, 2750)")
        let xnpv = try #require(BigDecimal(string: try eval("n", calc)))
        let xnpvDiff = xnpv - BigDecimal(string: "2086.65")!
        #expect((xnpvDiff.isNegative ? -xnpvDiff : xnpvDiff) < BigDecimal(string: "0.05")!)
    }

    @Test func xirrErrors() {
        let calc = Calculator()
        // Odd argument count can't split into dates/flows.
        #expect(calc.evaluate("xirr(1, 2, 3)").isFailure)
        // All-positive flows.
        #expect(calc.evaluate("xirr(date(2026,1,1), date(2026,6,1), 100, 200)").isFailure)
    }
}
