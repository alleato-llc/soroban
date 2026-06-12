import Anzan
import Foundation

/// The DEFAULT workbook-mutation implementations behind the log-only commands
/// `updateCell` / `addWorksheet` / `renameWorksheet` / `deleteWorksheet`
/// (wired in `installMutation`). They mutate the store directly — no undo, no
/// journal — which is what the CLI and headless tests want. The app overrides
/// `Calculator.hostMutationResolver` to make the SAME commands undoable through
/// `SheetModel`; these are the reference semantics that override must match.
///
/// A worksheet TARGET is either a `Worksheet` handle (`Workbook.worksheets[0]`)
/// or a sheet-name string — both resolve to an index via `sheetIndex(forTarget:)`.
extension SheetStore {
    // MARK: updateCell(cell, value)

    /// Sets a cell's raw contents from a value: a number becomes its digits, a
    /// string is taken verbatim (so `updateCell(c, "=B:1*2")` writes a formula
    /// and `updateCell(c, "Total")` a label). An empty string clears the cell.
    func mutateUpdateCell(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 2 else {
            throw EngineError.domainError(message: "updateCell(cell, value) takes a cell and a value")
        }
        guard case .host(let object) = arguments[0], let cell = object as? CellObject else {
            throw EngineError.domainError(
                message: "updateCell()'s first argument is a cell — e.g. cell(\"A\", 1)")
        }
        guard let grid = cell.grid else {
            throw EngineError.domainError(message: "that cell's sheet is no longer in the workbook")
        }
        let raw = try Self.rawText(from: arguments[1])
        grid.setCell(raw.isEmpty ? nil : raw, at: cell.address)
        recalculate() // setCell invalidates readers; recalc keeps cross-sheet fresh
        return arguments[1]
    }

    // MARK: addWorksheet(name)

    /// Adds an empty grid sheet and returns its handle.
    func mutateAddWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 1, case .string(let name) = arguments[0] else {
            throw EngineError.domainError(message: "addWorksheet(name) takes a sheet name")
        }
        let sheet = try addSheet(named: name)
        recalculate() // a new sheet may satisfy a previously-unknown qualifier
        return .host(WorksheetObject(sheet: sheet))
    }

    // MARK: renameWorksheet(sheet, newName)

    /// Renames a worksheet AND rewrites every `Old!A:1` / `'Old'!A:1` qualifier
    /// across all grid sheets — the same auto-rewrite the UI rename performs
    /// (references are by name; that's why you rename). Returns the handle.
    func mutateRenameWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 2 else {
            throw EngineError.domainError(
                message: "renameWorksheet(sheet, newName) takes a worksheet (or name) and the new name")
        }
        let index = try sheetIndex(forTarget: arguments[0])
        guard case .string(let newName) = arguments[1] else {
            throw EngineError.domainError(message: "renameWorksheet()'s new name is text")
        }
        let oldName = sheets[index].name
        try rename(at: index, to: newName) // validates + recalculates
        let resolved = sheets[index].name
        if oldName != resolved {
            for sheet in sheets where !sheet.isData {
                for (address, cell) in sheet.grid.cells {
                    if let rewritten = ReferenceRewriter.renamingSheet(
                        cell.raw, from: oldName, to: resolved) {
                        sheet.grid.setCell(rewritten, at: address)
                    }
                }
            }
            recalculate()
        }
        return .host(WorksheetObject(sheet: sheets[index]))
    }

    // MARK: deleteWorksheet(sheet)

    /// Removes a worksheet (refuses the last one) and returns the new count.
    /// Formulas referencing the removed sheet become "unknown sheet" errors,
    /// exactly as when a tab is removed in the UI.
    func mutateDeleteWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 1 else {
            throw EngineError.domainError(message: "deleteWorksheet(sheet) takes a worksheet or a sheet name")
        }
        let index = try sheetIndex(forTarget: arguments[0])
        try removeSheet(at: index) // validates (≥1 sheet) + recalculates
        return .number(BigDecimal(sheets.count))
    }

    // MARK: Shared helpers (public — the app's undoable override reuses them)

    /// A `Worksheet` handle for the sheet at `index` — the value the mutation
    /// commands return, built identically by the engine default and the app.
    public func worksheetHandle(at index: Int) -> Value {
        .host(WorksheetObject(sheet: sheets[index]))
    }

    /// Resolves a CELL handle (`cell("A", 1)` / `…cell("A", 1)`) to the sheet
    /// it lives on and its address — so the app can write it undoably.
    public func cellTarget(of value: Value) throws -> (sheetIndex: Int, address: CellAddress) {
        guard case .host(let object) = value, let cell = object as? CellObject else {
            throw EngineError.domainError(
                message: "updateCell()'s first argument is a cell — e.g. cell(\"A\", 1)")
        }
        guard let grid = cell.grid, let index = sheets.firstIndex(where: { $0.grid === grid }) else {
            throw EngineError.domainError(message: "that cell's sheet is no longer in the workbook")
        }
        return (index, cell.address)
    }

    /// Resolves a worksheet TARGET — a `Worksheet` handle or a name string —
    /// to its current index in the workbook.
    public func sheetIndex(forTarget value: Value) throws -> Int {
        switch value {
        case .string(let name):
            guard let index = sheets.firstIndex(where: {
                $0.name.compare(name, options: .caseInsensitive) == .orderedSame
            }) else {
                throw EngineError.domainError(message: "unknown sheet '\(name)'")
            }
            return index
        case .host(let object):
            guard let worksheet = object as? WorksheetObject, let sheet = worksheet.sheet,
                  let index = sheets.firstIndex(where: { $0 === sheet }) else {
                throw EngineError.domainError(message: "that worksheet is no longer in the workbook")
            }
            return index
        default:
            throw EngineError.domainError(
                message: "expected a worksheet or a sheet name, got \(value.kindName)")
        }
    }

    /// A value as a cell's raw text: numbers become digits, strings are
    /// verbatim. Structures/functions/handles can't live in a cell.
    public static func rawText(from value: Value) throws -> String {
        switch value {
        case .number(let number): return number.description
        case .string(let text): return text
        default:
            throw EngineError.domainError(
                message: "a cell holds a number or text, not \(value.kindName)")
        }
    }
}
