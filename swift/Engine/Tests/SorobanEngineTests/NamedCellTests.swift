import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Named cells")
struct NamedCellTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func parsesNameReferences() throws {
        #expect(try Parser.parse("'Projected Rate'")
                == .nameReference(sheet: nil, name: "Projected Rate"))
        #expect(try Parser.parse("Budget!'Rate'")
                == .nameReference(sheet: "Budget", name: "Rate"))
        #expect(try Parser.parse("'Q1 Budget'!'Rate'")
                == .nameReference(sheet: "Q1 Budget", name: "Rate"))
        // Sheet qualifiers still work — the ! disambiguates.
        #expect(try Parser.parse("'Q1 Budget'!A:1")
                == .cellReference(sheet: "Q1 Budget", column: "A", row: 1))
        // Names compose like any operand.
        #expect(try Parser.parse("'Rate' * 12")
                == .binary(.multiply, .nameReference(sheet: nil, name: "Rate"), .number(BigDecimal(12))))
    }

    @Test func namesResolveAndTrackDependencies() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("0.08", at: addr(1, 6)) // B:7
        try grid.setCellName("Projected Rate", at: addr(1, 6))

        // From a cell on the same sheet…
        grid.setCell("='Projected Rate' * 100", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(8)))
        // …from the log (active sheet)…
        #expect(try calc.evaluate("'Projected Rate' + 1").get()
                == .value(BigDecimal(string: "1.08")!))
        // …and qualified from another sheet.
        let second = try store.addSheet()
        second.grid.setCell("='Sheet 1'!'Projected Rate' * 2", at: addr(0, 0))
        #expect(second.grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(string: "0.16")!))

        // Dependency edges flow THROUGH the name: change B:7, readers update.
        grid.setCell("0.5", at: addr(1, 6))
        #expect(grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(50)))
        #expect(second.grid.displayValue(at: addr(0, 0)) == .value(BigDecimal(1)))
    }

    @Test func validationRules() throws {
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        try grid.setCellName("Rate", at: addr(0, 0))

        // Unique per sheet, case-insensitive.
        #expect(throws: EngineError.self) { try grid.setCellName("rate", at: addr(0, 1)) }
        // Renaming the same cell is fine.
        try grid.setCellName("RATE", at: addr(0, 0))
        #expect(grid.address(forName: "Rate") == addr(0, 0))

        #expect(throws: EngineError.self) { try grid.setCellName("", at: addr(0, 2)) }
        #expect(throws: EngineError.self) {
            try grid.setCellName(String(repeating: "x", count: 65), at: addr(0, 2))
        }
        #expect(throws: EngineError.self) { try grid.setCellName("a'b", at: addr(0, 2)) }
        #expect(throws: EngineError.self) { try grid.setCellName("a!b", at: addr(0, 2)) }

        // Removal.
        try grid.setCellName(nil, at: addr(0, 0))
        #expect(grid.address(forName: "Rate") == nil)
    }

    @Test func unknownAndCyclicNamesError() throws {
        let (calc, store) = makeStore()
        guard case .failure(let unknown) = calc.evaluate("'Nope' + 1") else {
            Issue.record("unknown names should error"); return
        }
        #expect("\(unknown)".contains("Nope"))

        // A named cell whose formula reads its own name → cycle, not a hang.
        let grid = store.activeSheet.grid
        grid.setCell("='Loop' + 1", at: addr(0, 0))
        try grid.setCellName("Loop", at: addr(0, 0))
        guard case .error(let message) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("self-referential name should error"); return
        }
        #expect(message.contains("circular"))
    }

    @Test func namesPersistInWorkbooks() throws {
        var payload = Workbook.SheetPayload(name: "Sheet 1", cells: ["B:7": "0.08"])
        payload.names = ["B:7": "Projected Rate"]
        let decoded = try Workbook.decode(try Workbook(sheets: [payload], variables: [:]).encode())
        #expect(decoded.sheets[0].names == ["B:7": "Projected Rate"])
    }

    @Test func rewritingRespectsScoping() {
        // Unqualified — rewritten only on the owning sheet.
        #expect(NamedCells.rewriting("'Rate' * 12  # note", oldName: "rate",
                                     owningSheet: "Sheet 1", onOwningSheet: true,
                                     replacement: "'APR'")
                == "'APR' * 12  # note")
        #expect(NamedCells.rewriting("'Rate' * 12", oldName: "Rate",
                                     owningSheet: "Sheet 1", onOwningSheet: false,
                                     replacement: "'APR'") == nil)

        // Qualified — rewritten anywhere, but only with the owning sheet.
        #expect(NamedCells.rewriting("Budget!'Rate' + 'Rate'", oldName: "Rate",
                                     owningSheet: "Budget", onOwningSheet: false,
                                     replacement: "B:7")
                == "Budget!B:7 + 'Rate'")
        #expect(NamedCells.rewriting("Other!'Rate'", oldName: "Rate",
                                     owningSheet: "Budget", onOwningSheet: false,
                                     replacement: "B:7") == nil)

        // A quoted SHEET qualifier with the same spelling is left alone.
        #expect(NamedCells.rewriting("'Rate'!A:1 + 'Rate'", oldName: "Rate",
                                     owningSheet: "Sheet 1", onOwningSheet: true,
                                     replacement: "B:7")
                == "'Rate'!A:1 + B:7")

        // Multiple occurrences, all spliced.
        #expect(NamedCells.rewriting("'x' + 'x'", oldName: "x",
                                     owningSheet: nil, onOwningSheet: true,
                                     replacement: "A:1")
                == "A:1 + A:1")
    }
}
