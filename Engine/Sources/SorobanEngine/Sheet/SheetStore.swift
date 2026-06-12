import Anzan
import Foundation

/// Shared by every sheet in a store. Two jobs:
///  1. The owning-sheet stack — while a formula on sheet X evaluates, its
///     unqualified `A:1` references must resolve against X, not whichever
///     tab is active in the UI.
///  2. Cross-sheet cycle detection — `Sheet1!A:1 → Sheet2!B:1 → Sheet1!A:1`
///     must report a circular reference, not recurse forever, so the
///     in-flight set is keyed by (sheet identity, address).
final class ResolutionContext {
    struct CellKey: Hashable {
        let sheet: ObjectIdentifier
        let address: CellAddress
    }

    var resolving: Set<CellKey> = []

    private var sheetStack: [Spreadsheet] = []
    private var keyStack: [CellKey] = []
    var currentSheet: Spreadsheet? { sheetStack.last }
    /// The cell whose formula is evaluating right now — dependency edges
    /// point at it.
    var currentKey: CellKey? { keyStack.last }

    func push(_ sheet: Spreadsheet, evaluating key: CellKey) {
        sheetStack.append(sheet)
        keyStack.append(key)
    }

    func pop() {
        sheetStack.removeLast()
        keyStack.removeLast()
    }

    // MARK: Dependency graph (edit → invalidate only the affected closure)

    /// source cell → cells whose formulas read it.
    private var dependents: [CellKey: Set<CellKey>] = [:]

    /// Range reads, per source sheet: rect + reader.
    private struct RangeRead: Hashable {
        let rows: ClosedRange<Int>
        let columns: ClosedRange<Int>
        let reader: CellKey
    }
    private var rangeDependents: [ObjectIdentifier: Set<RangeRead>] = [:]

    /// Registered sheets, weakly — invalidation needs to reach their memos.
    private var sheetAccessors: [ObjectIdentifier: () -> Spreadsheet?] = [:]

    func register(_ sheet: Spreadsheet) {
        sheetAccessors[ObjectIdentifier(sheet)] = { [weak sheet] in sheet }
    }

    /// Called when a formula reads one cell of `source`.
    func recordCellRead(of source: CellKey) {
        guard let reader = currentKey, reader != source else { return }
        dependents[source, default: []].insert(reader)
    }

    /// Called when a formula reads a rectangle of `sheet`.
    func recordRangeRead(sheet: ObjectIdentifier,
                         rows: ClosedRange<Int>, columns: ClosedRange<Int>) {
        guard let reader = currentKey else { return }
        rangeDependents[sheet, default: []].insert(
            RangeRead(rows: rows, columns: columns, reader: reader))
    }

    /// A cell changed: drop its memo and, transitively, every reader's —
    /// across sheets. Edges may be stale (a reader's formula changed since
    /// recording); that only over-invalidates, which is correctness-safe.
    func invalidate(_ start: CellKey) {
        var queue = [start]
        var visited: Set<CellKey> = []
        while let key = queue.popLast() {
            guard visited.insert(key).inserted else { continue }
            sheetAccessors[key.sheet]?()?.clearMemo(at: key.address)
            if let direct = dependents[key] {
                queue.append(contentsOf: direct)
            }
            if let ranges = rangeDependents[key.sheet] {
                for read in ranges
                where read.rows.contains(key.address.row)
                    && read.columns.contains(key.address.column) {
                    queue.append(read.reader)
                }
            }
        }
    }

    /// Everything is suspect (variables changed, sheets renamed/removed,
    /// workbook loaded): clear all memos and start the graph fresh.
    func invalidateEverything() {
        dependents.removeAll()
        rangeDependents.removeAll()
        for accessor in sheetAccessors.values {
            accessor()?.clearAllMemo()
        }
    }
}

/// One worksheet: a calculation grid plus its name and layout — or, when
/// `data` is set, a DATA sheet backed by a DataStore table (the grid then
/// exists but stays empty; edits write to the table, within its bounds).
public final class Sheet {
    public internal(set) var name: String
    public let grid: Spreadsheet
    public internal(set) var data: DataSheet?
    /// Sparse non-default sizes, in points (the app clamps).
    public var columnWidths: [Int: Double] = [:]
    public var rowHeights: [Int: Double] = [:]
    /// Sparse per-cell presentation (display-only — never touches the
    /// dependency graph). Defaults are pruned, not stored; empty cells may
    /// be formatted (fill a region before its data arrives).
    public var formats: [CellAddress: CellFormat] = [:]

    public var isData: Bool { data != nil }

    init(name: String, grid: Spreadsheet, data: DataSheet? = nil) {
        self.name = name
        self.grid = grid
        self.data = data
        grid.displayName = name
    }
}

/// An ordered collection of named worksheets sharing one Calculator.
/// Owns the resolver wiring: qualified references (`Budget!A:1`) route by
/// name; unqualified ones go to the formula's owning sheet, falling back to
/// the active sheet (the log's perspective).
public final class SheetStore {
    public static let maxSheets = 256
    public static let maxNameLength = 128

    public private(set) var sheets: [Sheet] = []
    public var activeIndex = 0 {
        didSet { activeIndex = min(max(activeIndex, 0), sheets.count - 1) }
    }

    public var activeSheet: Sheet { sheets[activeIndex] }

    private let calculator: Calculator
    private let context = ResolutionContext()

    /// The calculation log, for the `History` reflection API (log-only). Set by
    /// the host (the app's `CalculatorSession` via a small adapter); nil in the
    /// CLI/tests without a log, where `History` is simply unknown. Strong: the
    /// adapter back-references the session weakly, so there's no cycle.
    public var logSource: (any LogSource)?

    public init(calculator: Calculator) {
        self.calculator = calculator
        sheets = [Sheet(name: "Sheet 1",
                        grid: Spreadsheet(calculator: calculator, context: context))]

        calculator.cellResolver = { [weak self] sheetName, column, row in
            guard let self else { throw EngineError.domainError(message: "no sheets available") }
            // Unqualified refs inside a grid formula belong to the owning
            // grid (data sheets never own formulas).
            if sheetName == nil, let current = self.context.currentSheet {
                return try current.numericValue(column: column, row: row)
            }
            let target = try self.sheet(forReference: sheetName)
            if let data = target.data {
                return try data.numericValue(column: column, row: row)
            }
            return try target.grid.numericValue(column: column, row: row)
        }
        calculator.rangeResolver = { [weak self] sheetName, fc, fr, tc, tr in
            guard let self else { throw EngineError.domainError(message: "no sheets available") }
            if sheetName == nil, let current = self.context.currentSheet {
                return try current.numericValues(fromColumn: fc, fromRow: fr,
                                                 toColumn: tc, toRow: tr)
            }
            let target = try self.sheet(forReference: sheetName)
            if let data = target.data {
                return try data.numericValues(fromColumn: fc, fromRow: fr,
                                              toColumn: tc, toRow: tr)
            }
            return try target.grid.numericValues(fromColumn: fc, fromRow: fr,
                                                 toColumn: tc, toRow: tr)
        }

        // Named cells: 'Projected Rate' routes like an unqualified A:1
        // (owning sheet, active from the log); Budget!'Rate' by sheet name.
        calculator.nameResolver = { [weak self] sheetName, name in
            guard let self else { throw EngineError.domainError(message: "no sheets available") }
            if sheetName == nil, let current = self.context.currentSheet {
                return try current.numericValue(forName: name)
            }
            let target = try self.sheet(forReference: sheetName)
            guard !target.isData else {
                throw EngineError.domainError(
                    message: "data sheets don't have named cells")
            }
            return try target.grid.numericValue(forName: name)
        }

        // Sheet-scoped λ/𝑖 definitions: resolved against the formula's
        // owning sheet (mid-evaluation) or the active tab (log input).
        calculator.scopedFunctionResolver = { [weak self] name in
            guard let self else { return nil }
            return self.scopeSheet.definedFunction(named: name)
        }
        calculator.scopedVariableResolver = { [weak self] name in
            guard let self else { return nil }
            return try self.scopeSheet.definedValue(named: name)
        }
        calculator.scopedDefinitionOwner = { [weak self] name in
            guard let self else { return nil }
            return self.scopeSheet.definitionOwner(named: name)
        }
        calculator.scopedDataTypeResolver = { [weak self] name in
            guard let self else { return nil }
            return self.scopeSheet.definedDataType(named: name)
        }

        installReflection()
        installMutation()
    }

    /// Wires the read-only Workbook reflection API: the `Workbook` global and
    /// the flat `cell()`/`sheetName()`/`sheetNames()`/`rowCount()`/`columnCount()`
    /// accessors. All hand back `.host(…)` handles (or plain values) the
    /// language navigates uniformly. The CLI leaves these unwired — `Workbook`
    /// and `cell()` are simply unknown there.
    private func installReflection() {
        calculator.hostValueResolver = { [weak self] name, inLog in
            guard let self else { return nil }
            // Reflection names are case-sensitive, like data types/constructors.
            switch name {
            case "Workbook":
                return .host(WorkbookObject(store: self))
            case "History":
                // Log-only: the array on the log path, nil in a cell (where the
                // name then degrades to a text label, not an error).
                guard inLog, let source = self.logSource else { return nil }
                return HistoryReflection.value(from: source)
            default:
                return nil
            }
        }
        calculator.hostFunctionResolver = { [weak self] name, arguments in
            guard let self else { return nil }
            switch name {
            case "cell":
                // cell(col, row) on the scope sheet; cell(sheet, col, row) by name.
                return try self.reflectCell(arguments)
            case "sheetNames":
                try Self.expectArity(arguments, 0, name)
                return .array(self.sheets.map { .string($0.name) })
            case "sheetName":
                try Self.expectArity(arguments, 0, name)
                return .string(self.scopeSheetItem.name)
            case "rowCount":
                try Self.expectArity(arguments, 0, name)
                return .number(BigDecimal(Spreadsheet.rowCount))
            case "columnCount":
                try Self.expectArity(arguments, 0, name)
                return .number(BigDecimal(Spreadsheet.columnCount))
            default:
                return nil // not a reflection function — fall through to unknown
            }
        }
    }

    /// Wires the DEFAULT (direct, no-undo) workbook mutation API:
    /// `updateCell` / `addWorksheet` / `renameWorksheet` / `deleteWorksheet`.
    /// These change the workbook, so they run from the LOG only — `inLog` is
    /// false during cell recalc and the resolver throws then (recalc stays
    /// reproducible). The app OVERRIDES `hostMutationResolver` afterward to make
    /// the same commands undoable; this default is what the CLI/tests see.
    /// Implementations live in `SheetStore+Mutation.swift`.
    private func installMutation() {
        calculator.hostMutationResolver = { [weak self] name, arguments, inLog in
            guard let self else { return nil }
            guard Self.mutationNames.contains(name) else { return nil }
            guard inLog else {
                throw EngineError.domainError(message:
                    "'\(name)' changes the workbook — it runs in the calculation log, not a cell")
            }
            switch name {
            case "updateCell": return try self.mutateUpdateCell(arguments)
            case "addWorksheet": return try self.mutateAddWorksheet(arguments)
            case "renameWorksheet": return try self.mutateRenameWorksheet(arguments)
            case "deleteWorksheet": return try self.mutateDeleteWorksheet(arguments)
            default: return nil
            }
        }
    }

    /// The mutation command names — shared by the engine default and the app
    /// override so both gate the same set log-only.
    public static let mutationNames: Set<String> =
        ["updateCell", "addWorksheet", "renameWorksheet", "deleteWorksheet"]

    /// The Sheet whose definitions/grid are in scope right now (owning sheet
    /// mid-evaluation, active tab from the log) — the Sheet-level companion to
    /// `scopeSheet`, which returns its grid.
    private var scopeSheetItem: Sheet {
        if let current = context.currentSheet {
            return sheets.first { $0.grid === current } ?? activeSheet
        }
        return activeSheet
    }

    /// `cell(col, row)` on the scope sheet, or `cell(sheet, col, row)` by name.
    private func reflectCell(_ arguments: [Value]) throws -> Value {
        switch arguments.count {
        case 2:
            return try CellObject.make(on: scopeSheet, arguments: arguments)
        case 3:
            guard case .string(let sheetName) = arguments[0] else {
                throw EngineError.domainError(
                    message: "cell()'s first argument is a sheet name — cell(\"Budget\", \"A\", 1)")
            }
            guard let sheet = sheet(named: sheetName) else {
                throw EngineError.domainError(message: "unknown sheet '\(sheetName)'")
            }
            return try CellObject.make(on: sheet.grid, arguments: Array(arguments[1...]))
        default:
            throw EngineError.domainError(
                message: "cell() takes (column, row) or (sheet, column, row)")
        }
    }

    private static func expectArity(_ arguments: [Value], _ count: Int, _ name: String) throws {
        guard arguments.count == count else {
            throw EngineError.domainError(
                message: "\(name)() takes \(count) argument\(count == 1 ? "" : "s")")
        }
    }

    /// The grid whose definitions are in scope right now.
    private var scopeSheet: Spreadsheet {
        context.currentSheet ?? activeSheet.grid
    }

    /// Where a reference points: named sheet, else the active one (log input).
    private func sheet(forReference sheetName: String?) throws -> Sheet {
        guard let sheetName else { return activeSheet }
        guard let sheet = sheet(named: sheetName) else {
            throw EngineError.domainError(message: "unknown sheet '\(sheetName)'")
        }
        return sheet
    }

    public func sheet(named name: String) -> Sheet? {
        sheets.first { $0.name.compare(name, options: .caseInsensitive) == .orderedSame }
    }

    // MARK: Structure

    @discardableResult
    public func addSheet() throws -> Sheet {
        guard sheets.count < Self.maxSheets else {
            throw EngineError.domainError(message: "a workbook holds at most \(Self.maxSheets) sheets")
        }
        var n = sheets.count + 1
        while sheet(named: "Sheet \(n)") != nil { n += 1 }
        let sheet = Sheet(name: "Sheet \(n)",
                          grid: Spreadsheet(calculator: calculator, context: context))
        sheets.append(sheet)
        return sheet
    }

    /// Adds an empty grid sheet with a specific (validated) name — the
    /// mutation API's `addWorksheet(name)`. `addSheet()` (auto-named) stays
    /// the UI's +-button path.
    @discardableResult
    public func addSheet(named name: String) throws -> Sheet {
        guard sheets.count < Self.maxSheets else {
            throw EngineError.domainError(message: "a workbook holds at most \(Self.maxSheets) sheets")
        }
        let validated = try Self.validated(name: name, existing: sheets, exceptIndex: nil)
        let sheet = makeSheet(name: validated)
        sheets.append(sheet)
        return sheet
    }

    public func removeSheet(at index: Int) throws {
        guard sheets.count > 1 else {
            throw EngineError.domainError(message: "a workbook needs at least one sheet")
        }
        guard sheets.indices.contains(index) else { return }
        sheets.remove(at: index)
        activeIndex = min(activeIndex, sheets.count - 1)
        recalculate() // formulas referencing the removed sheet become errors
    }

    public func rename(at index: Int, to newName: String) throws {
        guard sheets.indices.contains(index) else { return }
        let name = try Self.validated(name: newName, existing: sheets, exceptIndex: index)
        sheets[index].name = name
        sheets[index].grid.displayName = name
        recalculate() // references resolve by name
    }

    /// Trimmed, non-empty, ≤128 chars, unique (case-insensitive), and free of
    /// the characters that would break the `Sheet!A:1` syntax.
    public static func validated(name: String, existing: [Sheet], exceptIndex: Int?) throws -> String {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else {
            throw EngineError.domainError(message: "sheet names can't be empty")
        }
        guard trimmed.count <= maxNameLength else {
            throw EngineError.domainError(message: "sheet names are limited to \(maxNameLength) characters")
        }
        guard !trimmed.contains("!"), !trimmed.contains("'") else {
            throw EngineError.domainError(message: "sheet names can't contain ! or '")
        }
        for (i, sheet) in existing.enumerated() where i != exceptIndex {
            if sheet.name.compare(trimmed, options: .caseInsensitive) == .orderedSame {
                throw EngineError.domainError(message: "a sheet named '\(trimmed)' already exists")
            }
        }
        return trimmed
    }

    /// Drops every sheet's memo — a log variable changed, a sheet was
    /// renamed/removed, or a workbook loaded.
    public func recalculate() {
        for sheet in sheets {
            sheet.grid.recalculate()
        }
    }

    /// Replaces everything (workbook open / new).
    public func replaceSheets(_ newSheets: [Sheet], activeName: String?) {
        precondition(!newSheets.isEmpty)
        sheets = newSheets
        activeIndex = newSheets.firstIndex {
            $0.name.compare(activeName ?? "", options: .caseInsensitive) == .orderedSame
        } ?? 0
        recalculate()
    }

    /// A fresh empty sheet built against this store's shared context —
    /// for workbook loading.
    public func makeSheet(name: String) -> Sheet {
        Sheet(name: name, grid: Spreadsheet(calculator: calculator, context: context))
    }

    /// A DATA sheet over a DataStore table (for loading/import).
    public func makeDataSheet(name: String, data: DataSheet) -> Sheet {
        Sheet(name: name, grid: Spreadsheet(calculator: calculator, context: context), data: data)
    }

    /// Imports a data sheet at the end (name pre-validated by the caller's
    /// uniquifier or validated here).
    @discardableResult
    public func addDataSheet(named name: String, data: DataSheet) throws -> Sheet {
        guard sheets.count < Self.maxSheets else {
            throw EngineError.domainError(message: "a workbook holds at most \(Self.maxSheets) sheets")
        }
        let validated = try Self.validated(name: name, existing: sheets, exceptIndex: nil)
        let sheet = makeDataSheet(name: validated, data: data)
        sheets.append(sheet)
        recalculate()
        return sheet
    }
}
