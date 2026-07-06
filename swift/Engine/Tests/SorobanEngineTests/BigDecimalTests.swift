import Testing
import BigInt
@testable import Anzan
@testable import SorobanEngine

@Suite("BigDecimal parsing")
struct BigDecimalParsingTests {
    @Test(arguments: [
        ("123", "123"),
        ("-1.5", "-1.5"),
        ("1_000", "1000"),
        ("2.5e-3", "0.0025"),
        ("2.5E3", "2500"),
        ("0.000", "0"),
        (".5", "0.5"),
        ("-0.25", "-0.25"),
    ])
    func parsesLiterals(input: String, expected: String) throws {
        let value = try #require(BigDecimal(string: input))
        #expect(value.description == expected)
    }

    @Test(arguments: ["", "-", "1.2.3", "1e", "abc", "1e2.5"])
    func rejectsMalformed(input: String) {
        #expect(BigDecimal(string: input) == nil)
    }
}

@Suite("BigDecimal arithmetic")
struct BigDecimalArithmeticTests {
    private func num(_ s: String) -> BigDecimal { BigDecimal(string: s)! }

    @Test func decimalAdditionIsExact() {
        // The whole reason this type exists.
        #expect(num("0.1") + num("0.2") == num("0.3"))
    }

    @Test func subtraction() {
        #expect(num("1") - num("0.9") == num("0.1"))
        #expect(num("2.5") - num("2.5") == .zero)
    }

    @Test func multiplicationIsExact() {
        #expect(num("1.5") * num("2.5") == num("3.75"))
        #expect(num("-0.001") * num("1000") == num("-1"))
    }

    @Test func exactDivision() throws {
        #expect(try num("1") / num("4") == num("0.25"))
        #expect(try num("-10") / num("4") == num("-2.5"))
    }

    @Test func repeatingDivisionCarriesWorkingPrecision() throws {
        let third = try num("1") / num("3")
        let digits = third.description.drop(while: { $0 == "0" || $0 == "." })
        #expect(digits.count == 50)
        #expect(digits.allSatisfy { $0 == "3" })
    }

    @Test func divisionByZeroThrows() {
        #expect(throws: EngineError.divisionByZero) {
            try num("1") / .zero
        }
    }

    @Test func moduloMatchesDividendSign() throws {
        #expect(try num("7") % num("3") == num("1"))
        #expect(try num("-7") % num("3") == num("-1"))
        #expect(try num("7.5") % num("2") == num("1.5"))
    }

    @Test func comparisonAcrossExponents() {
        #expect(num("0.5") < num("2"))
        #expect(num("-3") < num("0.001"))
        #expect(num("100") == BigDecimal(significand: Integer(1), exponent: 2))
    }
}

@Suite("BigDecimal rounding")
struct BigDecimalRoundingTests {
    private func num(_ s: String) -> BigDecimal { BigDecimal(string: s)! }

    @Test func roundsToPlaces() {
        #expect(num("2.345").rounded(toPlaces: 2) == num("2.34")) // banker's: half to even
        #expect(num("2.355").rounded(toPlaces: 2) == num("2.36"))
        #expect(num("2.3449").rounded(toPlaces: 2) == num("2.34"))
        #expect(num("-2.345").rounded(toPlaces: 2) == num("-2.34"))
        #expect(num("1234").rounded(toPlaces: -2) == num("1200"))
    }

    @Test func roundsToSignificantDigits() {
        #expect(num("123456").rounded(toSignificantDigits: 3) == num("123000"))
        #expect(num("0.0012349").rounded(toSignificantDigits: 3) == num("0.00123"))
    }
}

@Suite("BigDecimal powers and roots")
struct BigDecimalPowerTests {
    private func num(_ s: String) -> BigDecimal { BigDecimal(string: s)! }

    @Test func integerPowersAreExact() throws {
        #expect(try num("2").power(10) == num("1024"))
        #expect(try num("0.1").power(3) == num("0.001"))
        #expect(try num("-2").power(3) == num("-8"))
        #expect(try num("2").power(-2) == num("0.25"))
        #expect(try num("5").power(0) == .one)
    }

    @Test func zeroToZeroIsUndefined() {
        #expect(throws: EngineError.self) { try BigDecimal.zero.power(0) }
    }

    @Test func hugePowersAreRejected() {
        #expect(throws: EngineError.self) { try num("9").power(999_999_999) }
    }

    @Test func exactSquareRoots() throws {
        #expect(try num("9").squareRoot() == num("3"))
        #expect(try num("2.25").squareRoot() == num("1.5"))
        #expect(try num("0.0001").squareRoot() == num("0.01"))
        #expect(try BigDecimal.zero.squareRoot() == .zero)
    }

    @Test func sqrtTwoToWorkingPrecision() throws {
        let root = try num("2").squareRoot()
        // First 50 digits of sqrt(2).
        #expect(root.description.hasPrefix("1.414213562373095048801688724209698078569671875376"))
    }

    @Test func sqrtOfNegativeThrows() {
        #expect(throws: EngineError.self) { try num("-4").squareRoot() }
    }
}

@Suite("BigDecimal formatting")
struct BigDecimalFormattingTests {
    private func num(_ s: String) -> BigDecimal { BigDecimal(string: s)! }

    @Test func plainFormatting() {
        #expect(num("1500").description == "1500")
        #expect(num("0.030").description == "0.03")
        #expect(num("-12.50").description == "-12.5")
    }

    @Test func scientificForExtremes() {
        #expect(BigDecimal(string: "1e40")!.description == "1e+40")
        #expect(BigDecimal(string: "-2.5e-40")!.description == "-2.5e-40")
    }

    @Test func doubleRoundTrip() throws {
        let value = try #require(BigDecimal(0.25))
        #expect(value == num("0.25"))
        #expect(num("12.5").doubleValue == 12.5)
        #expect(BigDecimal(Double.infinity) == nil)
        #expect(BigDecimal(Double.nan) == nil)
    }
}
