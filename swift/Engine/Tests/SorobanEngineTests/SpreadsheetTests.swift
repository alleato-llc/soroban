import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Cell reference syntax")
struct CellReferenceSyntaxTests {
    @Test func lexesCellReferences() throws {
        // Column case is preserved in the token (map literals decompose
        // compact {b:1} into a key and want it as typed); resolution is
        // case-insensitive regardless.
        let kinds = try Lexer.tokenize("A:1 + b:12").map(\.kind)
        #expect(kinds == [
            .cellReference(column: "A", row: 1, pinColumn: false, pinRow: false), .plus,
            .cellReference(column: "b", row: 12, pinColumn: false, pinRow: false), .end,
        ])
    }

    @Test func identifiersWithDigitsOrUnderscoresAreNotCellRefs() throws {
        // rate_2 contains '_' → the ':' is NOT consumed into a cell
        // reference; it lexes as a bare colon (which only map literals
        // accept — anywhere else it's a parse error).
        #expect(try Lexer.tokenize("rate_2:1").map(\.kind) == [
            .identifier("rate_2"), .colon, .number(BigDecimal(1)), .end,
        ])
        #expect(throws: EngineError.self) { try Parser.parse("rate_2:1") }
        // q1 contains a digit → same.
        #expect(try Lexer.tokenize("q1:1").map(\.kind) == [
            .identifier("q1"), .colon, .number(BigDecimal(1)), .end,
        ])
        #expect(throws: EngineError.self) { try Parser.parse("q1:1") }
    }

    @Test func parsesCellArithmetic() throws {
        #expect(try Parser.parse("A:1 + A:2") == .binary(
            .add,
            .cellReference(sheet: nil, column: "A", row: 1),
            .cellReference(sheet: nil, column: "A", row: 2)))
        // Implicit multiplication with a cell reference.
        #expect(try Parser.parse("2 A:1") == .binary(
            .multiply, .number(BigDecimal(2)), .cellReference(sheet: nil, column: "A", row: 1)))
    }

    @Test func detectsCellReferences() throws {
        #expect(try Parser.parse("sum(A:1, 2) * 3").containsCellReference)
        #expect(try Parser.parse("x = B:9").containsCellReference)
        #expect(try !Parser.parse("sum(1, 2) * x").containsCellReference)
    }

    @Test func evaluatingWithoutASheetFails() {
        let calc = Calculator()
        guard case .failure(let error) = calc.evaluate("A:1 + 1") else {
            Issue.record("expected failure with no sheet attached")
            return
        }
        #expect(error == .domainError(message: "no sheet available for A:1"))
    }
}

@Suite("Spreadsheet")
struct SpreadsheetTests {
    /// Calculator + sheet wired together the way the app does it.
    private func makeSheet() -> (Calculator, Spreadsheet) {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        return (calc, sheet)
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func classifiesCells() {
        let (_, sheet) = makeSheet()
        sheet.setCell("1200", at: addr(1, 0))            // number
        sheet.setCell("Q1 revenue", at: addr(0, 0))      // label: parses as Q1*revenue but can't evaluate
        sheet.setCell("hello world!", at: addr(0, 1))    // doesn't even parse
        sheet.setCell("2 + 2", at: addr(2, 0))           // formula without cell refs

        #expect(sheet.displayValue(at: addr(1, 0)) == .value(BigDecimal(1200)))
        #expect(sheet.displayValue(at: addr(0, 0)) == .text("Q1 revenue"))
        #expect(sheet.displayValue(at: addr(0, 1)) == .text("hello world!"))
        #expect(sheet.displayValue(at: addr(2, 0)) == .value(BigDecimal(4)))
        #expect(sheet.displayValue(at: addr(5, 5)) == .empty)
    }

    @Test func formulasReferenceOtherCells() {
        let (_, sheet) = makeSheet()
        sheet.setCell("1200", at: addr(1, 0))    // B:1
        sheet.setCell("1350", at: addr(1, 1))    // B:2
        sheet.setCell("B:1 + B:2", at: addr(1, 2))
        sheet.setCell("sum(B:1, B:2, B:3)", at: addr(1, 3))

        #expect(sheet.displayValue(at: addr(1, 2)) == .value(BigDecimal(2550)))
        #expect(sheet.displayValue(at: addr(1, 3)) == .value(BigDecimal(5100)))
    }

    @Test func emptyCellsReadAsZero() {
        let (_, sheet) = makeSheet()
        sheet.setCell("D:9 + 5", at: addr(0, 0))
        #expect(sheet.displayValue(at: addr(0, 0)) == .value(BigDecimal(5)))
    }

    @Test func formulaWithCellRefShowsErrorsInsteadOfTextFallback() {
        let (_, sheet) = makeSheet()
        sheet.setCell("A:1 / 0", at: addr(1, 0))
        #expect(sheet.displayValue(at: addr(1, 0)) == .error("division by zero"))
    }

    @Test func runtimeErrorsWithoutCellRefsAreStillErrors() {
        // Only unresolved names fall back to text; computational failures
        // can only come from genuine formulas.
        let (_, sheet) = makeSheet()
        sheet.setCell("12 / 0", at: addr(0, 0))
        sheet.setCell("sqrt(-1)", at: addr(0, 1))
        sheet.setCell("abs(1, 2)", at: addr(0, 2))

        #expect(sheet.displayValue(at: addr(0, 0)) == .error("division by zero"))
        #expect(sheet.displayValue(at: addr(0, 1)) == .error("sqrt of a negative number"))
        guard case .error = sheet.displayValue(at: addr(0, 2)) else {
            Issue.record("expected arity error")
            return
        }

        // Labels with unknown names still read as text.
        sheet.setCell("Net Total", at: addr(0, 3))
        sheet.setCell("hello(5)", at: addr(0, 4))
        #expect(sheet.displayValue(at: addr(0, 3)) == .text("Net Total"))
        #expect(sheet.displayValue(at: addr(0, 4)) == .text("hello(5)"))
    }

    @Test func referencingTextIsAnError() {
        let (_, sheet) = makeSheet()
        sheet.setCell("Q1 revenue", at: addr(0, 0))
        sheet.setCell("A:1 * 2", at: addr(0, 1))
        guard case .error(let message) = sheet.displayValue(at: addr(0, 1)) else {
            Issue.record("expected error")
            return
        }
        #expect(message.contains("A:1 is not a number"))
    }

    @Test func circularReferencesAreCaught() {
        let (_, sheet) = makeSheet()
        sheet.setCell("C:2", at: addr(2, 0))
        sheet.setCell("C:1", at: addr(2, 1))
        guard case .error(let message) = sheet.displayValue(at: addr(2, 0)) else {
            Issue.record("expected error")
            return
        }
        #expect(message.contains("circular reference"))

        // Self-reference too.
        sheet.setCell("D:1 + 1", at: addr(3, 0))
        guard case .error = sheet.displayValue(at: addr(3, 0)) else {
            Issue.record("expected error")
            return
        }
    }

    @Test func sharedVariablesAndAnsProtection() {
        let (calc, sheet) = makeSheet()
        _ = calc.evaluate("rate = 0.1")          // log defines a variable; ans = 0.1
        sheet.setCell("100", at: addr(0, 0))
        sheet.setCell("A:1 * rate", at: addr(0, 1))
        sheet.recalculate()

        #expect(sheet.displayValue(at: addr(0, 1)) == .value(BigDecimal(10)))
        // Cell evaluation must not have clobbered ans.
        #expect(calc.environment.ans == .number(BigDecimal(string: "0.1")!))
    }

    @Test func logExpressionsCanReadTheSheet() throws {
        let (calc, sheet) = makeSheet()
        sheet.setCell("1200", at: addr(1, 0))
        sheet.setCell("1350", at: addr(1, 1))
        // The user's plan: `total = B:1 + B:2` straight from the input bar.
        #expect(try calc.evaluate("total = B:1 + B:2").get() == .value(BigDecimal(2550)))
        #expect(try calc.evaluate("ans * 2").get() == .value(BigDecimal(5100)))
    }

    @Test func recalculatePicksUpVariableChanges() {
        let (calc, sheet) = makeSheet()
        _ = calc.evaluate("rate = 0.1")
        sheet.setCell("100 * rate", at: addr(0, 0))
        #expect(sheet.displayValue(at: addr(0, 0)) == .value(BigDecimal(10)))

        _ = calc.evaluate("rate = 0.2")
        sheet.recalculate()
        #expect(sheet.displayValue(at: addr(0, 0)) == .value(BigDecimal(20)))
    }

    @Test func clearingAndRangeChecks() {
        let (_, sheet) = makeSheet()
        sheet.setCell("42", at: addr(0, 0))
        sheet.setCell("  ", at: addr(0, 0)) // blank clears
        #expect(sheet.displayValue(at: addr(0, 0)) == .empty)
        #expect(sheet.raws.isEmpty)

        sheet.setCell("A:1001 + 1", at: addr(0, 1)) // row out of range (max 1000)
        guard case .error(let message) = sheet.displayValue(at: addr(0, 1)) else {
            Issue.record("expected error")
            return
        }
        #expect(message.contains("out of range"))
    }

    @Test func equalsPrefixForcesFormula() {
        let (_, sheet) = makeSheet()
        sheet.setCell("1200", at: addr(1, 0))
        sheet.setCell("= B:1 * 2", at: addr(1, 1))
        #expect(sheet.displayValue(at: addr(1, 1)) == .value(BigDecimal(2400)))

        // The whole point: a typo'd name is an error, not a silent label.
        sheet.setCell("=12 * rte", at: addr(0, 0))
        #expect(sheet.displayValue(at: addr(0, 0)) == .error("unknown variable 'rte'"))

        sheet.setCell("=", at: addr(0, 1))
        #expect(sheet.displayValue(at: addr(0, 1)) == .error("empty formula"))

        // Even unparseable input is an error when explicitly marked.
        sheet.setCell("=1 +", at: addr(0, 2))
        guard case .error = sheet.displayValue(at: addr(0, 2)) else {
            Issue.record("expected parse error")
            return
        }
    }

    @Test func quotesForceText() {
        let (_, sheet) = makeSheet()
        sheet.setCell("\"123\"", at: addr(0, 0))      // numeric-looking label
        sheet.setCell("\"B:1 + 1\"", at: addr(0, 1))  // formula-looking label
        sheet.setCell("\"unclosed", at: addr(0, 2))

        #expect(sheet.displayValue(at: addr(0, 0)) == .text("123"))
        #expect(sheet.displayValue(at: addr(0, 1)) == .text("B:1 + 1"))
        #expect(sheet.displayValue(at: addr(0, 2)) == .text("unclosed"))

        // Quoted text is still text when referenced.
        sheet.setCell("A:1 * 2", at: addr(0, 3))
        guard case .error(let message) = sheet.displayValue(at: addr(0, 3)) else {
            Issue.record("expected error")
            return
        }
        #expect(message.contains("A:1 is not a number"))
    }

    @Test func addressFormatting() {
        #expect("\(addr(0, 0))" == "A:1")
        #expect("\(addr(25, 99))" == "Z:100")
    }

    @Test func addressParsing() {
        // All name/index/key conversions are centralized on CellAddress.
        #expect(CellAddress(key: "A:1") == addr(0, 0))
        #expect(CellAddress(key: "z:1000") == addr(25, 999)) // case-insensitive
        #expect(CellAddress(key: "A:0") == nil)
        #expect(CellAddress(key: "A:1001") == nil)
        #expect(CellAddress(key: "AA:1") == nil)
        #expect(CellAddress(key: "A1") == nil)
        #expect(CellAddress(columnName: "B", rowNumber: 3) == addr(1, 2))
        #expect(CellAddress.columnIndex(forName: "a") == 0)
        #expect(CellAddress.columnName(forIndex: 25) == "Z")
    }
}
