import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Worksheets (SheetStore)")
struct SheetStoreTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func qualifiedReferencesAcrossSheets() throws {
        let (calc, store) = makeStore()
        let budget = try store.addSheet()
        try store.rename(at: 1, to: "Budget")
        budget.grid.setCell("1200", at: addr(1, 0)) // Budget!B:1

        // From the log (Sheet 1 active): bang syntax, both spellings.
        #expect(try calc.evaluate("Budget!B:1 * 2").get() == .value(BigDecimal(2400)))
        #expect(try calc.evaluate("'Budget'!B:1 + 1").get() == .value(BigDecimal(1201)))

        // From a cell on Sheet 1.
        store.sheets[0].grid.setCell("Budget!B:1 / 2", at: addr(0, 0))
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(600)))

        // Qualified ranges.
        budget.grid.setCell("300", at: addr(1, 1))
        #expect(try calc.evaluate("sum(Budget!B:1..B:2)").get() == .value(BigDecimal(1500)))
    }

    @Test func quotedNamesWithSpaces() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        try store.rename(at: 1, to: "Q1 Budget")
        store.sheets[1].grid.setCell("10", at: addr(0, 0))
        #expect(try calc.evaluate("'Q1 Budget'!A:1 * 3").get() == .value(BigDecimal(30)))
        // Sheet name matching is case-insensitive, like everything else.
        #expect(try calc.evaluate("'q1 budget'!A:1").get() == .value(BigDecimal(10)))
    }

    @Test func unqualifiedRefsBelongToTheOwningSheet() throws {
        // The crux: a formula on Sheet 2 reads ITS OWN A:1, even while
        // Sheet 1 is active and triggers the evaluation.
        let (calc, store) = makeStore()
        try store.addSheet()
        store.sheets[0].grid.setCell("111", at: addr(0, 0))   // Sheet 1!A:1
        store.sheets[1].grid.setCell("222", at: addr(0, 0))   // Sheet 2!A:1
        store.sheets[1].grid.setCell("A:1 * 2", at: addr(0, 1)) // on Sheet 2

        store.activeIndex = 0 // user is looking at Sheet 1
        #expect(store.sheets[1].grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(444)))

        // From the log, unqualified refs follow the ACTIVE sheet.
        #expect(try calc.evaluate("A:1").get() == .value(BigDecimal(111)))
        store.activeIndex = 1
        #expect(try calc.evaluate("A:1").get() == .value(BigDecimal(222)))
    }

    @Test func crossSheetCyclesAreCaught() throws {
        let (_, store) = makeStore()
        try store.addSheet() // "Sheet 2"
        store.sheets[0].grid.setCell("'Sheet 2'!A:1 + 1", at: addr(0, 0))
        store.sheets[1].grid.setCell("'Sheet 1'!A:1 + 1", at: addr(0, 0))

        guard case .error(let message) = store.sheets[0].grid.displayValue(at: addr(0, 0)) else {
            Issue.record("expected a circular-reference error, not a hang")
            return
        }
        #expect(message.contains("circular reference"))
        #expect(message.contains("!")) // the report names the sheet
    }

    @Test func unknownSheetIsACleanError() {
        let (calc, store) = makeStore()
        defer { _ = store }
        guard case .failure(let error) = calc.evaluate("Nope!A:1") else {
            Issue.record("expected unknown-sheet failure")
            return
        }
        #expect(error.description.contains("unknown sheet 'Nope'"))
    }

    @Test func renameIsByNameSoOldReferencesError() throws {
        let (_, store) = makeStore()
        try store.addSheet()
        store.sheets[0].grid.setCell("'Sheet 2'!A:1 + 1", at: addr(0, 0))
        store.sheets[1].grid.setCell("41", at: addr(0, 0))
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(42)))

        try store.rename(at: 1, to: "Budget")
        guard case .error(let message) = store.sheets[0].grid.displayValue(at: addr(0, 0)) else {
            Issue.record("expected unknown-sheet error after rename")
            return
        }
        #expect(message.contains("unknown sheet"))
    }

    @Test func structureRules() throws {
        let (_, store) = makeStore()
        // Can't remove the last sheet.
        #expect(throws: EngineError.self) { try store.removeSheet(at: 0) }

        // Names: validation.
        try store.addSheet()
        #expect(throws: EngineError.self) { try store.rename(at: 1, to: "   ") }
        #expect(throws: EngineError.self) { try store.rename(at: 1, to: "Bad!Name") }
        #expect(throws: EngineError.self) { try store.rename(at: 1, to: "Bad'Name") }
        #expect(throws: EngineError.self) { try store.rename(at: 1, to: "sheet 1") } // dup, case-insensitive
        #expect(throws: EngineError.self) {
            try store.rename(at: 1, to: String(repeating: "x", count: 129))
        }
        try store.rename(at: 1, to: String(repeating: "x", count: 128)) // exactly the cap is fine

        // Auto-naming skips taken names.
        try store.rename(at: 1, to: "Sheet 3")
        let added = try store.addSheet()
        #expect(added.name == "Sheet 4")

        // Removal clamps the active index and recalculates.
        store.activeIndex = 2
        try store.removeSheet(at: 2)
        #expect(store.activeIndex == 1)
    }

    @Test func sheetCapIs256() throws {
        let (_, store) = makeStore()
        for _ in 1..<SheetStore.maxSheets {
            try store.addSheet()
        }
        #expect(store.sheets.count == 256)
        #expect(throws: EngineError.self) { try store.addSheet() }
    }

    @Test func editsInvalidateDependentsAcrossSheets() throws {
        // The dependency graph at work: an edit reaches only its readers —
        // including readers on OTHER sheets (this was a staleness bug when
        // recalc was per-sheet memo clearing).
        let (_, store) = makeStore()
        try store.addSheet()
        let s1 = store.sheets[0].grid
        let s2 = store.sheets[1].grid

        s1.setCell("10", at: addr(0, 0))
        s1.setCell("A:1 * 2", at: addr(0, 1))                  // same-sheet reader
        s2.setCell("'Sheet 1'!A:1 + 5", at: addr(0, 0))        // cross-sheet reader
        s2.setCell("sum('Sheet 1'!A:1..A:5)", at: addr(0, 1))  // cross-sheet range reader

        // Evaluate everything once so the graph is recorded.
        #expect(s1.displayValue(at: addr(0, 1)) == .value(BigDecimal(20)))
        #expect(s2.displayValue(at: addr(0, 0)) == .value(BigDecimal(15)))
        #expect(s2.displayValue(at: addr(0, 1)) == .value(BigDecimal(30))) // 10 + A:2(20)

        // Edit the source: every reader updates, with no full recalc.
        s1.setCell("100", at: addr(0, 0))
        #expect(s1.displayValue(at: addr(0, 1)) == .value(BigDecimal(200)))
        #expect(s2.displayValue(at: addr(0, 0)) == .value(BigDecimal(105)))
        #expect(s2.displayValue(at: addr(0, 1)) == .value(BigDecimal(300))) // 100 + 200

        // A NEW cell inside an already-recorded range is picked up too.
        s1.setCell("1", at: addr(0, 2)) // inside A:1..A:5
        #expect(s2.displayValue(at: addr(0, 1)) == .value(BigDecimal(301)))
    }

    @Test func dependencyChainsInvalidateTransitively() {
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("1", at: addr(0, 0))
        grid.setCell("A:1 + 1", at: addr(0, 1))
        grid.setCell("A:2 + 1", at: addr(0, 2))
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(3)))

        grid.setCell("10", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(12)))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(11)))
    }

    @Test func storeWideRecalcPicksUpVariables() throws {
        let (calc, store) = makeStore()
        try store.addSheet()
        store.sheets[1].grid.setCell("100 * rate", at: addr(0, 0))
        _ = calc.evaluate("rate = 0.1")
        store.recalculate()
        #expect(store.sheets[1].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(10)))

        _ = calc.evaluate("rate = 0.2")
        store.recalculate()
        #expect(store.sheets[1].grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(20)))
    }
}

@Suite("Workbook multi-sheet codec")
struct MultiSheetCodecTests {
    @Test func sheetsRoundTrip() throws {
        let workbook = Workbook(
            sheets: [
                .init(name: "Sheet 1", cells: ["A:1": "1"], columnWidths: ["A": 120]),
                .init(name: "Q1 Budget", cells: ["B:2": "'Sheet 1'!A:1 * 2"], rowHeights: ["2": 40]),
            ],
            activeSheet: "Q1 Budget",
            variables: ["rate": .number(BigDecimal(string: "0.1")!)])

        let decoded = try Workbook.decode(try workbook.encode())
        #expect(decoded.sheets.count == 2)
        #expect(decoded.sheets[1].name == "Q1 Budget")
        #expect(decoded.sheets[1].cells["B:2"] == "'Sheet 1'!A:1 * 2")
        #expect(decoded.sheets[0].columnWidths == ["A": 120])
        #expect(decoded.activeSheet == "Q1 Budget")
    }

    @Test func legacyFlatFilesBecomeOneSheet() throws {
        let decoded = try Workbook.decode(Data("""
            {"format": "soroban-workbook", "version": 1,
             "cells": {"A:1": "42"}, "variables": {},
             "columnWidths": {"A": 150}}
            """.utf8))
        #expect(decoded.sheets.count == 1)
        #expect(decoded.sheets[0].name == "Sheet 1")
        #expect(decoded.sheets[0].cells == ["A:1": "42"])
        #expect(decoded.sheets[0].columnWidths == ["A": 150])
    }
}
