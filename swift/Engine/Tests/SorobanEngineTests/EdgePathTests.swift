import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

/// Error branches and edge paths the feature suites skip past — kept in one
/// place so the coverage bar (≥ ~94% regions) is enforceable.
@Suite("Edge paths")
struct EdgePathTests {
    @Test func valueEdges() throws {
        // kindName, displayText, cross-kind equality.
        #expect(Value.number(.one).kindName == "a number")
        #expect(Value.string("s").kindName == "a string")
        #expect(Value.array([]).kindName == "an array")
        #expect(Value.map([]).kindName == "a map")
        #expect(Value.function(FunctionValue(kind: .builtin("abs"))).kindName == "a function")
        #expect(Value.number(BigDecimal(2)).displayText == "2")
        #expect(Value.string("raw").displayText == "raw")
        #expect(Value.string("a") != Value.number(.one))
        #expect(Value.map([]) != Value.array([]))
        #expect(Value.map([.init(key: "a", value: .number(.one))])
                != Value.map([.init(key: "b", value: .number(.one))]))
        #expect(Value.number(.one).mapValue(forKey: "x") == nil)

        // Literal parsing: successes and refusals.
        #expect(Value(parsing: "[1, -2]") == .array([.number(BigDecimal(1)),
                                                     .number(BigDecimal(-2))]))
        #expect(Value(parsing: "{a: \"s\"}") != nil)
        #expect(Value(parsing: "abs") == .function(FunctionValue(kind: .builtin("abs"))))
        #expect(Value(parsing: "1 + 1") == nil)        // expressions don't fold
        #expect(Value(parsing: "someUserFn") == nil)   // user refs can't fold
        #expect(Value(parsing: "[1, A:1]") == nil)     // refs don't fold
        #expect(Value(parsing: "((") == nil)           // garbage
        #expect(Value(parsing: "x -> x") != nil)       // lambdas fold capture-free

        // Function value equality branches.
        let lambda = FunctionValue(kind: .lambda(parameters: ["x"], body: .variable("x")))
        #expect(Value.function(lambda) == Value.function(lambda))
        #expect(Value.function(lambda) != Value.function(FunctionValue(kind: .user(name: "f"))))
    }

    @Test func sliderExtractionGuards() throws {
        func info(_ s: String) throws -> SliderInfo? {
            SliderInfo.extract(from: try Parser.parse(s), name: nil)
        }
        #expect(try info("slider(1, 0, 10)") != nil)
        #expect(try info("slider(1, 0)") == nil)            // arity
        #expect(try info("slider(1, 10, 0)") == nil)        // min < max
        #expect(try info("slider(1, 0, 10, 0)") == nil)     // step > 0
        #expect(try info("slider(1, 0, 10, -1)") == nil)
        #expect(try info("stepper(1, 0, 10)") == nil)       // function name mismatch
        #expect(try info("1 + 1") == nil)                   // not a call
        #expect(try info("slider(\"a\", 0, 10)") == nil)    // numeric literals only

        // Drag geometry edges.
        let slider = try #require(try info("slider(0, 0, 10, 1)"))
        #expect(slider.fraction == 0)
        #expect(slider.value(atFraction: 2) == BigDecimal(10)) // clamped
        #expect(slider.value(atFraction: -1) == BigDecimal(0))

        // Rewriting refusals.
        #expect(Control.rewriting("nothing here", toLiteral: "1") == nil)
        #expect(Control.rewriting("slider(A:1, 0, 1)", toLiteral: "1") == nil) // non-literal arg
        #expect(Control.rewriting("'unterminated", toLiteral: "1") == nil)     // lex failure
    }

    @Test func registryEdges() throws {
        // The default applier (no evaluator attached) refuses politely.
        #expect(throws: EngineError.self) {
            _ = try FunctionRegistry.standard.call(
                name: "map",
                arguments: [.function(FunctionValue(kind: .builtin("abs"))),
                            .array([.number(.one)])])
        }
        // Unknown name straight at the registry.
        #expect(throws: EngineError.self) {
            _ = try FunctionRegistry.standard.call(name: "nope", arguments: [])
        }
        // Arity description variants ride through error messages.
        let calc = Calculator()
        guard case .failure(let one) = calc.evaluate("abs(1, 2)") else {
            Issue.record("arity should fail"); return
        }
        #expect("\(one)".contains("expects 1 argument"))
        guard case .failure(let atLeast) = calc.evaluate("concat(1)") else {
            Issue.record("arity should fail"); return
        }
        #expect("\(atLeast)".contains("at least"))
        guard case .failure(let range) = calc.evaluate("round(1, 2, 3)") else {
            Issue.record("arity should fail"); return
        }
        #expect("\(range)".contains("to"))
    }

    @Test func financeAndDateEdges() throws {
        let calc = Calculator()
        // Zero-rate branches.
        #expect(try calc.evaluate("pmt(0, 10, 100)").get() == .value(BigDecimal(-10)))
        #expect(try calc.evaluate("pv(0, 10, -10)").get() == .value(BigDecimal(100)))
        // Solvers that can't solve say so.
        #expect(calc.evaluate("irr(100, 100, 100)").isFailure)   // no sign change
        // Date validation.
        #expect(calc.evaluate("date(2026, 13, 1)").isFailure)
        #expect(calc.evaluate("date(2026, 2, 30)").isFailure)
        #expect(calc.evaluate("weekday(date(2026, 6, 6))").isFailure == false)
        // xnpv/xirr halves must match up.
        #expect(calc.evaluate("xnpv(0.1, 1, 2, 3)").isFailure)   // odd count
    }

    @Test func moreFinanceAndDateBranches() throws {
        let calc = Calculator()
        // Zero-rate branches of the remaining time-value functions.
        #expect(try calc.evaluate("fv(0, 10, -10)").get() == .value(BigDecimal(100)))
        #expect(try calc.evaluate("nper(0, -10, 100)").get() == .value(BigDecimal(10)))
        // Month-end clamping and arithmetic.
        #expect(try calc.evaluate("edate(date(2026, 1, 31), 1) == date(2026, 2, 28)").get()
                == .value(BigDecimal(1)))
        #expect(try calc.evaluate("eomonth(date(2026, 2, 10), 0) == date(2026, 2, 28)").get()
                == .value(BigDecimal(1)))
        #expect(try calc.evaluate("days(date(2026, 3, 1), date(2026, 2, 1))").get()
                == .value(BigDecimal(28)))
        // today() exists and is a plausible serial (> 2020-01-01).
        let today = try #require(try calc.evaluate("today()").get().numericValue)
        #expect(today > BigDecimal(CivilDate.serial(year: 2020, month: 1, day: 1)))
        // xirr needs matching halves and a solvable system.
        #expect(calc.evaluate("xirr(1, 2, 3)").isFailure)
        // npv at rate 0 is a plain sum.
        #expect(try calc.evaluate("npv(0, 10, 20, 30)").get() == .value(BigDecimal(60)))
    }

    @Test func dataFunctionErrorBranches() throws {
        let calc = Calculator()
        #expect(calc.evaluate("first(5)").isFailure)
        #expect(calc.evaluate("last(\"s\")").isFailure)
        #expect(calc.evaluate("last([])").isFailure)
        #expect(calc.evaluate("values(5)").isFailure)
        #expect(calc.evaluate("keys(5)").isFailure)
        #expect(calc.evaluate("map(x -> x, {a: 1})").isFailure)
        #expect(calc.evaluate("filter(x -> x, \"s\")").isFailure)
        #expect(calc.evaluate("reduce(x -> x, 5, 0)").isFailure)
        #expect(calc.evaluate("dropdown(1, \"not an array\")").isFailure)
        #expect(calc.evaluate("checkbox(\"s\")").isFailure) // strings don't coerce
    }

    @Test func namedCellRewritingTokenForms() {
        // Quoted sheet qualifier.
        #expect(NamedCells.rewriting("'My Sheet'!'Rate' * 2", oldName: "rate",
                                     owningSheet: "My Sheet", onOwningSheet: false,
                                     replacement: "B:7")
                == "'My Sheet'!B:7 * 2")
        // Unparseable raws are left alone.
        #expect(NamedCells.rewriting("'unterminated", oldName: "x",
                                     owningSheet: nil, onOwningSheet: true,
                                     replacement: "B:7") == nil)
        // Qualified reference with no owning sheet recorded → no match.
        #expect(NamedCells.rewriting("Budget!'Rate'", oldName: "Rate",
                                     owningSheet: nil, onOwningSheet: false,
                                     replacement: "B:7") == nil)
    }

    @Test func sheetStoreEdges() throws {
        let calc = Calculator()
        let store = SheetStore(calculator: calc)
        // Unknown sheet, for cells AND ranges AND names.
        #expect(calc.evaluate("Nowhere!A:1").isFailure)
        #expect(calc.evaluate("sum(Nowhere!A:1..A:2)").isFailure)
        #expect(calc.evaluate("Nowhere!'Rate'").isFailure)
        // Sheet name validation branches.
        #expect(throws: EngineError.self) {
            try SheetStore.validated(name: "", existing: store.sheets, exceptIndex: nil)
        }
        #expect(throws: EngineError.self) {
            try SheetStore.validated(name: String(repeating: "x", count: 129),
                                     existing: store.sheets, exceptIndex: nil)
        }
        #expect(throws: EngineError.self) {
            try SheetStore.validated(name: "a!b", existing: store.sheets, exceptIndex: nil)
        }
        #expect(throws: EngineError.self) {
            try SheetStore.validated(name: "sheet 1", existing: store.sheets, exceptIndex: nil)
        }
        // Remove-last-sheet guard.
        #expect(throws: EngineError.self) { try store.removeSheet(at: 0) }
    }

    @Test func workbookEdges() throws {
        // Future versions are refused with the right error.
        let future = Data("""
        {"format": "soroban-workbook", "version": 999, "sheets": [], "variables": {}}
        """.utf8)
        #expect(throws: WorkbookError.unsupportedVersion(999)) {
            _ = try Workbook.decode(future)
        }
        #expect(throws: WorkbookError.notAWorkbook) {
            _ = try Workbook.decode(Data("{}".utf8))
        }
        #expect(WorkbookError.notAWorkbook.description.contains("not a Soroban workbook"))
        #expect(WorkbookError.unsupportedVersion(2).description.contains("version 2"))
    }
}
