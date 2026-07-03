import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

/// The workbook MUTATION commands — `updateCell` / `addWorksheet` /
/// `renameWorksheet` / `deleteWorksheet`. They run from the LOG only (rejected
/// during cell recalc, so recalculation stays reproducible) and change the
/// workbook directly in this default (no-undo) wiring.
@Suite("Workbook mutation")
struct WorkbookMutationTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    // MARK: updateCell

    @Test func updateCellWritesANumber() throws {
        let (calc, store) = makeStore()
        _ = try calc.evaluate("updateCell(cell(\"A\", 1), 42)").get()
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(42)))
    }

    @Test func updateCellWritesAFormulaAndALabel() throws {
        let (calc, store) = makeStore()
        store.sheets[0].grid.setCell("10", at: addr(1, 0)) // B:1 = 10
        _ = try calc.evaluate("updateCell(cell(\"A\", 1), \"=B:1 * 2\")").get()
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(20)))
        _ = try calc.evaluate("updateCell(cell(\"A\", 2), \"Total\")").get()
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 1)) == .text("Total"))
    }

    @Test func updateCellRoundTripsThroughReflection() throws {
        let (calc, store) = makeStore()
        _ = store // resolvers hold the store weakly; keep it alive
        _ = try calc.evaluate("updateCell(cell(\"A\", 1), 7)").get()
        #expect(try calc.evaluate("cell(\"A\", 1).value").get() == .value(BigDecimal(7)))
    }

    // MARK: log-only gating

    @Test func mutationFromACellIsRejected() throws {
        let (_, store) = makeStore()
        // A cell formula that tries to mutate must error, not change anything.
        store.sheets[0].grid.setCell("=updateCell(cell(\"B\", 1), 5)", at: addr(0, 0))
        guard case .error(let message) = store.sheets[0].grid.displayValue(at: addr(0, 0)) else {
            Issue.record("expected an error for a mutation inside a cell")
            return
        }
        #expect(message.contains("calculation log"))
        // And B:1 was never written.
        #expect(store.sheets[0].grid.displayValue(at: addr(1, 0)) == .empty)
    }

    // MARK: addWorksheet

    @Test func addWorksheetAppendsANamedSheet() throws {
        let (calc, store) = makeStore()
        _ = try calc.evaluate("addWorksheet(\"Budget\")").get()
        #expect(store.sheets.count == 2)
        #expect(store.sheets[1].name == "Budget")
        // The new sheet is immediately reachable by reflection.
        #expect(try calc.evaluate("Workbook.worksheets[\"Budget\"].name == \"Budget\"").get()
            == .value(BigDecimal(1)))
    }

    @Test func addWorksheetRejectsADuplicateName() throws {
        let (calc, store) = makeStore()
        _ = store
        #expect(throws: EngineError.self) {
            try calc.evaluate("addWorksheet(\"Sheet 1\")").get() // already exists
        }
    }

    // MARK: renameWorksheet

    @Test func renameWorksheetRewritesReferences() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        store.sheets[1].grid.setCell("1200", at: addr(0, 0))      // Budget!A:1
        store.sheets[0].grid.setCell("=Budget!A:1 * 2", at: addr(0, 0)) // Sheet 1!A:1

        _ = try calc.evaluate("renameWorksheet(\"Budget\", \"Costs\")").get()
        #expect(store.sheets[1].name == "Costs")
        // The cross-sheet reference followed the rename.
        #expect(store.sheets[0].grid.raw(at: addr(0, 0)) == "=Costs!A:1 * 2")
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(2400)))
    }

    @Test func renameWorksheetAcceptsAHandle() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        _ = try calc.evaluate("renameWorksheet(Workbook.worksheets[\"Budget\"], \"Costs\")").get()
        #expect(store.sheets[1].name == "Costs")
    }

    // MARK: deleteWorksheet

    @Test func deleteWorksheetRemovesIt() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        #expect(try calc.evaluate("deleteWorksheet(\"Budget\")").get() == .value(BigDecimal(1)))
        #expect(store.sheets.count == 1)
        #expect(store.sheet(named: "Budget") == nil)
    }

    @Test func deleteWorksheetRefusesTheLastSheet() throws {
        let (calc, store) = makeStore()
        _ = store
        #expect(throws: EngineError.self) {
            try calc.evaluate("deleteWorksheet(\"Sheet 1\")").get()
        }
    }

    @Test func unknownSheetTargetIsReported() throws {
        let (calc, store) = makeStore()
        _ = store
        #expect(throws: EngineError.self) {
            try calc.evaluate("deleteWorksheet(\"Nope\")").get()
        }
    }
}
