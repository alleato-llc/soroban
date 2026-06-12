import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Data store (SQLite)")
struct DataStoreTests {
    private func makeStore() throws -> (DataStore, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("soroban-data-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent("data.sqlite")
        return (try DataStore(url: url), url)
    }

    @Test func createReadDropPersist() throws {
        let (store, url) = try makeStore()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }

        try store.createTable(name: "sales", rows: [
            ["month", "amount"],
            ["jan", "1200.50"],
            ["feb", "", "stray"],   // ragged row, empty cell skipped
        ])

        let info = try #require(store.info(name: "sales"))
        #expect(info.rows == 3)
        #expect(info.columns == 3) // widest row wins
        #expect(store.value(table: "sales", row: 1, column: 1) == "1200.50")
        #expect(store.value(table: "sales", row: 2, column: 1) == nil) // empty skipped
        #expect(store.value(table: "SALES", row: 0, column: 0) == "month") // NOCASE

        // Reopen from disk — it's real persistence, not memory.
        let reopened = try DataStore(url: url)
        #expect(reopened.value(table: "sales", row: 1, column: 0) == "jan")

        try store.dropTable(name: "sales")
        #expect(store.info(name: "sales") == nil)
        #expect(store.value(table: "sales", row: 0, column: 0) == nil)
    }

    @Test func rectangleQuery() throws {
        let (store, url) = try makeStore()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try store.createTable(name: "t", rows: (0..<100).map { r in ["\(r)", "x\(r)"] })

        let values = try store.values(table: "t", rows: 10...12, columns: 0...0)
        #expect(values.map(\.value) == ["10", "11", "12"])
    }

    @Test func dataSheetSemantics() throws {
        let (store, url) = try makeStore()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try store.createTable(name: "sales", rows: [
            ["month", "amount"],     // header row (text)
            ["jan", "100"],
            ["feb", "250.5"],
            ["mar", ""],             // empty amount
        ])
        let sheet = try #require(DataSheet(table: "sales", store: store))
        #expect(sheet.rowCount == 4)
        #expect(sheet.columnCount == 2)

        // 1-based reference semantics, bounded by the table.
        #expect(try sheet.numericValue(column: "B", row: 2) == BigDecimal(100))
        #expect(try sheet.numericValue(column: "B", row: 4) == .zero)       // empty → 0
        #expect(throws: EngineError.self) { try sheet.numericValue(column: "B", row: 1) } // header text
        #expect(throws: EngineError.self) { try sheet.numericValue(column: "C", row: 1) } // beyond cols
        #expect(throws: EngineError.self) { try sheet.numericValue(column: "B", row: 5) } // beyond rows

        // Ranges skip the header text and empties (grid-consistent).
        let values = try sheet.numericValues(fromColumn: "B", fromRow: 1, toColumn: "B", toRow: 4)
        #expect(values == [BigDecimal(100), BigDecimal(string: "250.5")!])
        #expect(sheet.rawValue(row: 0, column: 1) == "amount")
    }

    @Test func editsWriteThroughAndPersist() throws {
        let (store, url) = try makeStore()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try store.createTable(name: "sales", rows: [
            ["month", "amount"],
            ["jan", "100"],
        ])
        let sheet = try #require(DataSheet(table: "sales", store: store))

        // Overwrite, blank (sparse delete), and fill an empty cell.
        try sheet.setRawValue("125.5", row: 1, column: 1)
        #expect(sheet.rawValue(row: 1, column: 1) == "125.5")
        #expect(try sheet.numericValue(column: "B", row: 2) == BigDecimal(string: "125.5")!)
        try sheet.setRawValue("", row: 1, column: 0)
        #expect(sheet.rawValue(row: 1, column: 0) == "")
        #expect(try sheet.numericValue(column: "A", row: 2) == .zero) // empty → 0

        // Bounds: the table's shape is fixed — no growing yet.
        #expect(throws: EngineError.self) { try sheet.setRawValue("x", row: 2, column: 0) }
        #expect(throws: EngineError.self) { try sheet.setRawValue("x", row: 0, column: 2) }

        // Durable: a fresh handle on the same file sees the edits.
        let reopened = try DataStore(url: url)
        #expect(reopened.value(table: "sales", row: 1, column: 1) == "125.5")
        #expect(reopened.value(table: "sales", row: 1, column: 0) == nil)
    }

    @Test func bigTableAggregatesStayExact() throws {
        // 50,000 rows — far beyond the grid — summed exactly.
        let (store, url) = try makeStore()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try store.createTable(name: "big", rows: (1...50_000).map { ["\($0)", "0.1"] })

        let sheet = try #require(DataSheet(table: "big", store: store))
        let values = try sheet.numericValues(fromColumn: "B", fromRow: 1,
                                             toColumn: "B", toRow: 50_000)
        let total = values.reduce(BigDecimal.zero, +)
        #expect(total == BigDecimal(5000)) // 0.1 × 50,000 exactly — no float drift
        #expect(try sheet.numericValue(column: "A", row: 50_000) == BigDecimal(50_000))
    }
}

@Suite("CSV parsing")
struct CSVTests {
    @Test func encodeQuotesOnlyWhenNeeded() {
        let rows = [["plain", "with,comma", "with \"quotes\"", "multi\nline", ""],
                    ["1200", "second row"]]
        let encoded = CSV.encode(rows)
        #expect(encoded ==
            "plain,\"with,comma\",\"with \"\"quotes\"\"\",\"multi\nline\",\n"
            + "1200,second row\n")
        // The contract: a perfect round-trip through parse.
        #expect(CSV.parse(encoded) == rows)
        #expect(CSV.encode([]) == "")
    }

    @Test func coversTheUsualSuspects() {
        #expect(CSV.parse("a,b,c\n1,2,3") == [["a", "b", "c"], ["1", "2", "3"]])
        #expect(CSV.parse("a,\"b, with comma\",c") == [["a", "b, with comma", "c"]])
        #expect(CSV.parse("\"he said \"\"hi\"\"\",2") == [["he said \"hi\"", "2"]])
        #expect(CSV.parse("a,b\r\n1,2\r\n") == [["a", "b"], ["1", "2"]])   // CRLF
        #expect(CSV.parse("a,b\r1,2") == [["a", "b"], ["1", "2"]])         // bare CR
        #expect(CSV.parse("\"multi\nline\",x") == [["multi\nline", "x"]])  // newline in quotes
        #expect(CSV.parse("a,,c") == [["a", "", "c"]])                     // empty field
        #expect(CSV.parse("") == [])
        #expect(CSV.parse("solo") == [["solo"]])
    }
}
