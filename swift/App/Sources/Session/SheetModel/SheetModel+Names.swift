import SorobanEngine
import Foundation

// MARK: Named cells ('Projected Rate')

extension SheetModel {
    func cellName(at address: CellAddress) -> String? {
        _ = generation
        guard !store.activeSheet.isData else { return nil }
        return store.activeSheet.grid.cellNames[address]
    }

    /// Adds or renames a cell's name. Renames AUTO-UPDATE every referencing
    /// formula across all sheets (that's why you rename). Returns a
    /// user-facing error message, or nil on success.
    ///
    /// Undo ordering: the name step is pushed BEFORE the rewrite steps, so
    /// ⌘Z pops rewrites first and the name last — after undoing everything,
    /// formulas and the name agree again.
    func nameCell(_ name: String, at address: CellAddress) -> String? {
        let grid = store.activeSheet.grid
        let oldName = grid.cellNames[address]
        do {
            try grid.setCellName(name, at: address)
        } catch {
            return "\(error as? EngineError ?? .domainError(message: "\(error)"))"
        }
        let newName = grid.cellNames[address]
        guard oldName != newName else { return nil } // no-op rename
        pushUndo(SheetEdit(sheetName: store.activeSheet.name,
                           kind: .name(NameChange(address: address, old: oldName, new: newName))))
        if let oldName, let newName {
            rewriteReferences(to: oldName, owner: store.activeSheet,
                              replacement: "'\(newName)'")
        }
        generation += 1
        persistAfterChange()
        return nil
    }

    /// How many formulas reference this cell's name — drives the remove
    /// dialog ("break / replace with addresses / cancel").
    func referenceCount(toNameAt address: CellAddress) -> Int {
        guard let name = cellName(at: address) else { return 0 }
        let owner = store.activeSheet
        var count = 0
        for sheet in store.sheets where !sheet.isData {
            for (_, cell) in sheet.grid.cells {
                if NamedCells.rewriting(cell.raw, oldName: name, owningSheet: owner.name,
                                        onOwningSheet: sheet === owner,
                                        replacement: "'\(name)'") != nil {
                    count += 1
                }
            }
        }
        return count
    }

    enum NameRemoval {
        case breakReferences   // they show "unknown name" errors
        case inlineAddresses   // rewrite references to the cell's address
    }

    /// Undo ordering (reverse of nameCell's): rewrites happen first, the
    /// name step is pushed LAST — so ⌘Z restores the name first (making the
    /// rewritten references resolvable again), then reverts the rewrites.
    func removeCellName(at address: CellAddress, mode: NameRemoval) {
        let grid = store.activeSheet.grid
        guard let name = grid.cellNames[address] else { return }
        if mode == .inlineAddresses {
            rewriteReferences(to: name, owner: store.activeSheet,
                              replacement: "\(address)")
        }
        try? grid.setCellName(nil, at: address)
        pushUndo(SheetEdit(sheetName: store.activeSheet.name,
                           kind: .name(NameChange(address: address, old: name, new: nil))))
        generation += 1
        persistAfterChange()
    }

    /// Token-precise rewrite of every reference to `name` across all sheets
    /// (scoping mirrors resolution — see NamedCells.rewriting). One undoable
    /// step PER AFFECTED SHEET (the undo model is sheet-tagged).
    private func rewriteReferences(to name: String, owner: Sheet, replacement: String) {
        for sheet in store.sheets where !sheet.isData {
            var changes: [CellChange] = []
            for (cellAddress, cell) in sheet.grid.cells {
                guard let rewritten = NamedCells.rewriting(
                    cell.raw, oldName: name, owningSheet: owner.name,
                    onOwningSheet: sheet === owner, replacement: replacement) else { continue }
                changes.append(CellChange(address: cellAddress, old: cell.raw, new: rewritten))
                sheet.grid.setCell(rewritten, at: cellAddress)
            }
            guard !changes.isEmpty else { continue }
            pushUndo(SheetEdit(sheetName: sheet.name, kind: .cells(changes)))
            persistCellEdits(changes.map { ($0.address, $0.new) }, sheetName: sheet.name)
        }
        store.recalculate()
    }
}
