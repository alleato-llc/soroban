import SorobanEngine
import Foundation

// MARK: Workbook mutation (log commands → undoable, persisted, UI-refreshing)

extension SheetModel {
    /// Replaces `SheetStore`'s DIRECT mutation resolver (set in its init) with
    /// one that routes the same log-only commands —
    /// `updateCell`/`addWorksheet`/`renameWorksheet`/`deleteWorksheet` —
    /// through the app's undo/persistence/observation machinery, so a command
    /// typed in the log behaves exactly like the equivalent UI edit. Gating
    /// (log-only; rejected during cell recalc) is unchanged.
    func installMutationOverride() {
        calculator.hostMutationResolver = { [weak self] name, arguments, inLog in
            guard let self else { return nil }
            guard SheetStore.mutationNames.contains(name) else { return nil }
            guard inLog else {
                throw EngineError.domainError(message:
                    "'\(name)' changes the workbook — it runs in the calculation log, not a cell")
            }
            // The log path that invokes this is already on the main actor
            // (CalculatorSession.submit); SheetModel is @MainActor.
            return try MainActor.assumeIsolated {
                switch name {
                case "updateCell": return try self.commandUpdateCell(arguments)
                case "addWorksheet": return try self.commandAddWorksheet(arguments)
                case "renameWorksheet": return try self.commandRenameWorksheet(arguments)
                case "deleteWorksheet": return try self.commandDeleteWorksheet(arguments)
                default: return nil
                }
            }
        }
    }

    // MARK: updateCell — one undoable cell write on the cell's own sheet

    private func commandUpdateCell(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 2 else {
            throw EngineError.domainError(message: "updateCell(cell, value) takes a cell and a value")
        }
        let (index, address) = try store.cellTarget(of: arguments[0])
        let raw = try SheetStore.rawText(from: arguments[1])
        let sheet = store.sheets[index]
        let old = sheet.grid.raw(at: address)
        guard old != raw else { return arguments[1] } // no-op
        sheet.grid.setCell(raw.isEmpty ? nil : raw, at: address)
        store.recalculate()
        pushUndo(SheetEdit(sheetName: sheet.name,
                           kind: .cells([CellChange(address: address, old: old, new: raw)])))
        persistCellEdits([(address, raw)], sheetName: sheet.name)
        generation += 1
        return arguments[1]
    }

    // MARK: addWorksheet — like the +-button (not undoable, but persisted)

    private func commandAddWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 1, case .string(let name) = arguments[0] else {
            throw EngineError.domainError(message: "addWorksheet(name) takes a sheet name")
        }
        try store.addSheet(named: name)
        store.activeIndex = store.sheets.count - 1
        endEditing()
        deselect()
        store.recalculate()
        generation += 1
        persistAfterChange()
        return store.worksheetHandle(at: store.sheets.count - 1)
    }

    // MARK: renameWorksheet — reuses the undoable active-sheet rename+rewrite

    private func commandRenameWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 2 else {
            throw EngineError.domainError(message:
                "renameWorksheet(sheet, newName) takes a worksheet (or name) and the new name")
        }
        let index = try store.sheetIndex(forTarget: arguments[0])
        guard case .string(let newName) = arguments[1] else {
            throw EngineError.domainError(message: "renameWorksheet()'s new name is text")
        }
        store.activeIndex = index // rename operates on the active sheet
        if let message = renameActiveSheet(to: newName) {
            throw EngineError.domainError(message: message)
        }
        return store.worksheetHandle(at: index) // rename never reorders
    }

    // MARK: deleteWorksheet — reuses the active-sheet removal

    private func commandDeleteWorksheet(_ arguments: [Value]) throws -> Value {
        guard arguments.count == 1 else {
            throw EngineError.domainError(message: "deleteWorksheet(sheet) takes a worksheet or a sheet name")
        }
        let index = try store.sheetIndex(forTarget: arguments[0])
        store.activeIndex = index
        if let message = removeActiveSheet() {
            throw EngineError.domainError(message: message)
        }
        return .number(BigDecimal(store.sheets.count))
    }
}
