import Testing
import Foundation
@testable import Anzan
@testable import SorobanEngine

/// Cross-ecosystem `.soroban` interchange — the Swift half. `examples/
/// interchange.soroban` is **Rust-authored** (see
/// `rust/engine/examples/author_interchange.rs`) and is opened + computed here
/// AND by Rust's `interchange.rs` test — so a workbook written by Rust is proven
/// to compute in Swift. Its mirror, `examples/mortgage.soroban`, is
/// Swift-authored and read by both suites (`WorkbookTests` / `workbook_edges`),
/// covering the other direction.
@Suite("Interchange")
struct InterchangeTests {
    private var repoRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SorobanEngineTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // Engine
            .deletingLastPathComponent() // swift
            .deletingLastPathComponent() // repo root
    }

    @Test("Swift opens the Rust-authored interchange workbook")
    func swiftOpensRustAuthored() throws {
        let url = repoRoot.appendingPathComponent("examples/interchange.soroban")
        let workbook = try Workbook.decode(try Data(contentsOf: url))
        #expect(workbook.sheets.map(\.name) == ["Sheet 1"])

        // Restore the app way: env first, then sheets into a wired SheetStore.
        let calc = Calculator()
        calc.restoreSession(from: workbook)
        let store = SheetStore(calculator: calc)
        var sheets: [Sheet] = []
        for payload in workbook.sheets {
            let sheet = store.makeSheet(name: payload.name)
            var contents: [CellAddress: String] = [:]
            for (key, raw) in payload.cells { contents[CellAddress(key: key)!] = raw }
            sheet.grid.load(contents)
            for (key, name) in payload.names {
                try sheet.grid.setCellName(name, at: CellAddress(key: key)!)
            }
            sheets.append(sheet)
        }
        store.replaceSheets(sheets, activeName: workbook.activeSheet)

        func value(_ key: String) -> BigDecimal? {
            guard case .value(let v) = store.sheet(named: "Sheet 1")!
                .grid.displayValue(at: CellAddress(key: key)!) else { return nil }
            return v
        }
        #expect(value("A:2") == BigDecimal(string: "2400"))  // =A:1 * 2
        #expect(value("B:1") == BigDecimal(string: "42"))     // =double(21) — user function
        #expect(value("B:2") == BigDecimal(string: "8.25"))   // =100 * taxRate — log variable
        #expect(value("C:1") == BigDecimal(string: "1201"))   // ='Base' + 1 — named cell

        // Env restored: the data-type record and the saved bit-format variable.
        #expect(calc.environment.userVariables["origin"]?.description == "Point(x: 3, y: 4)")
        #expect(calc.environment.userVariables["myfmt"] != nil)
    }
}
