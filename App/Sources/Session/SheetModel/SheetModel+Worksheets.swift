import SorobanEngine
import Foundation

// MARK: Worksheets (tabs)

extension SheetModel {
    var sheetNames: [String] {
        _ = generation
        return store.sheets.map(\.name)
    }

    var activeSheetIndex: Int {
        _ = generation
        return store.activeIndex
    }

    var activeSheetName: String {
        _ = generation
        return store.activeSheet.name
    }

    var sheetCount: Int { store.sheets.count }
    var canAddSheet: Bool { store.sheets.count < SheetStore.maxSheets }
    var canRemoveSheet: Bool { store.sheets.count > 1 }

    func activateSheet(at index: Int) {
        guard index != store.activeIndex, store.sheets.indices.contains(index) else { return }
        endEditing()
        deselect()
        store.activeIndex = index
        generation += 1
        persistAfterChange() // the active tab is saved state
    }

    /// Returns an error message for the UI, or nil on success.
    func addSheet() -> String? {
        do {
            try store.addSheet()
            store.activeIndex = store.sheets.count - 1
            endEditing()
            deselect()
            generation += 1
            persistAfterChange()
            return nil
        } catch {
            return "\(error as? EngineError ?? .domainError(message: "\(error)"))"
        }
    }

    func removeActiveSheet() -> String? {
        let removedName = store.activeSheet.name
        let removedTable = store.activeSheet.data?.table
        do {
            try store.removeSheet(at: store.activeIndex)
            if let removedTable {
                try? dataStore?.dropTable(name: removedTable)
            }
            endEditing()
            deselect()
            // Edits on the removed sheet can no longer be undone.
            dropUndoHistory(forSheet: removedName)
            generation += 1
            persistAfterChange()
            return nil
        } catch {
            return "\(error as? EngineError ?? .domainError(message: "\(error)"))"
        }
    }

    /// Renames the active sheet and AUTO-REWRITES every `Old!A:1` /
    /// `'Old Name'!A:1` qualifier across all sheets (that's why you rename —
    /// the named-cell precedent). Undo ordering mirrors nameCell: the rename
    /// step is pushed BEFORE the rewrite steps, so ⌘Z pops rewrites first
    /// and the rename last — after undoing everything, formulas and the
    /// sheet name agree again.
    func renameActiveSheet(to name: String) -> String? {
        let oldName = store.activeSheet.name
        do {
            try store.rename(at: store.activeIndex, to: name)
        } catch {
            return "\(error as? EngineError ?? .domainError(message: "\(error)"))"
        }
        let newName = store.activeSheet.name
        guard oldName != newName else { return nil } // no-op
        // Keep undo history working across the rename.
        retagUndoHistory(from: oldName, to: newName)
        pushUndo(SheetEdit(sheetName: newName,
                           kind: .renameSheet(old: oldName, new: newName)))

        // One undoable step PER AFFECTED SHEET (the undo model is
        // sheet-tagged), exactly like named-cell rewrites.
        for sheet in store.sheets where !sheet.isData {
            var changes: [CellChange] = []
            for (address, cell) in sheet.grid.cells {
                guard let rewritten = ReferenceRewriter.renamingSheet(
                    cell.raw, from: oldName, to: newName) else { continue }
                changes.append(CellChange(address: address, old: cell.raw, new: rewritten))
                sheet.grid.setCell(rewritten, at: address)
            }
            guard !changes.isEmpty else { continue }
            pushUndo(SheetEdit(sheetName: sheet.name, kind: .cells(changes)))
            persistCellEdits(changes.map { ($0.address, $0.new) }, sheetName: sheet.name)
        }
        store.recalculate()
        generation += 1
        persistAfterChange()
        return nil
    }
}
