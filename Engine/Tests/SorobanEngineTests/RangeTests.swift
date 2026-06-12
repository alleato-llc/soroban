import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Cell ranges")
struct RangeTests {
    /// Calculator + sheet with both resolvers wired, like the app.
    private func makeSheet() -> (Calculator, Spreadsheet) {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        calc.rangeResolver = { [weak sheet] _, fc, fr, tc, tr in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValues(fromColumn: fc, fromRow: fr,
                                           toColumn: tc, toRow: tr)
        }
        return (calc, sheet)
    }

    private func fill(_ sheet: Spreadsheet, _ cells: [(Int, Int, String)]) {
        for (column, row, raw) in cells {
            sheet.setCell(raw, at: CellAddress(column: column, row: row))
        }
    }

    @Test func aggregatesOverColumnRange() throws {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(1, 0, "10"), (1, 1, "20"), (1, 2, "30")]) // B:1..B:3

        #expect(try calc.evaluate("sum(B:1..B:3)").get() == .value(BigDecimal(60)))
        #expect(try calc.evaluate("avg(B:1..B:3)").get() == .value(BigDecimal(20)))
        #expect(try calc.evaluate("min(B:1..B:3)").get() == .value(BigDecimal(10)))
        #expect(try calc.evaluate("max(B:1..B:3)").get() == .value(BigDecimal(30)))
        #expect(try calc.evaluate("count(B:1..B:3)").get() == .value(BigDecimal(3)))
        #expect(try calc.evaluate("∑(B:1..B:3)").get() == .value(BigDecimal(60)))
    }

    @Test func rectanglesAndReversedCorners() throws {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(0, 0, "1"), (1, 0, "2"), (0, 1, "3"), (1, 1, "4")]) // A:1..B:2

        #expect(try calc.evaluate("sum(A:1..B:2)").get() == .value(BigDecimal(10)))
        // Corners normalize in any orientation.
        #expect(try calc.evaluate("sum(B:2..A:1)").get() == .value(BigDecimal(10)))
    }

    @Test func sparseAndTextCellsAreSkipped() throws {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(1, 0, "10"), (1, 2, "30"), (1, 4, "Q1 label")]) // gaps + text

        #expect(try calc.evaluate("sum(B:1..B:5)").get() == .value(BigDecimal(40)))
        #expect(try calc.evaluate("count(B:1..B:5)").get() == .value(BigDecimal(2)))
        #expect(try calc.evaluate("avg(B:1..B:5)").get() == .value(BigDecimal(20)))
    }

    @Test func mixesWithPlainArguments() throws {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(1, 0, "10"), (1, 1, "20")])
        #expect(try calc.evaluate("sum(B:1..B:2, 70)").get() == .value(BigDecimal(100)))
    }

    @Test func errorCellsPropagate() {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(1, 0, "10"), (1, 1, "1/0")])
        guard case .failure(let error) = calc.evaluate("sum(B:1..B:2)") else {
            Issue.record("expected propagated error")
            return
        }
        #expect(error.description.contains("division by zero"))
    }

    @Test func worksInsideCellsAndUserFunctions() throws {
        let (calc, sheet) = makeSheet()
        fill(sheet, [(1, 0, "10"), (1, 1, "20"), (1, 2, "30")])

        sheet.setCell("sum(B:1..B:3)", at: CellAddress(column: 2, row: 0))
        #expect(sheet.displayValue(at: CellAddress(column: 2, row: 0)) == .value(BigDecimal(60)))

        _ = calc.evaluate("total() = sum(B:1..B:3)")
        #expect(try calc.evaluate("total()").get() == .value(BigDecimal(60)))
    }

    @Test func rangeErrors() {
        let (calc, sheet) = makeSheet()
        defer { _ = sheet } // the resolvers capture it weakly
        // Standalone ranges are meaningless.
        guard case .failure(let standalone) = calc.evaluate("A:1..A:9 + 1") else {
            Issue.record("expected standalone-range failure")
            return
        }
        #expect(standalone.description.contains("inside functions"))

        // Dangling '..'
        #expect(calc.evaluate("sum(A:1..)").isFailure)
        // Out of bounds.
        #expect(calc.evaluate("sum(A:1..A:1001)").isFailure)
        // No sheet attached.
        #expect(Calculator().evaluate("sum(A:1..A:9)").isFailure)
    }

    @Test func countOfNothingIsZero() throws {
        let (calc, sheet) = makeSheet()
        defer { _ = sheet } // the resolvers capture it weakly
        #expect(try calc.evaluate("count(A:1..A:9)").get() == .value(BigDecimal.zero))
    }
}
