import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Summation (∑)")
struct SummationTests {
    private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
        try calc.evaluate(source).get().description
    }

    // MARK: ∑(…) — plain variadic sum

    @Test func plainCallIsVariadicSum() throws {
        #expect(try eval("sigma(2.5, 2.5)") == "5")
        #expect(try eval("∑(42)") == "42")
        #expect(try eval("∑(1 + 1, 2 * 2)") == "6")
    }

    @Test func plainSumOverCells() throws {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        sheet.setCell("1200", at: CellAddress(column: 1, row: 0))
        sheet.setCell("1350", at: CellAddress(column: 1, row: 1))
        #expect(try eval("∑(B:1, B:2)", calc) == "2550")
    }

    @Test func oldFourArgFormIsJustASumNow() {
        // ∑(i, 1, 10, i^2) with undefined i: a plain sum over an unknown
        // variable — pins the redesigned semantics.
        let calc = Calculator()
        guard case .failure(let error) = calc.evaluate("∑(i, 1, 10, i^2)") else {
            Issue.record("expected unknown-variable failure")
            return
        }
        #expect(error == .unknownVariable(name: "i"))
    }

    // MARK: ∑_i=1^10(term) — indexed summation

    @Test func indexedSummation() throws {
        #expect(try eval("∑_i=1^10(i^2)") == "385")
        #expect(try eval("sigma_i=1^10(i)") == "55")       // typeable spelling
        #expect(try eval("∑_k=0^4(2^k)") == "31")          // ^ in the term still powers
        #expect(try eval("∑_i=1^1(i + 5)") == "6")         // single term
        #expect(try eval("∑_i=5^1(i)") == "0")             // empty sum convention
        #expect(try eval("∑_i=-3^3(i)") == "0")            // negative bounds
        #expect(try eval("∑_i=-2^2(i^2)") == "10")
    }

    @Test func boundsAreSignedPrimaries() throws {
        let calc = Calculator()
        _ = calc.evaluate("n = 100")
        #expect(try eval("∑_i=1^n(i)", calc) == "5050")
        // Compound bounds need parens (the plaintext LaTeX braces).
        #expect(try eval("∑_i=(n-1)^(n+1)(i)", calc) == "300")
        #expect(try eval("∑_i=(2^3)^10(i)") == "27")       // 8+9+10
    }

    @Test func nestingCompositionAndScoping() throws {
        #expect(try eval("∑_i=1^3(∑_j=1^i(j))") == "10")
        let calc = Calculator()
        _ = calc.evaluate("triangle(n) = ∑_i=1^n(i)")
        #expect(try eval("triangle(100)", calc) == "5050")
        _ = calc.evaluate("i = 999")
        #expect(try eval("∑_i=1^3(i)", calc) == "6")       // bound index shadows global
        #expect(try eval("i", calc) == "999")
        #expect(try eval("∑_i=1^10(i^2) + 5", calc) == "390") // composes in expressions
    }

    @Test func indexedSummationErrors() {
        let calc = Calculator()
        // Compound lower bound without parens → the hint.
        guard case .failure(.parseError(let message, _)) = calc.evaluate("∑_i=n-1^10(i)") else {
            Issue.record("expected parse error")
            return
        }
        #expect(message.contains("parenthesize"))

        #expect(calc.evaluate("∑_i=1^10 i^2").isFailure)   // term must be parenthesized
        #expect(calc.evaluate("∑_pi=1^2(1)").isFailure)    // reserved index
        #expect(calc.evaluate("∑_i 1^2(1)").isFailure)     // missing =
        #expect(calc.evaluate("∑_i=1.5^3(i)").isFailure)   // non-integer bound
        #expect(calc.evaluate("∑_i=1^99999999(i)").isFailure) // span cap

        // The sigma_ prefix is reserved.
        #expect(calc.evaluate("sigma_total = 5").isFailure)
        #expect(calc.evaluate("sigma_f(x) = 1").isFailure)
        #expect(calc.evaluate("sigma = 5").isFailure)
    }

    @Test func sigmaAppearsInAutocomplete() {
        let names = Calculator().completions(forPrefix: "sig").map(\.name)
        #expect(names.contains("sigma"))
    }
}

@Suite("Product (∏)")
struct ProductTests {
    private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
        try calc.evaluate(source).get().description
    }

    @Test func plainCallIsVariadicProduct() throws {
        #expect(try eval("product(2, 3, 4)") == "24")
        #expect(try eval("∏(1.5, 2)") == "3")
        #expect(try eval("∏(42)") == "42")
    }

    @Test func indexedProduct() throws {
        #expect(try eval("∏_i=1^5(i)") == "120")
        #expect(try eval("product_i=1^5(i)") == "120")     // typeable spelling
        #expect(try eval("∏_i=5^1(i)") == "1")             // empty product = 1
        #expect(try eval("∏_k=1^3(2)") == "8")             // constant terms
    }

    @Test func exactBigFactorials() throws {
        // BigDecimal keeps this exact — well past Double territory.
        #expect(try eval("∏_i=1^25(i)") == "15511210043330985984000000")
        // Consistent with fact().
        #expect(try eval("∏_i=1^20(i) - fact(20)") == "0")
    }

    @Test func compoundGrowthIdiom() throws {
        // ∏ of (1 + r) over n periods — the textbook compound-growth form.
        let calc = Calculator()
        _ = calc.evaluate("r = 0.1")
        #expect(try eval("∏_i=1^3(1 + r)", calc) == "1.331")
    }

    @Test func mixesWithSummation() throws {
        // ∑ of factorials: 1! + 2! + 3! = 9
        #expect(try eval("∑_n=1^3(∏_i=1^n(i))") == "9")
    }

    @Test func productErrors() {
        let calc = Calculator()
        #expect(calc.evaluate("∏_pi=1^2(1)").isFailure)      // reserved index
        #expect(calc.evaluate("∏_i=n-1^10(i)").isFailure)    // compound bound, no parens
        #expect(calc.evaluate("∏_i=1^99999999(i)").isFailure) // span cap
        #expect(calc.evaluate("product_total = 5").isFailure) // reserved prefix
        #expect(calc.evaluate("product_f(x) = 1").isFailure)
        // `product` itself is a registry function — redefinition blocked there.
        #expect(calc.evaluate("product(x) = x").isFailure)
    }
}
