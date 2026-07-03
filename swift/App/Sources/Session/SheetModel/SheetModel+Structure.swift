import SorobanEngine
import Foundation

// MARK: Structural edits (insert/delete rows & columns)

extension SheetModel {
    /// Inserts `count` rows above `row` (0-based grid index). One undoable
    /// step; persistence rides the SNAPSHOT path — the cell journal can't
    /// represent shifts.
    func insertRows(at row: Int, count: Int = 1) -> String? {
        structural { try store.insertSlots(axis: .row, at: row, count: count,
                                           in: store.activeSheet) }
    }

    func deleteRows(at row: Int, count: Int = 1) -> String? {
        structural { try store.deleteSlots(axis: .row, at: row, count: count,
                                           in: store.activeSheet) }
    }

    func insertColumns(at column: Int, count: Int = 1) -> String? {
        structural { try store.insertSlots(axis: .column, at: column, count: count,
                                           in: store.activeSheet) }
    }

    func deleteColumns(at column: Int, count: Int = 1) -> String? {
        structural { try store.deleteSlots(axis: .column, at: column, count: count,
                                           in: store.activeSheet) }
    }

    /// The selected row span (0-based), for the header menus' pluralization.
    var selectedRowSpan: (start: Int, count: Int)? {
        guard let rect = selectionRect else { return nil }
        return (rect.rows.lowerBound, rect.rows.count)
    }

    var selectedColumnSpan: (start: Int, count: Int)? {
        guard let rect = selectionRect else { return nil }
        return (rect.columns.lowerBound, rect.columns.count)
    }

    private func structural(_ op: () throws -> SheetStore.StructuralChange) -> String? {
        endEditing()
        let change: SheetStore.StructuralChange
        do {
            change = try op()
        } catch {
            return "\(error as? EngineError ?? .domainError(message: "\(error)"))"
        }
        deselect()
        pushUndo(SheetEdit(sheetName: store.activeSheet.name, kind: .structure(change)))
        generation += 1
        persistAfterChange()
        return nil
    }
}
