import Testing
import Foundation
@testable import Anzan
@testable import SorobanEngine

@Suite("Workbook format")
struct WorkbookTests {
    @Test func roundTripsCellsAndVariables() throws {
        let original = Workbook(
            cells: ["A:1": "Q1 revenue", "B:1": "1200", "B:3": "=B:1 * rate"],
            variables: [
                "rate": .number(BigDecimal(string: "0.0825")!),
                "big": .number(BigDecimal(string: "1e+40")!),
                "third": .number(try BigDecimal.one / BigDecimal(3)),
            ])

        let decoded = try Workbook.decode(try original.encode())
        #expect(decoded == original)
        #expect(decoded.parsedVariables["rate"] == .number(BigDecimal(string: "0.0825")!))
        #expect(decoded.parsedVariables["big"] == .number(BigDecimal(string: "1e40")!))
        #expect(decoded.parsedVariables["third"] == .number(try BigDecimal.one / BigDecimal(3)))
    }

    @Test func encodesAVersionedPrettyEnvelope() throws {
        let data = try Workbook(cells: ["A:1": "1"], variables: [:]).encode()
        let text = String(decoding: data, as: UTF8.self)
        #expect(text.contains("\"format\" : \"soroban-workbook\""))
        #expect(text.contains("\"version\" : 2"))
        #expect(text.contains("\n")) // pretty-printed for diffability
    }

    @Test func rejectsForeignAndMalformedFiles() {
        #expect(throws: WorkbookError.notAWorkbook) {
            try Workbook.decode(Data("not json at all".utf8))
        }
        #expect(throws: WorkbookError.notAWorkbook) {
            try Workbook.decode(Data(#"{"some": "other json"}"#.utf8))
        }
        #expect(throws: WorkbookError.notAWorkbook) {
            try Workbook.decode(Data(
                #"{"format": "other-app", "version": 1, "cells": {}, "variables": {}}"#.utf8))
        }
    }

    @Test func rejectsFutureVersions() {
        #expect(throws: WorkbookError.unsupportedVersion(99)) {
            try Workbook.decode(Data(
                #"{"format": "soroban-workbook", "version": 99, "cells": {}, "variables": {}}"#.utf8))
        }
    }

    @Test func dataTypesRoundTripAndOlderFilesDecodeEmpty() throws {
        let calc = Calculator()
        _ = calc.evaluate("data Person { name: String, age: Number } # who")
        let decoded = try Workbook.decode(try Workbook(
            sheets: [Workbook.SheetPayload(name: "Sheet 1", cells: [:])],
            variables: [:], dataTypes: calc.environment.userDataTypes).encode())
        #expect(decoded.dataTypes == ["Person": "data Person { name: String, age: Number } # who"])

        // Files written before data types existed decode with the default.
        let older = try Workbook.decode(Data(
            #"{"format": "soroban-workbook", "version": 1, "cells": {}, "variables": {}}"#.utf8))
        #expect(older.dataTypes.isEmpty)
    }

    @Test func handEditedBadVariablesAreDropped() throws {
        let decoded = try Workbook.decode(Data("""
            {"format": "soroban-workbook", "version": 1,
             "cells": {}, "variables": {"good": "1.5", "bad": "not-a-number"}}
            """.utf8))
        #expect(decoded.parsedVariables == ["good": .number(BigDecimal(string: "1.5")!)])
    }

    @Test func functionsRoundTrip() throws {
        let calc = Calculator()
        _ = calc.evaluate("f(x) = x * 2")
        _ = calc.evaluate("g(x) = f(x) + 1")

        let workbook = Workbook(cells: [:], variables: [:],
                                functions: calc.environment.allUserFunctions)
        let decoded = try Workbook.decode(try workbook.encode())
        #expect(decoded.functions.sorted() == ["f(x) = x * 2", "g(x) = f(x) + 1"])

        // Apply into a fresh session the way WorkbookManager does.
        let fresh = Calculator()
        for source in decoded.functions.sorted() {
            _ = fresh.evaluate(source)
        }
        #expect(try fresh.evaluate("g(20)").get() == .value(BigDecimal(41)))
    }

    @Test func typedOverloadsRoundTrip() throws {
        // Two operator overloads of `+`/`*` for Point survive save/reload —
        // the reason `functions` is a list, not a name→source map.
        let calc = Calculator()
        _ = calc.evaluate("data Point { x: Number, y: Number }")
        _ = calc.evaluate("+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)")
        _ = calc.evaluate("*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)")

        let workbook = Workbook(
            sheets: [.init(name: "Sheet 1", cells: [:])],
            variables: calc.environment.userVariables,
            functions: calc.environment.allUserFunctions,
            dataTypes: calc.environment.userDataTypes)
        let decoded = try Workbook.decode(try workbook.encode())
        #expect(decoded.functions.count == 2) // both overloads persisted, not collapsed

        let fresh = Calculator()
        fresh.restoreSession(from: decoded)
        _ = fresh.evaluate("p = Point(x: 1, y: 2)")
        _ = fresh.evaluate("q = Point(x: 10, y: 20)")
        #expect(try fresh.evaluate("(p + q).x").get() == .value(BigDecimal(11)))
        #expect(try fresh.evaluate("(p * 3).y").get() == .value(BigDecimal(6)))
    }

    @Test func filesWithoutFunctionsFieldStillOpen() throws {
        let decoded = try Workbook.decode(Data("""
            {"format": "soroban-workbook", "version": 1,
             "cells": {"A:1": "42"}, "variables": {}}
            """.utf8))
        #expect(decoded.functions.isEmpty)
        #expect(decoded.columnWidths.isEmpty)
        #expect(decoded.rowHeights.isEmpty)
        #expect(decoded.cells == ["A:1": "42"])
    }

    @Test func layoutRoundTrips() throws {
        let workbook = Workbook(cells: [:], variables: [:],
                                columnWidths: ["A": 150, "C": 60],
                                rowHeights: ["5": 48])
        let decoded = try Workbook.decode(try workbook.encode())
        #expect(decoded.columnWidths == ["A": 150, "C": 60])
        #expect(decoded.rowHeights == ["5": 48])
    }

    @Test func shippedExampleWorkbookOpensAndComputes() throws {
        // examples/mortgage.soroban is documentation — keep it honest.
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // (file) → SorobanEngineTests
            .deletingLastPathComponent()  // → Tests
            .deletingLastPathComponent()  // → Engine
            .deletingLastPathComponent()  // → repo root
        let url = repoRoot.appendingPathComponent("examples/mortgage.soroban")
        let workbook = try Workbook.decode(try Data(contentsOf: url))
        #expect(workbook.sheets.map(\.name) == ["Loan", "What If"])

        // Apply the way the app does: a SheetStore + functions + cells.
        let calc = Calculator()
        let store = SheetStore(calculator: calc)
        for source in workbook.functions.sorted() {
            _ = calc.evaluate(source)
        }
        var sheets: [Sheet] = []
        for payload in workbook.sheets {
            let sheet = store.makeSheet(name: payload.name)
            var contents: [CellAddress: String] = [:]
            for (key, raw) in payload.cells {
                contents[try #require(CellAddress(key: key))] = raw
            }
            sheet.grid.load(contents)
            sheets.append(sheet)
        }
        store.replaceSheets(sheets, activeName: workbook.activeSheet)

        // $350k at 6.5% APR over 30 years → -$2,212.24/month.
        let loan = try #require(store.sheet(named: "Loan"))
        let payment = loan.grid.displayValue(at: CellAddress(column: 1, row: 4)) // B:5
        guard case .value(let monthly) = payment else {
            Issue.record("expected a computed payment, got \(payment)")
            return
        }
        #expect(monthly == BigDecimal(string: "-2212.24")!)

        // The What If sheet reads the Loan sheet cross-sheet.
        let whatIf = try #require(store.sheet(named: "What If"))
        guard case .value(let extra) = whatIf.grid.displayValue(at: CellAddress(column: 1, row: 1)) else {
            Issue.record("expected cross-sheet extra-cost value")
            return
        }
        #expect(extra == BigDecimal(string: "235.01")!) // +1% APR costs $235.01/mo more

        // The documented function carries its doc comment.
        #expect(calc.documentation(for: "monthly")?.summary.contains("monthly loan payment") == true)
        // No #ERR anywhere in the example.
        for sheet in store.sheets {
            for address in sheet.grid.raws.keys {
                if case .error(let message) = sheet.grid.displayValue(at: address) {
                    Issue.record("example cell \(sheet.name)!\(address) errors: \(message)")
                }
            }
        }
    }

    @Test func replaceUserVariablesAffectsEvaluation() throws {
        let calc = Calculator()
        calc.environment.replaceUserVariables(["rate": .number(BigDecimal(string: "0.1")!)])
        #expect(try calc.evaluate("100 * rate").get() == .value(BigDecimal(10)))

        calc.environment.replaceUserVariables([:])
        // The VARIABLE is gone. (Bare `rate` alone now resolves to the
        // finance builtin as a function value, so probe numerically.)
        #expect(calc.evaluate("100 * rate").isFailure)
    }
}
