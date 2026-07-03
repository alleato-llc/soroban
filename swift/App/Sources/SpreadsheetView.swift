import SwiftUI
import SorobanEngine
import UniformTypeIdentifiers

/// The mini-spreadsheet: A–Z columns × 100 rows. Column headers stay pinned
/// while scrolling vertically; the whole grid scrolls horizontally.
///
/// Interaction model: single click selects (highlight — copy/cut/paste/delete
/// target, arrow keys move), double click or Return opens the editor.
struct SpreadsheetView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager
    @FocusState private var gridFocused: Bool

    private var theme: Theme { themeManager.current }
    private var sheet: SheetModel { session.sheet }

    static let baseGutterWidth: CGFloat = 44
    /// The row-number gutter scales with the font like the cells, so wide row
    /// numbers don't clip at large sizes.
    private var gutterWidth: CGFloat {
        (Self.baseGutterWidth * sheet.gridFontSize / SheetModel.baseFontSize).rounded()
    }

    var body: some View {
        ScrollView(.horizontal) {
            VStack(spacing: 0) {
                headerRow
                Divider()
                ScrollView(.vertical) {
                    LazyVStack(spacing: 0) {
                        // Data sheets browse up to 10k rows; grids show all.
                        ForEach(0..<sheet.visibleRowCount, id: \.self) { row in
                            gridRow(row)
                        }
                    }
                    // Row-resize guide: lives in scroll-content coordinates
                    // so it tracks the dragged row even mid-scroll.
                    .overlay(alignment: .topLeading) {
                        rowResizeGuide
                    }
                }
            }
            // Column-resize guide spans header + grid.
            .overlay(alignment: .topLeading) {
                columnResizeGuide
            }
        }
        .background(theme.windowBackground.color)
        // The grid itself holds keyboard focus while no cell editor is open,
        // so selection responds to arrows, Return, Delete, and ⌘C/⌘X/⌘V.
        .focusable()
        .focusEffectDisabled()
        .focused($gridFocused)
        // Arrows move the anchor; shift-arrows stretch the rectangle.
        .onKeyPress(keys: [.upArrow, .downArrow, .leftArrow, .rightArrow]) { press in
            let (rowDelta, columnDelta): (Int, Int) = switch press.key {
            case .upArrow: (-1, 0)
            case .downArrow: (1, 0)
            case .leftArrow: (0, -1)
            default: (0, 1)
            }
            if press.modifiers.contains(.shift) {
                sheet.extendSelection(rowDelta: rowDelta, columnDelta: columnDelta)
            } else {
                sheet.moveSelection(rowDelta: rowDelta, columnDelta: columnDelta)
            }
            return .handled
        }
        .onKeyPress(.return) {
            guard let selected = sheet.selected, sheet.editing == nil else { return .ignored }
            sheet.beginEditing(selected)
            return .handled
        }
        .onKeyPress(.escape) {
            guard sheet.editing == nil else { return .ignored }
            sheet.deselect()
            return .handled
        }
        .onDeleteCommand {
            guard sheet.editing == nil else { return }
            sheet.clearSelection()
        }
        // Copy/cut/paste write and read the pasteboard through the model
        // (returning [] so SwiftUI doesn't write a second time): a COPY
        // carries the custom origin type that lets paste adjust relative
        // references; a CUT is plain TSV (cut-paste keeps refs verbatim).
        .onCopyCommand {
            guard sheet.selectionTSV() != nil else { return [] }
            sheet.copySelectionToPasteboard()
            return []
        }
        .onCutCommand {
            guard sheet.selectionTSV() != nil else { return [] }
            sheet.cutSelectionToPasteboard()
            return []
        }
        .onPasteCommand(of: [.plainText, .utf8PlainText]) { _ in
            guard sheet.selected != nil, sheet.editing == nil else { return }
            sheet.pasteFromPasteboard()
        }
        .onAppear {
            gridFocused = true
            sheet.gridFontSize = themeManager.current.fontSize
        }
        // Keep the grid's default cell geometry proportional to the app font.
        .onChange(of: themeManager.current.fontSize) {
            sheet.gridFontSize = themeManager.current.fontSize
        }
        // When a cell editor closes, hand keyboard focus back to the grid.
        .onChange(of: sheet.editing) {
            if sheet.editing == nil {
                gridFocused = true
            }
        }
    }

    private var headerRow: some View {
        HStack(spacing: 0) {
            Text("") // gutter corner
                .frame(width: gutterWidth, height: sheet.defaultRowHeightScaled)
            ForEach(0..<sheet.visibleColumnCount, id: \.self) { column in
                Text(CellAddress(column: column, row: 0).columnName)
                    .font(theme.font(scale: 0.85))
                    .foregroundStyle(theme.secondaryText.color)
                    .frame(width: sheet.width(ofColumn: column), height: sheet.defaultRowHeightScaled)
                    .contentShape(Rectangle())
                    .contextMenu {
                        if !sheet.activeSheetIsData {
                            ColumnHeaderMenu(column: column)
                        }
                    }
                    .overlay(alignment: .trailing) {
                        ColumnResizeHandle(column: column)
                    }
            }
        }
    }

    private func gridRow(_ row: Int) -> some View {
        GridRowView(row: row, gutterWidth: gutterWidth)
    }

    // MARK: Resize guide lines (the only views that update during a drag)

    @ViewBuilder
    private var columnResizeGuide: some View {
        if let preview = sheet.columnResizePreview {
            let leftEdge = (0..<preview.index).reduce(gutterWidth) { $0 + sheet.width(ofColumn: $1) }
            Rectangle()
                .fill(theme.accent.color)
                .frame(width: 1.5)
                .frame(maxHeight: .infinity)
                .offset(x: leftEdge + preview.size)
        }
    }

    @ViewBuilder
    private var rowResizeGuide: some View {
        if let preview = sheet.rowResizePreview {
            let topEdge = (0..<preview.index).reduce(CGFloat(0)) { $0 + sheet.height(ofRow: $1) }
            let gridWidth = (0..<Spreadsheet.columnCount).reduce(gutterWidth) { $0 + sheet.width(ofColumn: $1) }
            Rectangle()
                .fill(theme.accent.color)
                .frame(width: gridWidth, height: 1.5)
                .offset(y: topEdge + preview.size)
        }
    }
}

/// Right-click menu on a row number: insert/delete, pluralized over the
/// selection when the clicked row is inside it (the retarget rule).
private struct RowHeaderMenu: View {
    let row: Int

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        let sheet = session.sheet
        // Inside the selected span → act on the span; outside → this row.
        let span: (start: Int, count: Int) = {
            if let selected = sheet.selectedRowSpan,
               (selected.start..<selected.start + selected.count).contains(row) {
                return selected
            }
            return (row, 1)
        }()
        let noun = span.count > 1 ? "\(span.count) Rows" : "Row"

        Button("Insert \(noun) Above") {
            structuralAlert(sheet.insertRows(at: span.start, count: span.count))
        }
        Button("Insert \(noun) Below") {
            structuralAlert(sheet.insertRows(at: span.start + span.count, count: span.count))
        }
        Divider()
        Button("Delete \(noun)") {
            structuralAlert(sheet.deleteRows(at: span.start, count: span.count))
        }
    }
}

/// Same for column letters.
private struct ColumnHeaderMenu: View {
    let column: Int

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        let sheet = session.sheet
        let span: (start: Int, count: Int) = {
            if let selected = sheet.selectedColumnSpan,
               (selected.start..<selected.start + selected.count).contains(column) {
                return selected
            }
            return (column, 1)
        }()
        let noun = span.count > 1 ? "\(span.count) Columns" : "Column"

        Button("Insert \(noun) Before") {
            structuralAlert(sheet.insertColumns(at: span.start, count: span.count))
        }
        Button("Insert \(noun) After") {
            structuralAlert(sheet.insertColumns(at: span.start + span.count, count: span.count))
        }
        Divider()
        Button("Delete \(noun)") {
            structuralAlert(sheet.deleteColumns(at: span.start, count: span.count))
        }
    }
}

/// Insert can refuse (content would fall off the grid) — say why.
@MainActor
private func structuralAlert(_ message: String?) {
    guard let message else { return }
    let alert = NSAlert()
    alert.messageText = "Can't Change the Grid"
    alert.informativeText = message
    alert.runModal()
}

/// Drag the right edge of a column header to resize; double-click to reset.
private struct ColumnResizeHandle: View {
    let column: Int

    @Environment(CalculatorSession.self) private var session
    @State private var dragStartWidth: CGFloat?

    var body: some View {
        Rectangle()
            .fill(.clear)
            .frame(width: 8)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .onHover { hovering in
                if hovering {
                    NSCursor.resizeLeftRight.push()
                } else {
                    NSCursor.pop()
                }
            }
            .gesture(DragGesture(minimumDistance: 1)
                .onChanged { value in
                    let base = dragStartWidth ?? session.sheet.width(ofColumn: column)
                    dragStartWidth = base
                    // Preview only — the width applies on release, so rows
                    // don't re-render on every drag tick.
                    session.sheet.previewColumnResize(base + value.translation.width,
                                                      forColumn: column)
                }
                .onEnded { _ in
                    dragStartWidth = nil
                    session.sheet.endColumnResize()
                })
            // Per the gesture-latency invariant: simultaneous, never stacked.
            .simultaneousGesture(TapGesture(count: 2).onEnded {
                session.sheet.resetColumnWidth(forColumn: column)
            })
    }
}

/// Drag the bottom edge of a row number to resize; double-click to reset.
private struct RowResizeHandle: View {
    let row: Int

    @Environment(CalculatorSession.self) private var session
    @State private var dragStartHeight: CGFloat?

    var body: some View {
        Rectangle()
            .fill(.clear)
            .frame(height: 6)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .onHover { hovering in
                if hovering {
                    NSCursor.resizeUpDown.push()
                } else {
                    NSCursor.pop()
                }
            }
            .gesture(DragGesture(minimumDistance: 1)
                .onChanged { value in
                    let base = dragStartHeight ?? session.sheet.height(ofRow: row)
                    dragStartHeight = base
                    session.sheet.previewRowResize(base + value.translation.height, forRow: row)
                }
                .onEnded { _ in
                    dragStartHeight = nil
                    session.sheet.endRowResize()
                })
            .simultaneousGesture(TapGesture(count: 2).onEnded {
                session.sheet.resetRowHeight(forRow: row)
            })
    }
}

/// One grid row. Reads the selection ONCE and passes plain Bools down so a
/// selection change re-evaluates ~40 visible row bodies and exactly the two
/// affected cells — not every visible cell. (Cells reading `sheet.selected`
/// directly made every click invalidate ~1,000 CellView bodies.)
private struct GridRowView: View {
    let row: Int
    let gutterWidth: CGFloat

    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    var body: some View {
        let sheet = session.sheet
        let theme = themeManager.current
        // This row's slice of the selection rectangle, as plain values.
        let rect = sheet.selectionRect
        let selectedColumns: ClosedRange<Int>? =
            (rect?.rows.contains(row) == true) ? rect?.columns : nil
        let anchorColumn = sheet.selected?.row == row ? sheet.selected?.column : nil
        let editingColumn = sheet.editing?.row == row ? sheet.editing?.column : nil
        let rowHeight = sheet.height(ofRow: row)

        HStack(spacing: 0) {
            Text("\(row + 1)")
                .font(theme.font(scale: 0.85))
                .foregroundStyle(theme.secondaryText.color)
                .frame(width: gutterWidth, height: rowHeight)
                .contentShape(Rectangle())
                .contextMenu {
                    if !sheet.activeSheetIsData {
                        RowHeaderMenu(row: row)
                    }
                }
                .overlay(alignment: .bottom) {
                    RowResizeHandle(row: row)
                }
            ForEach(0..<session.sheet.visibleColumnCount, id: \.self) { column in
                let address = CellAddress(column: column, row: row)
                CellView(address: address,
                         display: sheet.display(at: address),
                         raw: sheet.raw(at: address),
                         format: sheet.format(at: address),
                         isSelected: selectedColumns?.contains(column) == true,
                         isAnchor: column == anchorColumn,
                         isEditing: column == editingColumn,
                         theme: theme,
                         width: sheet.width(ofColumn: column), height: rowHeight)
            }
        }
    }
}
