import SorobanEngine
import Foundation

// MARK: Cell formatting (display-only; route every mutation through here)

extension SheetModel {
    func format(at address: CellAddress) -> CellFormat {
        _ = generation
        return store.activeSheet.formats[address] ?? CellFormat()
    }

    /// Applies one transform to every cell of the selection as ONE undoable
    /// step. Default-valued results are pruned from the sparse map.
    func applyFormat(_ transform: (inout CellFormat) -> Void) {
        guard let rect = selectionRect else { return }
        let sheet = store.activeSheet
        var changes: [FormatChange] = []
        for row in rect.rows {
            for column in rect.columns {
                let address = CellAddress(column: column, row: row)
                let old = sheet.formats[address] ?? CellFormat()
                var new = old
                transform(&new)
                guard new != old else { continue }
                changes.append(FormatChange(address: address, old: old, new: new))
                sheet.formats[address] = new.isDefault ? nil : new
            }
        }
        guard !changes.isEmpty else { return }
        pushUndo(SheetEdit(sheetName: sheet.name, kind: .formats(changes)))
        generation += 1
        persistAfterChange() // snapshot path — format edits skip the cell journal
    }

    /// Numbers' toggle rule: if EVERY selected cell already has the flag,
    /// clear it; otherwise set it everywhere.
    func toggleStyle(_ keyPath: WritableKeyPath<CellFormat, Bool>) {
        guard let rect = selectionRect else { return }
        let allSet = rect.rows.allSatisfy { row in
            rect.columns.allSatisfy { column in
                format(at: CellAddress(column: column, row: row))[keyPath: keyPath]
            }
        }
        applyFormat { $0[keyPath: keyPath] = !allSet }
    }

    /// The anchor cell's format — what the Format menu shows checkmarks for.
    var selectionFormat: CellFormat? {
        guard let selected else { return nil }
        return format(at: selected)
    }
}
