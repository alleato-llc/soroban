import Testing
@testable import Anzan
@testable import SorobanEngine

/// Engine-API contracts the Gherkin features can't express: typed error
/// equality, the function-name fallback nuance, the names API. The
/// user-visible input→output pairs live in Features/*.feature.
@Suite("Evaluator")
struct EvaluatorTests {
    private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
        try calc.evaluate(source).get().description
    }

    @Test func variableNamesAreCaseSensitiveButFunctionsAreNot() throws {
        let calc = Calculator()
        #expect(try eval("Rate = 2", calc) == "2")
        #expect(try eval("Rate + 1", calc) == "3")
        // `rate` does NOT find the variable (case-sensitive) — it falls back
        // to the rate() builtin as a function value, which isn't a number.
        #expect(calc.evaluate("rate + 1").isFailure)
        #expect(try eval("MIN(1, 2)", calc) == "1")
    }

    @Test(arguments: [
        ("1 / 0", EngineError.divisionByZero),
        ("y + 1", EngineError.unknownVariable(name: "y")),
        ("nope(1)", EngineError.unknownFunction(name: "nope")),
        ("abs(1, 2)", EngineError.arityMismatch(function: "abs", expected: "1", got: 2)),
        ("sqrt(-1)", EngineError.domainError(message: "sqrt of a negative number")),
        ("ln(0)", EngineError.domainError(message: "ln needs a positive argument")),
        ("fact(1.5)", EngineError.domainError(message: "fact() needs a non-negative integer")),
    ])
    func surfacesErrors(source: String, expected: EngineError) {
        let result = Calculator().evaluate(source)
        guard case .failure(let error) = result else {
            Issue.record("expected \(expected), got success")
            return
        }
        #expect(error == expected)
    }

    @Test func functionNamesAreExposed() {
        let names = Calculator.functionNames
        #expect(names.contains("pmt"))
        #expect(names.contains("abs"))
        #expect(names == names.sorted())
    }
}

extension Result {
    var isFailure: Bool {
        if case .failure = self { return true }
        return false
    }
}
