import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Structural edits (insert/delete rows & columns)")
struct StructuralEditTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func insertRowShiftsContentFormulasNamesAndLayout() throws {
        let (_, store) = makeStore()
        let sheet = store.activeSheet
        let grid = sheet.grid
        grid.setCell("100", at: addr(0, 2))          // A:3
        grid.setCell("=A:3 * 2", at: addr(1, 4))     // B:5
        try grid.setCellName("Base", at: addr(0, 2))
        sheet.formats[addr(0, 2)] = { var f = CellFormat(); f.bold = true; return f }()
        sheet.rowHeights[2] = 44

        _ = try store.insertSlots(axis: .row, at: 1, count: 2, in: sheet) // above row 2

        #expect(grid.raw(at: addr(0, 2)).isEmpty)            // old home empty
        #expect(grid.raw(at: addr(0, 4)) == "100")           // A:3 → A:5
        #expect(grid.raw(at: addr(1, 6)) == "=A:5 * 2")      // formula followed
        #expect(grid.displayValue(at: addr(1, 6)) == .value(BigDecimal(200)))
        #expect(grid.cellNames[addr(0, 4)] == "Base")
        #expect(sheet.formats[addr(0, 4)]?.bold == true)
        #expect(sheet.rowHeights[4] == 44)
        #expect(sheet.rowHeights[2] == nil)
    }

    @Test func deleteRowKillsRefsShrinksRangesAndRecords() throws {
        let (_, store) = makeStore()
        let sheet = store.activeSheet
        let grid = sheet.grid
        grid.setCell("10", at: addr(0, 0))
        grid.setCell("20", at: addr(0, 1))           // A:2 — to be deleted
        grid.setCell("30", at: addr(0, 2))
        grid.setCell("=A:2 * 2", at: addr(1, 0))     // B:1 reads the dead row
        grid.setCell("=sum(A:1..A:3)", at: addr(1, 3)) // B:4 range shrinks

        let change = try store.deleteSlots(axis: .row, at: 1, count: 1, in: sheet)

        #expect(grid.raw(at: addr(0, 1)) == "30")            // A:3 slid up
        #expect(grid.raw(at: addr(1, 0)) == "=refError() * 2")
        guard case .error(let message) = grid.displayValue(at: addr(1, 0)) else {
            Issue.record("dead ref should error"); return
        }
        #expect(message.contains("deleted"))
        #expect(grid.raw(at: addr(1, 2)) == "=sum(A:1..A:2)") // B:4 → B:3, shrunk
        #expect(grid.displayValue(at: addr(1, 2)) == .value(BigDecimal(40)))
        #expect(change.removedCells[addr(0, 1)] == "20")
    }

    @Test func qualifiedRefsFollowFromOtherSheets() throws {
        let (_, store) = makeStore()
        let budget = store.activeSheet
        budget.grid.setCell("250", at: addr(1, 0))   // Budget B:1
        try store.addSheet()
        let other = store.sheets[1]
        other.grid.setCell("='Sheet 1'!B:1 * 2", at: addr(0, 0))

        _ = try store.insertSlots(axis: .column, at: 0, count: 1, in: budget)

        #expect(other.grid.raw(at: addr(0, 0)) == "='Sheet 1'!C:1 * 2")
        #expect(other.grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(500)))
        // The other sheet's own A:1 (unqualified would be its sheet) did NOT shift.
    }

    @Test func revertIsAnExactInverse() throws {
        let (_, store) = makeStore()
        let sheet = store.activeSheet
        let grid = sheet.grid
        grid.setCell("10", at: addr(0, 1))           // A:2
        grid.setCell("=A:2 + 1", at: addr(1, 1))     // B:2
        grid.setCell("=sum(A:1..A:4)", at: addr(2, 0)) // C:1
        try grid.setCellName("Rate", at: addr(0, 1))
        sheet.rowHeights[1] = 33

        let change = try store.deleteSlots(axis: .row, at: 1, count: 1, in: sheet)
        // The whole row-2 slice (value, formula, name, height) left the grid.
        #expect(grid.raw(at: addr(1, 1)).isEmpty)
        #expect(change.removedCells[addr(1, 1)] == "=A:2 + 1")

        store.revert(change)

        #expect(grid.raw(at: addr(0, 1)) == "10")
        #expect(grid.raw(at: addr(1, 1)) == "=A:2 + 1")
        #expect(grid.raw(at: addr(2, 0)) == "=sum(A:1..A:4)")
        #expect(grid.cellNames[addr(0, 1)] == "Rate")
        #expect(sheet.rowHeights[1] == 33)
        #expect(grid.displayValue(at: addr(1, 1)) == .value(BigDecimal(11)))

        // Insert + revert round-trips too.
        let insert = try store.insertSlots(axis: .row, at: 0, count: 3, in: sheet)
        #expect(grid.raw(at: addr(0, 4)) == "10")
        store.revert(insert)
        #expect(grid.raw(at: addr(0, 1)) == "10")
        #expect(grid.raw(at: addr(1, 1)) == "=A:2 + 1")
    }

    @Test func definitionsAndControlsSurviveTheMove() throws {
        let (calc, store) = makeStore()
        let sheet = store.activeSheet
        let grid = sheet.grid
        grid.setCell("rate = slider(0.08, 0, 0.2)", at: addr(0, 0)) // A:1
        grid.setCell("=rate * 100", at: addr(0, 1))                  // A:2

        _ = try store.insertSlots(axis: .row, at: 0, count: 1, in: sheet)

        guard case .slider = grid.displayValue(at: addr(0, 1)) else {
            Issue.record("the slider definition should have moved intact"); return
        }
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(8)))
        #expect(try calc.evaluate("rate * 1000").get() == .value(BigDecimal(80)))
    }

    @Test func insertRefusesWhenContentWouldFallOff() throws {
        let (_, store) = makeStore()
        let sheet = store.activeSheet
        sheet.grid.setCell("edge", at: addr(0, Spreadsheet.rowCount - 1)) // A:1000
        #expect(throws: EngineError.self) {
            try store.insertSlots(axis: .row, at: 0, count: 1, in: sheet)
        }
        // Deleting that row is fine, and frees the insert.
        _ = try store.deleteSlots(axis: .row, at: Spreadsheet.rowCount - 1, count: 1, in: sheet)
        _ = try store.insertSlots(axis: .row, at: 0, count: 1, in: sheet)
    }

    @Test func dataSheetsRefuseStructuralEdits() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("soroban-structure-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let dataStore = try DataStore(url: dir.appendingPathComponent("data.sqlite"))
        try dataStore.createTable(name: "t", rows: [["a", "b"], ["1", "2"]])

        let (_, store) = makeStore()
        let sheet = store.activeSheet
        sheet.data = DataSheet(table: "t", store: dataStore)
        #expect(throws: EngineError.self) {
            try store.insertSlots(axis: .row, at: 0, count: 1, in: sheet)
        }
        #expect(throws: EngineError.self) {
            try store.deleteSlots(axis: .column, at: 0, count: 1, in: sheet)
        }
    }
}
