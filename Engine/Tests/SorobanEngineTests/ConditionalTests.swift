import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Comparisons and conditionals")
struct ConditionalTests {
    private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
        try calc.evaluate(source).get().description
    }

    @Test(arguments: [
        ("1 < 2", "1"), ("2 < 1", "0"),
        ("2 > 1", "1"), ("1 > 2", "0"),
        ("2 <= 2", "1"), ("3 <= 2", "0"),
        ("2 >= 2", "1"), ("1 >= 2", "0"),
        ("2 == 2", "1"), ("2 == 3", "0"),
        ("2 != 3", "1"), ("2 != 2", "0"),
        ("2 ≤ 2", "1"), ("2 ≥ 3", "0"), ("2 ≠ 3", "1"),     // typographic forms
        ("0.1 + 0.2 == 0.3", "1"),                          // exactness pays off
        ("1 + 1 < 2 * 2", "1"),                             // additive binds tighter
    ])
    func comparisons(source: String, expected: String) throws {
        #expect(try eval(source) == expected)
    }

    @Test func chainsAreRejected() {
        guard case .failure(.parseError(let message, _)) = Calculator().evaluate("1 < 2 < 3") else {
            Issue.record("expected parse error")
            return
        }
        #expect(message.contains("chained"))
    }

    @Test func assignmentStillWorks() throws {
        // Single = is untouched by ==.
        let calc = Calculator()
        #expect(try eval("x = 5", calc) == "5")
        #expect(try eval("x == 5", calc) == "1")
        #expect(try eval("y = x == 5", calc) == "1") // assign a comparison result
    }

    @Test func conditionalBasics() throws {
        #expect(try eval("if(1 < 2, 10, 20)") == "10")
        #expect(try eval("if(1 > 2, 10, 20)") == "20")
        #expect(try eval("if(3, 10, 20)") == "10")          // truthiness: nonzero
        #expect(try eval("if(0, 10, 20)") == "20")
        #expect(try eval("if(1, 2, 3) + 1") == "3")         // composes in expressions
    }

    @Test func branchesAreLazy() throws {
        #expect(try eval("if(1, 2, 1/0)") == "2")           // untaken branch never runs
        #expect(try eval("if(0, sqrt(-1), 7)") == "7")
        #expect(Calculator().evaluate("if(0, 2, 1/0)").isFailure) // taken branch does
    }

    @Test func recursionFinallyTerminates() throws {
        let calc = Calculator()
        _ = calc.evaluate("fact2(n) = if(n <= 1, 1, n * fact2(n - 1))")
        #expect(try eval("fact2(20)", calc) == "2432902008176640000")
        #expect(try eval("fact2(20) - fact(20)", calc) == "0")

        _ = calc.evaluate("fib(n) = if(n < 2, n, fib(n - 1) + fib(n - 2))")
        #expect(try eval("fib(20)", calc) == "6765")

        // Still bounded: runaway recursion errors cleanly.
        _ = calc.evaluate("loop(n) = loop(n + 1)")
        #expect(calc.evaluate("loop(1)").isFailure)
    }

    @Test func booleanHelpers() throws {
        #expect(try eval("not(0)") == "1")
        #expect(try eval("not(5)") == "0")
        #expect(try eval("and(1, 2, 3)") == "1")
        #expect(try eval("and(1, 0)") == "0")
        #expect(try eval("or(0, 0, 4)") == "1")
        #expect(try eval("or(0, 0)") == "0")
        #expect(try eval("and(1 < 2, 2 < 3)") == "1")       // the chain workaround
    }

    @Test func ifIsReservedAndHasArity() {
        let calc = Calculator()
        #expect(calc.evaluate("if = 5").isFailure)
        #expect(calc.evaluate("if(x) = x").isFailure)
        #expect(calc.evaluate("if(1, 2)").isFailure)        // needs all three parts
    }

    @Test func worksInCellsAndReductions() throws {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        sheet.setCell("1500", at: CellAddress(column: 1, row: 0))
        sheet.setCell("if(B:1 > 1000, B:1 * 0.1, 0)", at: CellAddress(column: 1, row: 1))
        #expect(sheet.displayValue(at: CellAddress(column: 1, row: 1)) == .value(BigDecimal(150)))

        // Sum of even numbers 1...10 via if inside ∑.
        #expect(try eval("∑_i=1^10(if(mod(i, 2) == 0, i, 0))", calc) == "30")
    }
}
