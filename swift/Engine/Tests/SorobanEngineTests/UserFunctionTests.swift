import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("User-defined functions")
struct UserFunctionTests {
    private func value(_ source: String, _ calc: Calculator) throws -> String {
        try calc.evaluate(source).get().description
    }

    @Test func defineAndCall() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("f(x) = x * 2").get() == .functionDefined(signature: "f(x)"))
        #expect(try value("f(21)", calc) == "42")
        // Definitions don't touch ans.
        _ = calc.evaluate("100")
        _ = calc.evaluate("g(x) = x + 1")
        #expect(calc.environment.ans == .number(BigDecimal(100)))
    }

    @Test func multiParameterAndZeroParameter() throws {
        let calc = Calculator()
        _ = calc.evaluate("area(w, h) = w * h")
        #expect(try value("area(6, 7)", calc) == "42")
        _ = calc.evaluate("answer() = 42")
        #expect(try value("answer()", calc) == "42")
    }

    @Test func compositionAndLateBinding() throws {
        let calc = Calculator()
        // g calls f before f exists — resolved at call time.
        _ = calc.evaluate("g(x) = f(x) + 1")
        #expect(calc.evaluate("g(1)").isFailure) // f not defined yet
        _ = calc.evaluate("f(x) = x * 2")
        #expect(try value("g(20)", calc) == "41")
    }

    @Test func parametersShadowGlobals() throws {
        let calc = Calculator()
        _ = calc.evaluate("x = 100")
        _ = calc.evaluate("f(x) = x + 1")
        #expect(try value("f(1)", calc) == "2")     // param x, not global x
        #expect(try value("x", calc) == "100")      // global untouched
    }

    @Test func callsAreCaseInsensitive() throws {
        let calc = Calculator()
        _ = calc.evaluate("double(x) = x * 2")
        #expect(try value("DOUBLE(4)", calc) == "8")
    }

    @Test func builtinsAreProtectedButFreeNamesWork() throws {
        let calc = Calculator()
        guard case .failure(let error) = calc.evaluate("abs(x) = x") else {
            Issue.record("expected redefinition failure")
            return
        }
        #expect(error == .domainError(message: "'abs' is a built-in function and can't be redefined"))

        // 'add' is NOT a built-in — the user's question — so it just works.
        _ = calc.evaluate("add(a, b) = a + b")
        #expect(try value("add(2, 3)", calc) == "5")

        // Reserved constants can't become functions either.
        #expect(calc.evaluate("pi(x) = x").isFailure)
    }

    @Test func redefiningYourOwnFunctionIsAllowed() throws {
        let calc = Calculator()
        _ = calc.evaluate("f(x) = x * 2")
        _ = calc.evaluate("f(x) = x * 3")
        #expect(try value("f(10)", calc) == "30")
    }

    @Test func arityMismatch() {
        let calc = Calculator()
        _ = calc.evaluate("f(x) = x * 2")
        guard case .failure(let error) = calc.evaluate("f(1, 2)") else {
            Issue.record("expected arity failure")
            return
        }
        #expect(error == .arityMismatch(function: "f", expected: "1", got: 2))
    }

    @Test func recursionIsCutOffCleanly() {
        let calc = Calculator()
        _ = calc.evaluate("f(x) = f(x)")
        guard case .failure(let error) = calc.evaluate("f(1)") else {
            Issue.record("expected recursion failure")
            return
        }
        // The message carries a base-case hint; pin the behavior + the hint.
        #expect("\(error)".contains("nested too deeply"))
        #expect("\(error)".contains("base case"))
    }

    @Test func definitionSyntaxEdgeCases() throws {
        let calc = Calculator()
        // Call-with-assignment isn't a definition — parse error.
        #expect(calc.evaluate("f(2) = 1").isFailure)
        // Duplicate parameters rejected.
        #expect(calc.evaluate("f(x, x) = x").isFailure)
        // Calls still parse normally.
        #expect(try value("max(1, 2)", calc) == "2")
    }

    @Test func cellsCanCallButNotDefine() throws {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }

        _ = calc.evaluate("double(x) = x * 2")
        sheet.setCell("21", at: CellAddress(column: 1, row: 0))
        sheet.setCell("double(B:1)", at: CellAddress(column: 1, row: 1))
        #expect(sheet.displayValue(at: CellAddress(column: 1, row: 1)) == .value(BigDecimal(42)))

        // A definition in a cell is no longer an error — it's a sheet-scoped
        // λ cell (SheetDefinitionTests cover the semantics).
        sheet.setCell("h(x) = 1", at: CellAddress(column: 0, row: 0))
        #expect(sheet.displayValue(at: CellAddress(column: 0, row: 0)) == .definition("λ h(x)"))

        // The explicit = marker stays rejected — definitions are plain.
        sheet.setCell("=g(x) = 1", at: CellAddress(column: 0, row: 1))
        guard case .error = sheet.displayValue(at: CellAddress(column: 0, row: 1)) else {
            Issue.record("expected =definition to be an error")
            return
        }
    }

    @Test func autocompleteIncludesUserFunctions() {
        let calc = Calculator()
        _ = calc.evaluate("payback(x) = x / 12")
        let names = calc.completions(forPrefix: "pay").map(\.name)
        #expect(names.contains("payback"))
    }

    @Test func sourceIsRecordedForPersistence() throws {
        let calc = Calculator()
        _ = calc.evaluate("= f(x) = x * 2") // leading = stripped, like any log line
        #expect(calc.environment.function(named: "f")?.source == "f(x) = x * 2")
    }
}
