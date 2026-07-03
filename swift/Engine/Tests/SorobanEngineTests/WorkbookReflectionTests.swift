import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

/// The read-only Workbook reflection API: the `Workbook` object graph and the
/// flat `cell()`/`sheetNames()`/… accessors. Reads are LIVE (dependency edges
/// keep formulas fresh) and there are no mutators — inspect, never change.
@Suite("Workbook reflection")
struct WorkbookReflectionTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    // MARK: Object graph

    @Test func workbookExposesSheetMetadata() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")

        #expect(try calc.evaluate("Workbook.count").get() == .value(BigDecimal(2)))
        #expect(try calc.evaluate("len(Workbook.sheetNames)").get() == .value(BigDecimal(2)))
        #expect(try calc.evaluate("Workbook.sheetNames[1]").get() == .value(.string("Budget")))
        #expect(try calc.evaluate("Workbook.worksheets.count").get() == .value(BigDecimal(2)))
    }

    @Test func worksheetByPositionAndName() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")

        #expect(try calc.evaluate("Workbook.worksheets[0].name").get() == .value(.string("Sheet 1")))
        #expect(try calc.evaluate("Workbook.worksheets[1].name").get() == .value(.string("Budget")))
        // Negative index counts from the end.
        #expect(try calc.evaluate("Workbook.worksheets[-1].name").get() == .value(.string("Budget")))
        // By name.
        #expect(try calc.evaluate("Workbook.worksheets[\"Budget\"].name").get()
            == .value(.string("Budget")))
        #expect(try calc.evaluate("Workbook.worksheets[0].rowCount").get()
            == .value(BigDecimal(Spreadsheet.rowCount)))
        #expect(try calc.evaluate("Workbook.worksheets[0].columnCount").get()
            == .value(BigDecimal(Spreadsheet.columnCount)))
    }

    @Test func cellValueThroughMethodCall() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        store.sheets[1].grid.setCell("1200", at: addr(1, 0)) // Budget!B:1

        #expect(try calc.evaluate("Workbook.worksheets[\"Budget\"].cell(\"B\", 1).value").get()
            == .value(BigDecimal(1200)))
        #expect(try calc.evaluate("Workbook.worksheets[1].cell(\"B\", 1).value * 2").get()
            == .value(BigDecimal(2400)))
    }

    @Test func cellMembers() throws {
        let (calc, store) = makeStore()
        store.sheets[0].grid.setCell("=2 + 3", at: addr(0, 0)) // A:1 = 5
        store.sheets[0].grid.setCell("hello", at: addr(0, 1))  // A:2 text

        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"A\", 1).value").get()
            == .value(BigDecimal(5)))
        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"A\", 1).address").get()
            == .value(.string("A:1")))
        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"A\", 1).raw").get()
            == .value(.string("=2 + 3")))
        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"A\", 2).text").get()
            == .value(.string("hello")))
        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"A\", 1).isEmpty").get()
            == .value(.bool(false)))
        #expect(try calc.evaluate("Workbook.worksheets[0].cell(\"Z\", 9).isEmpty").get()
            == .value(.bool(true)))
    }

    // MARK: Flat functions

    @Test func flatAccessors() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        store.sheets[0].grid.setCell("42", at: addr(0, 0))     // Sheet 1!A:1
        store.sheets[1].grid.setCell("99", at: addr(0, 0))     // Budget!A:1

        // Active sheet is Sheet 1.
        #expect(try calc.evaluate("sheetName()").get() == .value(.string("Sheet 1")))
        #expect(try calc.evaluate("len(sheetNames())").get() == .value(BigDecimal(2)))
        #expect(try calc.evaluate("rowCount()").get() == .value(BigDecimal(Spreadsheet.rowCount)))
        #expect(try calc.evaluate("columnCount()").get()
            == .value(BigDecimal(Spreadsheet.columnCount)))

        // cell(col, row) reads the active sheet; cell(sheet, col, row) by name.
        #expect(try calc.evaluate("cell(\"A\", 1).value").get() == .value(BigDecimal(42)))
        #expect(try calc.evaluate("cell(\"Budget\", \"A\", 1).value").get()
            == .value(BigDecimal(99)))
    }

    // MARK: Reads are live (dependency edges)

    @Test func cellReadThroughReflectionRecalculatesLive() throws {
        let (_, store) = makeStore()
        let grid = store.sheets[0].grid
        grid.setCell("10", at: addr(0, 0)) // A:1 = 10
        // B:1 reads A:1 through the reflection API.
        grid.setCell("=cell(\"A\", 1).value + 1", at: addr(1, 0))
        #expect(grid.displayValue(at: addr(1, 0)) == .value(BigDecimal(11)))

        // Change A:1 — B:1 must recompute (the reflection read recorded an edge).
        grid.setCell("100", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(1, 0)) == .value(BigDecimal(101)))
    }

    @Test func unqualifiedCellFunctionFollowsOwningSheet() throws {
        // cell("A", 1) inside a formula on Sheet 2 reads Sheet 2's A:1, even
        // while Sheet 1 is active — same owning-sheet rule as a bare A:1.
        let (_, store) = makeStore()
        try store.addSheet()
        store.sheets[0].grid.setCell("111", at: addr(0, 0))
        store.sheets[1].grid.setCell("222", at: addr(0, 0))
        store.sheets[1].grid.setCell("=cell(\"A\", 1).value", at: addr(0, 1))
        store.activeIndex = 0
        #expect(store.sheets[1].grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(222)))
    }

    // MARK: Errors

    @Test func reflectionErrorsAreClear() throws {
        let (calc, _) = makeStore()
        #expect(throws: EngineError.self) {
            try calc.evaluate("Workbook.worksheets[0].cell(\"A\")").get() // wrong arity
        }
        #expect(throws: EngineError.self) {
            try calc.evaluate("cell(\"Nope\", \"A\", 1).value").get() // unknown sheet
        }
        // Out-of-range index returns nothing → an index error.
        #expect(throws: EngineError.self) {
            try calc.evaluate("Workbook.worksheets[9].name").get()
        }
    }

    @Test func userFunctionShadowsReflectionFunction() throws {
        // A user's own cell(x) wins over the reflection accessor — reflection
        // functions resolve LAST, like a builtin would.
        let (calc, _) = makeStore()
        _ = try calc.evaluate("cell(x) = x * 10").get()
        #expect(try calc.evaluate("cell(5)").get() == .value(BigDecimal(50)))
    }

    @Test func storedWorksheetHandleSurvivesInVariable() throws {
        // The handle is a first-class value: bind it, read through it later.
        let (calc, store) = makeStore()
        store.sheets[0].grid.setCell("7", at: addr(0, 0))
        _ = try calc.evaluate("w = Workbook.worksheets[0]").get()
        #expect(try calc.evaluate("w.cell(\"A\", 1).value").get() == .value(BigDecimal(7)))
        #expect(try calc.evaluate("w.name").get() == .value(.string("Sheet 1")))
    }
}
