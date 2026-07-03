import Testing
@testable import Anzan
@testable import SorobanEngine

private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
    try calc.evaluate(source).get().description
}

@Suite("Higher-order functions")
struct HigherOrderTests {
    @Test func mapWithLambdasAndNames() throws {
        #expect(try eval("map(sqrt, [1, 4, 9])") == "[1, 2, 3]")     // builtin by name
        #expect(try eval("map(x -> x.price, [{price: 5}, {price: 7}])") == "[5, 7]")

        let calc = Calculator()
        _ = try calc.evaluate("double(x) = x * 2").get()
        #expect(try eval("map(double, [1, 2])", calc) == "[2, 4]")   // user fn by name
        #expect(try eval("sum(map(x -> x ^ 2, [1, 2, 3]))", calc) == "14") // composes
    }

    @Test func filterAndReduce() throws {
        #expect(try eval("filter(x -> x > 1, [1, 2, 3])") == "[2, 3]")
        #expect(try eval("filter(x -> mod(x, 2) == 0, [1, 2, 3, 4])") == "[2, 4]")
        #expect(try eval("reduce((a, b) -> a + b, [1, 2, 3], 0)") == "6")
        #expect(try eval("reduce((a, b) -> a * b, [1, 2, 3, 4], 1)") == "24")
        #expect(try eval("reduce((a, b) -> a + b, [], 42)") == "42") // empty → initial
        // The accumulator can be structured.
        #expect(try eval("reduce((acc, x) -> concat(acc, [x * 10]), [1, 2], [])") == "[10, 20]")
    }

    @Test func lambdasAreValues() throws {
        let calc = Calculator()
        _ = try calc.evaluate("f = x -> x * 2").get()
        #expect(try eval("f(21)", calc) == "42")               // call through a variable
        #expect(try eval("map(f, [1, 2])", calc) == "[2, 4]")  // pass it on
        #expect(try eval("f", calc) == "(x) -> (x * 2)")       // canonical, re-parseable
        let rendered = try eval("f", calc)
        _ = try calc.evaluate("g = \(rendered)").get()
        #expect(try eval("g(5)", calc) == "10")
    }

    @Test func zeroParamAndMultiParam() throws {
        let calc = Calculator()
        _ = try calc.evaluate("seven = () -> 7").get()
        #expect(try eval("seven()", calc) == "7")
        _ = try calc.evaluate("add = (a, b) -> a + b").get()
        #expect(try eval("add(2, 3)", calc) == "5")
        #expect(try eval("reduce(add, [1, 2, 3], 10)", calc) == "16")
    }

    @Test func closuresCaptureLocals() throws {
        let calc = Calculator()
        // n is a LOCAL of scale(); the lambda must carry it into map().
        _ = try calc.evaluate("scale(arr, n) = map(x -> x * n, arr)").get()
        #expect(try eval("scale([1, 2, 3], 10)", calc) == "[10, 20, 30]")
        // Captures snapshot at creation; parameters shadow captures.
        _ = try calc.evaluate("make(n) = x -> x + n").get()
        _ = try calc.evaluate("addFive = make(5)").get()
        #expect(try eval("addFive(1)", calc) == "6")
    }

    @Test func namedReferencesFollowRedefinition() throws {
        let calc = Calculator()
        _ = try calc.evaluate("h(x) = x + 1").get()
        _ = try calc.evaluate("alias = h").get()
        _ = try calc.evaluate("h(x) = x + 100").get() // redefine
        #expect(try eval("alias(1)", calc) == "101")  // by-name: follows
    }

    @Test func errorsAreTyped() throws {
        let calc = Calculator()
        #expect(calc.evaluate("map(5, [1])").isFailure)            // not a function
        #expect(calc.evaluate("map(x -> x, 5)").isFailure)         // not an array
        #expect(calc.evaluate("filter(x -> \"a\", [1])").isFailure) // predicate not numeric
        #expect(calc.evaluate("nope(1)").isFailure)                 // still unknown
        #expect(calc.evaluate("sum(x -> x)").isFailure)             // functions aren't numbers
        #expect(calc.evaluate("(x -> x)[0]").isFailure)             // not indexable
        _ = try calc.evaluate("f = x -> x").get()
        #expect(calc.evaluate("f(1, 2)").isFailure)                 // lambda arity
        // Runaway recursion through variables still hits the depth limit.
        _ = try calc.evaluate("r = x -> r(x)").get()
        #expect(calc.evaluate("r(1)").isFailure)
    }

    @Test func cellsRejectFunctionValues() throws {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        sheet.setCell("=x -> x * 2", at: CellAddress(column: 0, row: 0))
        guard case .error = sheet.displayValue(at: CellAddress(column: 0, row: 0)) else {
            Issue.record("a function value in a cell should be an error"); return
        }
        _ = sheet // keep alive
    }
}
