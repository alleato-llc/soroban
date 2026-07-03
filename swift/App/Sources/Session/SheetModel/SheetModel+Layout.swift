import SorobanEngine
import Foundation

// MARK: Grid layout (resizable columns/rows)

extension SheetModel {
    /// The font size the default geometry below is tuned for (92×24 at 14pt).
    static let baseFontSize: CGFloat = 14
    static let defaultColumnWidth: CGFloat = 92
    static let defaultRowHeight: CGFloat = 24
    static let columnWidthRange: ClosedRange<CGFloat> = 40...400
    static let rowHeightRange: ClosedRange<CGFloat> = 18...120

    /// Defaults scale with the app font so a larger font gets proportionally
    /// larger cells; a column/row the user resized keeps its explicit size.
    var defaultColumnWidthScaled: CGFloat {
        (Self.defaultColumnWidth * gridFontSize / Self.baseFontSize).clamped(to: Self.columnWidthRange)
    }
    var defaultRowHeightScaled: CGFloat {
        (Self.defaultRowHeight * gridFontSize / Self.baseFontSize).clamped(to: Self.rowHeightRange)
    }

    func width(ofColumn column: Int) -> CGFloat {
        _ = generation // layout is per-sheet; switches re-render via generation
        return store.activeSheet.columnWidths[column].map { CGFloat($0) } ?? defaultColumnWidthScaled
    }

    func height(ofRow row: Int) -> CGFloat {
        _ = generation
        return store.activeSheet.rowHeights[row].map { CGFloat($0) } ?? defaultRowHeightScaled
    }

    /// In-flight drag, shown as a guide line. The actual size applies on
    /// release: mutating `columnWidths`/`rowHeights` live would re-run every
    /// alive row body per drag tick — only the guide overlay observes the
    /// previews.
    func previewColumnResize(_ width: CGFloat, forColumn column: Int) {
        columnResizePreview = ResizePreview(index: column,
                                            size: width.clamped(to: Self.columnWidthRange))
    }

    func previewRowResize(_ height: CGFloat, forRow row: Int) {
        rowResizePreview = ResizePreview(index: row,
                                         size: height.clamped(to: Self.rowHeightRange))
    }

    /// Drag ended — apply the previewed size (one re-layout, not 120/s).
    func endColumnResize() {
        guard let preview = columnResizePreview else { return }
        store.activeSheet.columnWidths[preview.index] = Double(preview.size)
        columnResizePreview = nil
        generation += 1
        finishLayoutChange()
    }

    func endRowResize() {
        guard let preview = rowResizePreview else { return }
        store.activeSheet.rowHeights[preview.index] = Double(preview.size)
        rowResizePreview = nil
        generation += 1
        finishLayoutChange()
    }

    func resetColumnWidth(forColumn column: Int) {
        store.activeSheet.columnWidths.removeValue(forKey: column)
        generation += 1
        finishLayoutChange()
    }

    func resetRowHeight(forRow row: Int) {
        store.activeSheet.rowHeights.removeValue(forKey: row)
        generation += 1
        finishLayoutChange()
    }

    /// Layout is workbook data — persist and dirty like a cell edit.
    private func finishLayoutChange() {
        persistAfterChange()
    }
}
