import Anzan
import Foundation
import SQLite3

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

/// SQLite-backed storage for DATA sheets — imported records at volumes the
/// JSON manifest shouldn't carry. Values are read lazily (indexed lookups /
/// single range queries), so opening a workbook never loads tables into
/// memory. SQLite ships with macOS; this is a deliberately small wrapper.
public final class DataStore {
    public struct TableInfo: Equatable, Sendable {
        public let name: String
        public let rows: Int
        public let columns: Int
    }

    public enum DataStoreError: Error, CustomStringConvertible {
        case sqlite(String)
        public var description: String {
            if case .sqlite(let message) = self { return "data store error: \(message)" }
            return "data store error"
        }
    }

    public let url: URL
    private var db: OpaquePointer?
    private var valueStatement: OpaquePointer?

    public init(url: URL) throws {
        self.url = url
        guard sqlite3_open_v2(url.path, &db,
                              SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, nil) == SQLITE_OK else {
            throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
        }
        try exec("PRAGMA journal_mode=WAL")
        try exec("""
            CREATE TABLE IF NOT EXISTS tables(
                name TEXT PRIMARY KEY COLLATE NOCASE,
                rows INTEGER NOT NULL, cols INTEGER NOT NULL)
            """)
        try exec("""
            CREATE TABLE IF NOT EXISTS cells(
                t TEXT COLLATE NOCASE, r INTEGER, c INTEGER, v TEXT NOT NULL,
                PRIMARY KEY(t, r, c)) WITHOUT ROWID
            """)
    }

    deinit {
        sqlite3_finalize(valueStatement)
        sqlite3_close(db)
    }

    private func exec(_ sql: String) throws {
        guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else {
            throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
        }
    }

    // MARK: Tables

    public func tables() throws -> [TableInfo] {
        var statement: OpaquePointer?
        defer { sqlite3_finalize(statement) }
        guard sqlite3_prepare_v2(db, "SELECT name, rows, cols FROM tables ORDER BY name",
                                 -1, &statement, nil) == SQLITE_OK else {
            throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
        }
        var result: [TableInfo] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            result.append(TableInfo(name: String(cString: sqlite3_column_text(statement, 0)),
                                    rows: Int(sqlite3_column_int64(statement, 1)),
                                    columns: Int(sqlite3_column_int64(statement, 2))))
        }
        return result
    }

    public func info(name: String) -> TableInfo? {
        (try? tables())?.first { $0.name.compare(name, options: .caseInsensitive) == .orderedSame }
    }

    /// Imports a rectangular table (one transaction; empty values skipped —
    /// the store is sparse like the grid).
    public func createTable(name: String, rows: [[String]]) throws {
        let columnCount = rows.map(\.count).max() ?? 0
        try exec("BEGIN")
        do {
            var statement: OpaquePointer?
            defer { sqlite3_finalize(statement) }
            guard sqlite3_prepare_v2(
                db, "INSERT OR REPLACE INTO cells(t, r, c, v) VALUES(?, ?, ?, ?)",
                -1, &statement, nil) == SQLITE_OK else {
                throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
            }
            for (r, row) in rows.enumerated() {
                for (c, value) in row.enumerated() where !value.isEmpty {
                    sqlite3_reset(statement)
                    sqlite3_bind_text(statement, 1, name, -1, SQLITE_TRANSIENT)
                    sqlite3_bind_int64(statement, 2, Int64(r))
                    sqlite3_bind_int64(statement, 3, Int64(c))
                    sqlite3_bind_text(statement, 4, value, -1, SQLITE_TRANSIENT)
                    guard sqlite3_step(statement) == SQLITE_DONE else {
                        throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
                    }
                }
            }
            var meta: OpaquePointer?
            defer { sqlite3_finalize(meta) }
            sqlite3_prepare_v2(db, "INSERT OR REPLACE INTO tables(name, rows, cols) VALUES(?, ?, ?)",
                               -1, &meta, nil)
            sqlite3_bind_text(meta, 1, name, -1, SQLITE_TRANSIENT)
            sqlite3_bind_int64(meta, 2, Int64(rows.count))
            sqlite3_bind_int64(meta, 3, Int64(columnCount))
            guard sqlite3_step(meta) == SQLITE_DONE else {
                throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
            }
            try exec("COMMIT")
        } catch {
            try? exec("ROLLBACK")
            throw error
        }
    }

    /// Edits one cell: empty/nil deletes (the store is sparse). The imported
    /// table is the workbook's own copy — edits never touch the source CSV.
    public func setValue(_ value: String?, table: String, row: Int, column: Int) throws {
        var statement: OpaquePointer?
        defer { sqlite3_finalize(statement) }
        if let value, !value.isEmpty {
            guard sqlite3_prepare_v2(
                db, "INSERT OR REPLACE INTO cells(t, r, c, v) VALUES(?, ?, ?, ?)",
                -1, &statement, nil) == SQLITE_OK else {
                throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
            }
            sqlite3_bind_text(statement, 1, table, -1, SQLITE_TRANSIENT)
            sqlite3_bind_int64(statement, 2, Int64(row))
            sqlite3_bind_int64(statement, 3, Int64(column))
            sqlite3_bind_text(statement, 4, value, -1, SQLITE_TRANSIENT)
        } else {
            guard sqlite3_prepare_v2(
                db, "DELETE FROM cells WHERE t = ? AND r = ? AND c = ?",
                -1, &statement, nil) == SQLITE_OK else {
                throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
            }
            sqlite3_bind_text(statement, 1, table, -1, SQLITE_TRANSIENT)
            sqlite3_bind_int64(statement, 2, Int64(row))
            sqlite3_bind_int64(statement, 3, Int64(column))
        }
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
        }
    }

    public func dropTable(name: String) throws {
        try exec("BEGIN")
        var cells: OpaquePointer?
        sqlite3_prepare_v2(db, "DELETE FROM cells WHERE t = ?", -1, &cells, nil)
        sqlite3_bind_text(cells, 1, name, -1, SQLITE_TRANSIENT)
        sqlite3_step(cells)
        sqlite3_finalize(cells)
        var meta: OpaquePointer?
        sqlite3_prepare_v2(db, "DELETE FROM tables WHERE name = ?", -1, &meta, nil)
        sqlite3_bind_text(meta, 1, name, -1, SQLITE_TRANSIENT)
        sqlite3_step(meta)
        sqlite3_finalize(meta)
        try exec("COMMIT")
    }

    // MARK: Values (0-based row/column)

    public func value(table: String, row: Int, column: Int) -> String? {
        if valueStatement == nil {
            sqlite3_prepare_v2(db, "SELECT v FROM cells WHERE t = ? AND r = ? AND c = ?",
                               -1, &valueStatement, nil)
        }
        guard let statement = valueStatement else { return nil }
        sqlite3_reset(statement)
        sqlite3_bind_text(statement, 1, table, -1, SQLITE_TRANSIENT)
        sqlite3_bind_int64(statement, 2, Int64(row))
        sqlite3_bind_int64(statement, 3, Int64(column))
        guard sqlite3_step(statement) == SQLITE_ROW,
              let text = sqlite3_column_text(statement, 0) else { return nil }
        return String(cString: text)
    }

    /// All stored values in a rectangle, one query (for range expansion).
    public func values(table: String, rows: ClosedRange<Int>,
                       columns: ClosedRange<Int>) throws -> [(row: Int, column: Int, value: String)] {
        var statement: OpaquePointer?
        defer { sqlite3_finalize(statement) }
        guard sqlite3_prepare_v2(db, """
            SELECT r, c, v FROM cells
            WHERE t = ? AND r BETWEEN ? AND ? AND c BETWEEN ? AND ?
            ORDER BY r, c
            """, -1, &statement, nil) == SQLITE_OK else {
            throw DataStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
        }
        sqlite3_bind_text(statement, 1, table, -1, SQLITE_TRANSIENT)
        sqlite3_bind_int64(statement, 2, Int64(rows.lowerBound))
        sqlite3_bind_int64(statement, 3, Int64(rows.upperBound))
        sqlite3_bind_int64(statement, 4, Int64(columns.lowerBound))
        sqlite3_bind_int64(statement, 5, Int64(columns.upperBound))
        var result: [(Int, Int, String)] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            result.append((Int(sqlite3_column_int64(statement, 0)),
                           Int(sqlite3_column_int64(statement, 1)),
                           String(cString: sqlite3_column_text(statement, 2))))
        }
        return result
    }
}

/// A worksheet backed by a DataStore table: lazily fetched, editable within
/// its OWN row bounds (a data sheet can exceed the grid's 1,000 rows —
/// references like `Sales!C:50000` are valid against its table size).
public final class DataSheet {
    public let table: String
    public let rowCount: Int
    public let columnCount: Int
    private let store: DataStore

    public init?(table: String, store: DataStore) {
        guard let info = store.info(name: table) else { return nil }
        self.table = table
        self.store = store
        self.rowCount = info.rows
        self.columnCount = min(info.columns, Spreadsheet.columnCount)
    }

    /// Raw stored text (UI display / copy). 0-based.
    public func rawValue(row: Int, column: Int) -> String {
        store.value(table: table, row: row, column: column) ?? ""
    }

    /// Edits one cell of the imported copy (0-based; within the table's
    /// rectangle — growing the table is a future feature).
    public func setRawValue(_ value: String, row: Int, column: Int) throws {
        guard row >= 0, row < rowCount, column >= 0, column < columnCount else {
            throw EngineError.domainError(message: "cell is outside this data sheet")
        }
        try store.setValue(value, table: table, row: row, column: column)
    }

    /// Resolver semantics, mirroring the grid: empty → 0, text → error.
    /// Row is 1-based (reference syntax), bounded by the TABLE, not the grid.
    public func numericValue(column: String, row: Int) throws -> BigDecimal {
        guard let columnIndex = CellAddress.columnIndex(forName: column),
              columnIndex < columnCount, (1...max(rowCount, 1)).contains(row) else {
            throw EngineError.domainError(message: "cell \(column):\(row) is outside this data sheet")
        }
        guard let text = store.value(table: table, row: row - 1, column: columnIndex) else {
            return .zero
        }
        guard let value = BigDecimal(string: text) else {
            throw EngineError.domainError(message: "cell \(column):\(row) is not a number")
        }
        return value
    }

    /// Range expansion, grid-consistent: numeric values only, text and empty
    /// skipped. One SQL query regardless of rectangle size.
    public func numericValues(fromColumn: String, fromRow: Int,
                              toColumn: String, toRow: Int) throws -> [BigDecimal] {
        guard let from = CellAddress.columnIndex(forName: fromColumn),
              let to = CellAddress.columnIndex(forName: toColumn),
              fromRow >= 1, toRow >= 1 else {
            throw EngineError.domainError(message: "range is outside this data sheet")
        }
        let rows = min(fromRow, toRow) - 1...max(fromRow, toRow) - 1
        let columns = min(from, to)...max(from, to)
        return try store.values(table: table, rows: rows, columns: columns)
            .compactMap { BigDecimal(string: $0.value) } // text (headers) skipped
    }
}

/// Minimal RFC 4180-style CSV: quoted fields, escaped quotes (""), CR/LF/CRLF.
public enum CSV {
    public static func parse(_ text: String) -> [[String]] {
        var rows: [[String]] = []
        var row: [String] = []
        var field = ""
        var inQuotes = false
        var iterator = text.makeIterator()
        var pending: Character?

        func endField() { row.append(field); field = "" }
        func endRow() {
            endField()
            if !(row.count == 1 && row[0].isEmpty) { rows.append(row) }
            row = []
        }

        while let c = pending ?? iterator.next() {
            pending = nil
            if inQuotes {
                if c == "\"" {
                    if let next = iterator.next() {
                        if next == "\"" { field.append("\"") }   // escaped quote
                        else { inQuotes = false; pending = next } // closing quote
                    } else { inQuotes = false }
                } else {
                    field.append(c)
                }
            } else {
                switch c {
                case "\"" where field.isEmpty: inQuotes = true
                case ",": endField()
                // NB: "\r\n" is ONE Swift Character (grapheme cluster) —
                // match all three line-end forms as single characters.
                case "\n", "\r", "\r\n": endRow()
                default: field.append(c)
                }
            }
        }
        if !field.isEmpty || !row.isEmpty { endRow() }
        return rows
    }

    /// The inverse of `parse`: rows → RFC 4180-style text (\n line ends).
    /// Fields are quoted only when they need it (comma, quote, or newline);
    /// quotes double inside quoted fields. `parse(encode(rows)) == rows`.
    public static func encode(_ rows: [[String]]) -> String {
        rows.map { row in
            row.map(encodeField).joined(separator: ",")
        }.joined(separator: "\n") + (rows.isEmpty ? "" : "\n")
    }

    private static func encodeField(_ field: String) -> String {
        guard field.contains(where: { $0 == "," || $0 == "\"" || $0.isNewline }) else {
            return field
        }
        return "\"" + field.replacingOccurrences(of: "\"", with: "\"\"") + "\""
    }
}
