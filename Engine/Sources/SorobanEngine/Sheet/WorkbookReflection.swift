import Anzan
import Foundation

/// The read-only Workbook reflection graph. A `Workbook` global plus the flat
/// `cell()`/`sheetNames()`/… functions hand the language opaque `.host` handles
/// it navigates uniformly (`.member`, `[…]`, `.method(…)`). Everything here is
/// READ-ONLY — no mutator is exposed, so a formula can inspect the workbook but
/// never change it (mutation is a deferred, log-only API).
///
/// Handles hold the store/sheet/grid WEAKLY: a stored handle
/// (`w = Workbook.worksheets[0]`) never keeps a removed sheet alive, and reads
/// after teardown throw cleanly instead of crashing. Cell reads route through
/// the ordinary `numericValue`/`displayValue` path, so dependency edges and
/// cycle detection come for free — a formula that reads a cell through the
/// reflection API recalculates when that cell changes, exactly like a plain
/// `A:1`. The `@unchecked Sendable` rests on the engine's single-threaded
/// evaluation discipline (the resolver closures are already non-Sendable).
///
/// Wired by `SheetStore` (see `installReflection`); nil in the CLI, where
/// `Workbook` and `cell()` are simply unknown.

/// `Workbook` — the root handle. `.worksheets` is the collection, `.sheetNames`
/// a quick array, `.count` the number of sheets.
final class WorkbookObject: HostObject, @unchecked Sendable {
    weak var store: SheetStore?

    init(store: SheetStore) { self.store = store }

    var typeName: String { "Workbook" }

    var description: String {
        let count = store?.sheets.count ?? 0
        return "Workbook(\(count) sheet\(count == 1 ? "" : "s"))"
    }

    func member(_ name: String) -> Value? {
        guard let store else { return nil }
        switch name {
        case "worksheets", "sheets":
            return .host(WorksheetCollection(store: store))
        case "sheetNames":
            return .array(store.sheets.map { .string($0.name) })
        case "count":
            return .number(BigDecimal(store.sheets.count))
        case "activeSheet":
            return .host(WorksheetObject(sheet: store.activeSheet))
        default:
            return nil
        }
    }
}

/// `Workbook.worksheets` — index by position (`[0]`, `[-1]` from the end) or by
/// name (`["Budget"]`). `.count` is the number of sheets.
final class WorksheetCollection: HostObject, @unchecked Sendable {
    weak var store: SheetStore?

    init(store: SheetStore) { self.store = store }

    var typeName: String { "Worksheets" }

    var description: String {
        "Worksheets(\(store?.sheets.count ?? 0))"
    }

    func member(_ name: String) -> Value? {
        guard let store else { return nil }
        if name == "count" { return .number(BigDecimal(store.sheets.count)) }
        return nil
    }

    func index(_ key: Value) -> Value? {
        guard let store else { return nil }
        switch key {
        case .number(let position):
            guard let raw = position.intValue else { return nil }
            // Negative indices count from the end (-1 is the last sheet).
            let resolved = raw < 0 ? store.sheets.count + raw : raw
            guard store.sheets.indices.contains(resolved) else { return nil }
            return .host(WorksheetObject(sheet: store.sheets[resolved]))
        case .string(let name):
            guard let sheet = store.sheet(named: name) else { return nil }
            return .host(WorksheetObject(sheet: sheet))
        default:
            return nil
        }
    }
}

/// One worksheet — `.name`, `.rowCount`/`.columnCount`, `.isData`, and the
/// `.cell("A", 2)` method returning a `Cell` handle.
final class WorksheetObject: HostObject, @unchecked Sendable {
    weak var sheet: Sheet?

    init(sheet: Sheet) { self.sheet = sheet }

    var typeName: String { "Worksheet" }

    var description: String {
        "Worksheet(\(sheet?.name ?? "—"))"
    }

    func isEqual(to other: any HostObject) -> Bool {
        guard let other = other as? WorksheetObject else { return false }
        return sheet === other.sheet
    }

    func member(_ name: String) -> Value? {
        guard let sheet else { return nil }
        switch name {
        case "name":
            return .string(sheet.name)
        case "rowCount":
            return .number(BigDecimal(Spreadsheet.rowCount))
        case "columnCount":
            return .number(BigDecimal(Spreadsheet.columnCount))
        case "isData":
            return .bool(sheet.isData)
        default:
            return nil
        }
    }

    func call(_ method: String, _ arguments: [Value]) throws -> Value {
        guard let sheet else {
            throw EngineError.domainError(message: "the worksheet is no longer available")
        }
        switch method {
        case "cell":
            return try CellObject.make(on: sheet.grid, arguments: arguments)
        default:
            throw EngineError.domainError(
                message: "Worksheet has no method '\(method)' — try .cell(\"A\", 1)")
        }
    }
}

/// One cell — `.value` (numeric, throws when the cell isn't a number, exactly
/// like a plain reference), `.text` (its displayed string), `.raw`/`.formula`
/// (the source), `.address`, `.isEmpty`.
final class CellObject: HostObject, @unchecked Sendable {
    weak var grid: Spreadsheet?
    let address: CellAddress

    init(grid: Spreadsheet, address: CellAddress) {
        self.grid = grid
        self.address = address
    }

    /// Builds a Cell handle from a `("A", 2)` argument pair, validating the
    /// column letters and row number into an in-range address.
    static func make(on grid: Spreadsheet, arguments: [Value]) throws -> Value {
        guard arguments.count == 2 else {
            throw EngineError.domainError(
                message: "cell() takes a column and a row — cell(\"A\", 1)")
        }
        guard case .string(let column) = arguments[0] else {
            throw EngineError.domainError(
                message: "cell()'s first argument is a column letter — cell(\"A\", 1)")
        }
        guard case .number(let rowValue) = arguments[1], let row = rowValue.intValue else {
            throw EngineError.domainError(
                message: "cell()'s second argument is a row number — cell(\"A\", 1)")
        }
        guard let address = CellAddress(columnName: column, rowNumber: row) else {
            throw EngineError.domainError(message: "cell \(column):\(row) is out of range")
        }
        return .host(CellObject(grid: grid, address: address))
    }

    var typeName: String { "Cell" }

    var description: String { "Cell(\(address))" }

    func isEqual(to other: any HostObject) -> Bool {
        guard let other = other as? CellObject else { return false }
        return grid === other.grid && address == other.address
    }

    func member(_ name: String) -> Value? {
        guard let grid else { return nil }
        switch name {
        case "value":
            // Routes through numericValue → records a dependency edge, so a
            // formula reading a cell this way recalcs when that cell changes.
            // A non-numeric cell throws on access (caught by the evaluator),
            // matching a direct reference — but member() can't throw, so a
            // non-number reads as the placeholder text instead. Use .text when
            // a cell may hold a label.
            return (try? grid.numericValue(column: address.columnName,
                                           row: address.rowNumber)).map(Value.number)
                ?? .string(Self.text(grid.displayValue(at: address)))
        case "text":
            return .string(Self.text(grid.displayValue(at: address)))
        case "raw", "formula":
            return .string(grid.raw(at: address))
        case "address":
            return .string(address.description)
        case "isEmpty":
            return .bool(grid.displayValue(at: address) == .empty)
        default:
            return nil
        }
    }

    /// The cell's displayed string — the human-readable face of any display.
    static func text(_ display: CellDisplay) -> String {
        switch display {
        case .empty: return ""
        case .text(let text): return text
        case .note(let comment): return "# \(comment)"
        case .value(let value): return value.description
        case .definition(let glyph): return glyph
        case .error(let message): return message
        case .slider(let info), .stepper(let info): return info.value.description
        case .checkbox(let info): return info.isOn ? "true" : "false"
        case .dropdown(let info): return info.value.displayText
        }
    }
}
