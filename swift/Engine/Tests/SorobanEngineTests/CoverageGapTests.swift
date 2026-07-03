import Testing
@testable import Anzan
@testable import SorobanEngine

/// Targeted tests for branches the coverage report showed as unexercised —
/// mostly error paths and degenerate inputs.
@Suite("Error branches and edge cases")
struct CoverageGapTests {
    private func failure(_ source: String, _ calc: Calculator = Calculator()) -> EngineError? {
        if case .failure(let error) = calc.evaluate(source) { return error }
        return nil
    }

    // MARK: EngineError surface

    @Test func errorPositionsAndDescriptions() {
        #expect(EngineError.lexError(message: "x", position: 4).position == 4)
        #expect(EngineError.parseError(message: "x", position: 7).position == 7)
        #expect(EngineError.divisionByZero.position == nil)
        #expect(EngineError.unknownFunction(name: "f").description == "unknown function 'f'")
        #expect(EngineError.arityMismatch(function: "f", expected: "1", got: 2).description
            == "f() expects 1 argument, got 2")
        #expect(EngineError.arityMismatch(function: "f", expected: "2", got: 1).description
            == "f() expects 2 arguments, got 1")
    }

    @Test func arityDescriptionsInMessages() {
        // round is 1...2 ("1 to 2"); sum is 1...Int.max ("at least 1").
        #expect(failure("round(1, 2, 3)")?.description.contains("1 to 2") == true)
        #expect(failure("sum()")?.description.contains("at least 1") == true)
    }

    // MARK: Calculator string-formula path

    @Test func evaluateFormulaFromString() throws {
        let calc = Calculator()
        #expect(try calc.evaluateFormula("1 + 2").get() == .number(BigDecimal(3)))
        guard case .failure = calc.evaluateFormula("1 +") else {
            Issue.record("expected parse failure")
            return
        }
    }

    // MARK: Core function domain errors

    @Test func coreDomainErrors() {
        #expect(failure("pow(-2, 0.5)")?.description.contains("negative base") == true)
        #expect(failure("pow(10, 99999.5)")?.description.contains("out of range") == true)
        #expect(failure("fact(20001)")?.description.contains("too large") == true)
        #expect(failure("log(1, 8)")?.description.contains("undefined") == true)
        #expect(failure("root(8, 0)")?.description.contains("positive") == true)
        #expect(failure("root(-16, 4)")?.description.contains("even root") == true)
        #expect(failure("gcd(1e30, 2)")?.description.contains("integer") == true)
        #expect(failure("acos(2)") != nil) // Double NaN through the viaDouble seam
    }

    @Test func oddRootsOfNegativesWork() throws {
        #expect(try Calculator().evaluate("root(-32, 5)").get() == .value(BigDecimal(-2)))
    }

    // MARK: Finance degenerate paths

    @Test func financeDegenerateInputs() throws {
        let calc = Calculator()
        // pv at zero rate.
        #expect(try calc.evaluate("pv(0, 12, -100)").get() == .value(BigDecimal(1200)))
        // nper: zero rate requires nonzero pmt.
        #expect(failure("nper(0, 0, 1000)")?.description.contains("pmt cannot be 0") == true)
        // nper: log of a non-positive ratio.
        #expect(failure("nper(0.1, 0, 1000)")?.description.contains("undefined") == true)
        // effectiveRate needs positive periods.
        #expect(failure("effectiveRate(0.06, 0)")?.description.contains("positive") == true)
        // rate: all-positive cash flows have no root.
        #expect(failure("rate(12, 100, 1000)")?.description.contains("converge") == true)
    }

    // MARK: Accounting zero guards

    @Test func accountingZeroGuards() {
        #expect(failure("margin(0, 5)")?.description.contains("price cannot be 0") == true)
        #expect(failure("percentChange(0, 5)")?.description.contains("old value") == true)
    }

    // MARK: AST traversal corners

    @Test func containsCellReferenceTraversesAllShapes() throws {
        #expect(try Parser.parse("-A:1").containsCellReference)
        #expect(try Parser.parse("∑_i=1^3(A:1)").containsCellReference)
        #expect(try Parser.parse("∑_i=A:1^3(i)").containsCellReference)
        #expect(try !Parser.parse("∑_i=1^3(i)").containsCellReference)
        #expect(try Parser.parse("f(x) = A:1 + x").containsCellReference)
        #expect(try !Parser.parse("f(x) = x * 2").containsCellReference)
    }

    // MARK: Spreadsheet corners

    @Test func spreadsheetMemoAndRangeCorners() {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }

        let address = CellAddress(column: 0, row: 0)
        #expect(sheet.raw(at: address) == "")
        sheet.setCell("41 + 1", at: address)
        #expect(sheet.raw(at: address) == "41 + 1")
        // Second read hits the memo (same generation).
        #expect(sheet.displayValue(at: address) == .value(BigDecimal(42)))
        #expect(sheet.displayValue(at: address) == .value(BigDecimal(42)))

        // Row 0 is out of range in the 1-based reference syntax.
        if case .failure(let error) = calc.evaluate("A:0 + 1") {
            #expect(error.description.contains("out of range"))
        } else {
            Issue.record("expected out-of-range failure")
        }
    }
}
