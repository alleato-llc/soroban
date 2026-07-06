import Foundation
import SorobanEngine
// Pasteboard access goes through the platform-neutral `Clipboard` seam
// (NSPasteboard on macOS, UIPasteboard on iPadOS) — no direct AppKit here.

// MARK: Clipboard (TSV — interoperates with Excel/Numbers)

extension SheetModel {
    /// The selection's raw contents, tab-separated, one line per row.
    func selectionTSV() -> String? {
        guard let rect = selectionRect else { return nil }
        return rect.rows.map { row in
            rect.columns.map { column in
                raw(at: CellAddress(column: column, row: row))
            }.joined(separator: "\t")
        }.joined(separator: "\n")
    }

    /// Clears every cell in the selection as one undoable edit.
    func clearSelection() {
        guard let rect = selectionRect else { return }
        var changes: [(CellAddress, String)] = []
        for row in rect.rows {
            for column in rect.columns {
                changes.append((CellAddress(column: column, row: row), ""))
            }
        }
        applyEdit(changes)
    }

    /// The second pasteboard representation an in-app COPY writes alongside
    /// the plain TSV: the copy's origin, so paste can adjust relative
    /// references by the move offset. External apps see only the TSV.
    struct CellsClipboard: Codable {
        let anchor: String // "A:1" — the copied rect's top-left
        let tsv: String    // must match the plain-string content, else stale
    }

    /// ⌘C and the context menu both land here (SpreadsheetView's
    /// onCopyCommand defers to this so both pasteboard types are written).
    func copySelectionToPasteboard() {
        guard let rect = selectionRect, let tsv = selectionTSV() else { return }
        let anchor = CellAddress(column: rect.columns.lowerBound,
                                 row: rect.rows.lowerBound)
        let cells = try? JSONEncoder().encode(
            CellsClipboard(anchor: "\(anchor)", tsv: tsv))
        Clipboard.writeCells(tsv: tsv, cells: cells)
    }

    /// Cut writes ONLY plain TSV — cut-paste keeps references verbatim
    /// (Excel's no-adjust-on-move, without the move-references machinery).
    func cutSelectionToPasteboard() {
        guard let tsv = selectionTSV() else { return }
        Clipboard.write(string: tsv)
        clearSelection()
    }

    func pasteFromPasteboard() {
        guard let text = Clipboard.readString() else { return }
        // Our own copy? (The TSV must match — a newer copy from another app
        // replaces the string but leaves our stale custom data behind.)
        if let data = Clipboard.readCellsData(),
           let payload = try? JSONDecoder().decode(CellsClipboard.self, from: data),
           payload.tsv == text,
           let source = CellAddress(key: payload.anchor) {
            pasteCopiedCells(tsv: text, copiedFrom: source)
        } else {
            paste(tsv: text) // external text pastes verbatim
        }
    }

    /// In-app paste: every cell adjusts by the copy → paste offset
    /// (pins hold their axes; refs pushed off the grid become refError()).
    func pasteCopiedCells(tsv: String, copiedFrom source: CellAddress) {
        guard let target = selected else { return }
        paste(tsv: tsv, adjustingByRows: target.row - source.row,
              columns: target.column - source.column)
    }

    // MARK: Fill (⌘D / ⌘R — the formula-propagation gesture)

    /// Fills the selection downward from its top row; a single-row selection
    /// fills from the row above it (Excel's ⌘D). One undoable edit.
    func fillDown() {
        fill(axis: .row)
    }

    /// Same, rightward from the left column (⌘R).
    func fillRight() {
        fill(axis: .column)
    }

    private func fill(axis: ReferenceRewriter.Axis) {
        guard editing == nil, var rect = selectionRect else { return }
        // Single-line selection: the source is the neighbor before it.
        if axis == .row, rect.rows.count == 1 {
            guard rect.rows.lowerBound > 0 else { return }
            rect.rows = (rect.rows.lowerBound - 1)...rect.rows.upperBound
        }
        if axis == .column, rect.columns.count == 1 {
            guard rect.columns.lowerBound > 0 else { return }
            rect.columns = (rect.columns.lowerBound - 1)...rect.columns.upperBound
        }

        var changes: [(CellAddress, String)] = []
        switch axis {
        case .row:
            guard rect.rows.count > 1 else { return }
            for column in rect.columns {
                let source = raw(at: CellAddress(column: column, row: rect.rows.lowerBound))
                for (offset, row) in rect.rows.dropFirst().enumerated() {
                    changes.append((CellAddress(column: column, row: row),
                                    adjusted(source, byRows: offset + 1, columns: 0)))
                }
            }
        case .column:
            guard rect.columns.count > 1 else { return }
            for row in rect.rows {
                let source = raw(at: CellAddress(column: rect.columns.lowerBound, row: row))
                for (offset, column) in rect.columns.dropFirst().enumerated() {
                    changes.append((CellAddress(column: column, row: row),
                                    adjusted(source, byRows: 0, columns: offset + 1)))
                }
            }
        }
        applyEdit(changes)
    }

    private func adjusted(_ raw: String, byRows rows: Int, columns: Int) -> String {
        guard !raw.isEmpty else { return raw }
        return ReferenceRewriter.adjustingRelative(raw, byRows: rows, byColumns: columns) ?? raw
    }

    /// Right-clicking a cell OUTSIDE the current selection retargets the
    /// selection there first, so menu actions apply where the user clicked.
    func retargetSelection(toInclude cell: CellAddress) {
        let inSelection = selectionRect.map {
            $0.rows.contains(cell.row) && $0.columns.contains(cell.column)
        } ?? false
        if !inSelection {
            select(cell)
        }
    }

    /// Pastes a TSV block starting at the anchor (clipped to the grid) as
    /// one undoable edit; the selection grows to cover the pasted area.
    /// Non-zero offsets adjust every field's relative references (in-app
    /// copies); external text pastes verbatim.
    func paste(tsv: String, adjustingByRows rows: Int = 0, columns: Int = 0) {
        guard let anchor = selected else { return }
        var text = tsv
        if text.hasSuffix("\n") { text.removeLast() } // trailing terminator

        var changes: [(CellAddress, String)] = []
        var bottomRight = anchor
        for (rowOffset, line) in text.split(separator: "\n", omittingEmptySubsequences: false)
            .enumerated() {
            let row = anchor.row + rowOffset
            guard row < Spreadsheet.rowCount else { break }
            for (columnOffset, field) in line.split(separator: "\t", omittingEmptySubsequences: false)
                .enumerated() {
                let column = anchor.column + columnOffset
                guard column < Spreadsheet.columnCount else { break }
                let address = CellAddress(column: column, row: row)
                changes.append((address, adjusted(String(field), byRows: rows, columns: columns)))
                bottomRight = address
            }
        }
        guard !changes.isEmpty else { return }
        applyEdit(changes)
        selectionExtent = bottomRight == anchor ? nil : bottomRight
    }
}
