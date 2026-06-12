import Testing
@testable import Anzan
@testable import SorobanEngine

/// Evaluates in a fresh calculator and returns the canonical rendering.
private func eval(_ source: String, _ calc: Calculator = Calculator()) throws -> String {
    try calc.evaluate(source).get().description
}

@Suite("Structured values")
struct StructureTests {
    @Test func arrayLiteralsAndIndexing() throws {
        #expect(try eval("[1, 2, 3]") == "[1, 2, 3]")
        #expect(try eval("[]") == "[]")
        #expect(try eval("[1, 2, 3][0]") == "1")  // 0-based
        #expect(try eval("[1, 2, 3][2]") == "3")
        #expect(try eval("[[1, 2], [3, 4]][1][0]") == "3") // chained
        #expect(try eval("[1 + 1, 2 * 3]") == "[2, 6]")    // elements evaluate
    }

    @Test func indexingErrors() throws {
        let calc = Calculator()
        #expect(calc.evaluate("[1, 2][2]").isFailure)   // out of range
        #expect(calc.evaluate("[1, 2][-1]").isFailure)  // negative
        #expect(calc.evaluate("[1, 2][0.5]").isFailure) // non-integer
        #expect(calc.evaluate("5[0]").isFailure)        // numbers aren't indexable
    }

    @Test func mapLiteralsAndAccess() throws {
        #expect(try eval("{name: \"Ada\", age: 36}.age") == "36")
        #expect(try eval("{name: \"Ada\"}.name") == "\"Ada\"")
        #expect(try eval("{a: 1, b: 2}[\"b\"]") == "2")
        #expect(try eval("{}") == "{}")

        let calc = Calculator()
        #expect(calc.evaluate("{a: 1}.b").isFailure)      // missing key
        #expect(calc.evaluate("{a: 1}[0]").isFailure)     // keys are strings
        #expect(calc.evaluate("{a: 1, a: 2}").isFailure)  // duplicate key
        #expect(calc.evaluate("[1].x").isFailure)         // member needs a map
    }

    @Test func compactMapKeysDecomposeFromCellRefTokens() throws {
        // {b:1} lexes the key+value as a cell-reference token; the map
        // literal decomposes it back. Key case is preserved.
        #expect(try eval("{b:1}") == "{b: 1}")
        #expect(try eval("{b:1, c:2}.c") == "2")
        #expect(try eval("{price:99}.price") == "99")
    }

    @Test func nestingAndCanonicalRendering() throws {
        let people = "people = [{name: \"Bob\", age: 32}, {name: \"Ada\", age: 36}]"
        let calc = Calculator()
        _ = try calc.evaluate(people).get()
        #expect(try eval("people[1].age", calc) == "36")
        #expect(try eval("people[0].name", calc) == "\"Bob\"")
        // Canonical rendering re-parses to an equal value.
        let rendered = try eval("people", calc)
        let reparsed = try calc.evaluate(rendered).get()
        #expect(reparsed == (try calc.evaluate("people").get()))
    }

    @Test func numericFunctionsFlattenArraysLikeRanges() throws {
        #expect(try eval("sum([1, 2, 3])") == "6")
        #expect(try eval("avg([1, 2, 3])") == "2")
        #expect(try eval("max([1, 5], 3)") == "5")     // mixes with scalars
        #expect(try eval("sum([[1, 2], [3]])") == "6") // recursive flatten

        let calc = Calculator()
        #expect(calc.evaluate("sum([\"a\"])").isFailure)        // strings don't coerce
        #expect(calc.evaluate("sum([{a: 1}])").isFailure)       // maps don't coerce
        #expect(calc.evaluate("sqrt([4, 9])").isFailure)        // flattened arity = 2
    }

    @Test func structureBuiltins() throws {
        #expect(try eval("len([1, 2, 3])") == "3")
        #expect(try eval("len({a: 1})") == "1")
        #expect(try eval("len(\"hello\")") == "5")
        #expect(try eval("first([5, 6, 7])") == "5")
        #expect(try eval("last([5, 6, 7])") == "7")
        #expect(try eval("keys({name: \"Ada\", age: 36})") == "[\"name\", \"age\"]")
        #expect(try eval("values({a: 1, b: 2})") == "[1, 2]")
        #expect(try eval("sum(values({a: 1, b: 2}))") == "3")

        let calc = Calculator()
        #expect(calc.evaluate("first([])").isFailure)
        #expect(calc.evaluate("len(5)").isFailure)
        #expect(calc.evaluate("keys([1])").isFailure)
    }

    @Test func equalityIsDeepAndOrderingIsNumeric() throws {
        #expect(try eval("[1, 2] == [1, 2]") == "1")
        #expect(try eval("[1, 2] == [2, 1]") == "0")
        #expect(try eval("{a: 1, b: 2} == {b: 2, a: 1}") == "1") // order-insensitive
        #expect(try eval("\"x\" == \"x\"") == "1")
        #expect(try eval("\"x\" != 5") == "1") // different kinds are unequal, not errors

        let calc = Calculator()
        #expect(calc.evaluate("[1] < [2]").isFailure) // ordering needs numbers
        #expect(calc.evaluate("\"a\" < \"b\"").isFailure)
    }

    @Test func userFunctionsTakeAndReturnStructures() throws {
        let calc = Calculator()
        _ = try calc.evaluate("total(m) = m.price * m.qty").get()
        #expect(try eval("total({price: 9.5, qty: 4})", calc) == "38")

        _ = try calc.evaluate("pair(a, b) = [a, b]").get()
        #expect(try eval("pair(1, 2)[1]", calc) == "2")
        #expect(try eval("∑_i=0^2(pair(i, 10)[0])", calc) == "3") // composes with ∑

        // Arity counts the array as ONE argument for user functions.
        #expect(calc.evaluate("total([1, 2])").isFailure) // .price of an array
    }

    @Test func variablesAndAnsHoldStructures() throws {
        let calc = Calculator()
        _ = try calc.evaluate("arr = [1, 2, 3]").get()
        #expect(try eval("arr[1] + last(arr)", calc) == "5")
        _ = try calc.evaluate("[10, 20]").get()
        #expect(try eval("ans[1]", calc) == "20") // ans carries structures
    }

    @Test func reductionsRequireNumericParts() throws {
        let calc = Calculator()
        _ = try calc.evaluate("arr = [1, 2, 3]").get()
        #expect(try eval("∑_i=0^2(arr[i])", calc) == "6") // idiomatic iteration
        #expect(calc.evaluate("∑_i=1^3(\"a\")").isFailure)
        #expect(calc.evaluate("if([1], 1, 2)").isFailure) // condition must be numeric
    }
}

@Suite("String values")
struct StringValueTests {
    @Test func literalsAndEscapes() throws {
        #expect(try eval("\"hello\"") == "\"hello\"")
        #expect(try eval("\"say \\\"hi\\\"\"") == "\"say \\\"hi\\\"\"")
        #expect(try eval("len(\"a\\nb\")") == "3")

        #expect(throws: EngineError.self) { try Parser.parse("\"unterminated") }
        #expect(throws: EngineError.self) { try Parser.parse("\"bad \\q escape\"") }
    }

    @Test func plusConcatenates() throws {
        #expect(try eval("\"Q\" + 1") == "\"Q1\"")
        #expect(try eval("1 + \"Q\"") == "\"1Q\"")
        #expect(try eval("\"a\" + \"b\" + \"c\"") == "\"abc\"")
        #expect(try eval("concat(\"Q\", 1, \"-\", 2026)") == "\"Q1-2026\"")
        #expect(try eval("concat([1, 2], [3])") == "[1, 2, 3]") // all-array form

        // Every other operator stays numeric.
        let calc = Calculator()
        #expect(calc.evaluate("\"a\" * 2").isFailure)
        #expect(calc.evaluate("\"a\" - \"b\"").isFailure)
        #expect(calc.evaluate("-\"a\"").isFailure)
    }

    @Test func stringIndexingIsZeroBased() throws {
        #expect(try eval("\"abc\"[0]") == "\"a\"")
        #expect(try eval("\"abc\"[2]") == "\"c\"")
        #expect(Calculator().evaluate("\"abc\"[3]").isFailure)
    }

    @Test func trueAndFalseAreConstants() throws {
        #expect(try eval("true") == "1")
        #expect(try eval("false") == "0")
        #expect(try eval("if(true, 10, 20)") == "10")
        #expect(Calculator().evaluate("true = 5").isFailure) // reserved
    }

    @Test func gridPolicy() throws {
        // Cells hold scalars: strings render as text, structures error.
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        calc.rangeResolver = { [weak sheet] _, fc, fr, tc, tr in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValues(fromColumn: fc, fromRow: fr, toColumn: tc, toRow: tr)
        }
        defer { _ = sheet } // resolvers capture weakly

        _ = try calc.evaluate("quarter = 1").get()
        sheet.setCell("=\"Q\" + quarter", at: CellAddress(column: 0, row: 0))
        sheet.setCell("=[1, 2, 3]", at: CellAddress(column: 0, row: 1))
        sheet.setCell("={a: 1}", at: CellAddress(column: 0, row: 2))
        sheet.setCell("100", at: CellAddress(column: 1, row: 0))

        #expect(sheet.displayValue(at: CellAddress(column: 0, row: 0)) == .text("Q1"))
        guard case .error = sheet.displayValue(at: CellAddress(column: 0, row: 1)) else {
            Issue.record("array in a cell should be an error"); return
        }
        guard case .error = sheet.displayValue(at: CellAddress(column: 0, row: 2)) else {
            Issue.record("map in a cell should be an error"); return
        }

        // A string-valued formula behaves like text when referenced:
        // direct numeric use errors, ranges skip it.
        #expect(calc.evaluate("A:1 * 2").isFailure)
        #expect(try calc.evaluate("sum(A:1..B:1)").get() == .value(BigDecimal(100)))
    }

    @Test func structuredVariablesRoundTripThroughWorkbooks() throws {
        let calc = Calculator()
        _ = try calc.evaluate("arr = [1, 2, [3, 4]]").get()
        _ = try calc.evaluate("person = {name: \"Ada\", age: 36}").get()
        _ = try calc.evaluate("label = \"Q1 \\\"final\\\"\"").get()

        let workbook = Workbook(cells: [:], variables: calc.environment.userVariables)
        let decoded = try Workbook.decode(try workbook.encode())
        let restored = decoded.parsedVariables
        #expect(restored["arr"] == calc.environment["arr"])
        #expect(restored["person"] == calc.environment["person"])
        #expect(restored["label"] == calc.environment["label"])
    }
}
