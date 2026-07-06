import SorobanEngine
import Observation
import Foundation

/// UI-facing wrapper around the engine `Spreadsheet`: selection state,
/// editing, grid layout (column widths / row heights), and persistence.
///
/// One type, many files: @Observable requires every STORED property to live
/// in this class body, so they're all declared here (grouped by owner) while
/// the behavior is split into SheetModel+*.swift extensions per concern
/// (formatting, layout, worksheets, workbook, point mode, clipboard, names,
/// controls, CSV, data sheets, persistence). Members shared across those
/// files are `internal` by necessity — treat anything marked as extension
/// state as private to its section.
@Observable
@MainActor
final class SheetModel {
    /// All worksheets; resolver wiring (incl. owning-sheet routing) lives in
    /// the engine's SheetStore. (Internal: the extension files share them.)
    let store: SheetStore
    let calculator: Calculator

    /// The grid the UI is showing/editing.
    private var spreadsheet: Spreadsheet { store.activeSheet.grid }

    /// Single click — the selection ANCHOR (editor target, paste origin).
    var selected: CellAddress?
    /// Shift-click / shift-arrows — the far corner of a rectangular
    /// selection; nil means a single-cell selection.
    var selectionExtent: CellAddress?
    /// Double click / Return — the cell whose editor is open.
    var editing: CellAddress?

    /// The normalized selected rectangle.
    var selectionRect: (rows: ClosedRange<Int>, columns: ClosedRange<Int>)? {
        guard let anchor = selected else { return nil }
        let extent = selectionExtent ?? anchor
        return (min(anchor.row, extent.row)...max(anchor.row, extent.row),
                min(anchor.column, extent.column)...max(anchor.column, extent.column))
    }
    /// Bumped whenever displayed values may have changed; cells observe this
    /// instead of the (non-Observable) engine model. Views read it; only
    /// SheetModel extensions write it.
    var generation = 0

    /// Workbook hooks (set by WorkbookManager): while untitled, changes keep
    /// autosaving to the Application Support scratch file; once a workbook
    /// file is open, changes only notify so the manager can mark it dirty.
    var autosaveToScratch = true
    var onContentChange: (() -> Void)?

    nonisolated static var storeURL: URL? {
        guard let support = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first else { return nil }
        let directory = support.appendingPathComponent("Soroban", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appendingPathComponent("sheet.json")
    }

    /// The SQLite store backing DATA sheets, at the working path. Lazy —
    /// created on first import; opened at init when the file exists.
    /// (Internal: the data-sheet and worksheet extensions share it.)
    var dataStore: DataStore?

    nonisolated static var workingDataURL: URL? {
        storeURL?.deletingLastPathComponent().appendingPathComponent("working-data.sqlite")
    }

    // MARK: Stored state owned by extension files (see the type comment)

    /// Point mode (SheetModel+PointMode.swift): the open editor's text —
    /// lives here (not in the editor view) so clicking other cells can
    /// splice references into it — plus the insert/replace bookkeeping.
    var editingDraft = ""
    var editorRefocusTrigger = 0
    var pointModeExpectedDraft: String?
    var lastInsertedReference: String?
    var lastInsertedAddress: CellAddress?
    var pendingFocusCommit = false

    /// Layout (SheetModel+Layout.swift): an in-flight resize drag, shown as
    /// a guide line; the actual size applies on release.
    struct ResizePreview: Equatable {
        let index: Int
        let size: CGFloat
    }
    var columnResizePreview: ResizePreview?
    var rowResizePreview: ResizePreview?

    /// The current app font size (the app keeps this synced to ThemeManager).
    /// The grid's DEFAULT column width / row height scale with it so cells stay
    /// proportional to the font — see SheetModel+Layout.
    var gridFontSize: CGFloat = 14

    /// Worksheets (SheetModel+Worksheets.swift): set by the menu-bar Sheet
    /// menu; SheetTabBar consumes them (the rename field and the delete
    /// confirmation live there).
    var renameRequested = false
    var removeRequested = false

    /// Persistence (SheetModel+Persistence.swift): the ordered file-work
    /// chain and the journal-compaction counter.
    var persistChain: Task<Void, Never>?
    var journalEntriesSinceSnapshot = 0

    init(calculator: Calculator) {
        self.calculator = calculator
        self.store = SheetStore(calculator: calculator) // wires resolvers
        if let url = Self.workingDataURL, FileManager.default.fileExists(atPath: url.path) {
            dataStore = try? DataStore(url: url)
        }
        loadScratch()
        installMutationOverride() // make log mutations undoable + persisted
    }

    var activeSheetIsData: Bool {
        _ = generation
        return store.activeSheet.isData
    }

    var hasDataSheets: Bool { store.sheets.contains { $0.isData } }

    /// Where the working database lives, for package saves (nil when the
    /// workbook has no data sheets — data.sqlite is then omitted).
    var workingDatabaseURL: URL? {
        hasDataSheets ? Self.workingDataURL : nil
    }

    /// Rendering bounds: data sheets browse up to 10,000 rows in the grid
    /// (formulas can still reference every row).
    var visibleRowCount: Int {
        _ = generation
        if let data = store.activeSheet.data {
            return min(max(data.rowCount, 1), 10_000)
        }
        return Spreadsheet.rowCount
    }

    var visibleColumnCount: Int {
        _ = generation
        if let data = store.activeSheet.data {
            return max(data.columnCount, 1)
        }
        return Spreadsheet.columnCount
    }

    // MARK: Cell access

    func display(at address: CellAddress) -> CellDisplay {
        _ = generation // register observation
        if let data = store.activeSheet.data {
            let raw = data.rawValue(row: address.row, column: address.column)
            if raw.isEmpty { return .empty }
            if let value = BigDecimal(string: raw) { return .value(value) }
            return .text(raw)
        }
        return spreadsheet.displayValue(at: address)
    }

    func raw(at address: CellAddress) -> String {
        if let data = store.activeSheet.data {
            return data.rawValue(row: address.row, column: address.column)
        }
        return spreadsheet.raw(at: address)
    }

    func commit(_ raw: String, at address: CellAddress) {
        applyEdit([(address, raw)])
    }

    /// Applies cell changes as ONE undoable step (paste/clear regions group).
    func applyEdit(_ changes: [(address: CellAddress, raw: String)]) {
        let sheet = store.activeSheet
        var cellChanges: [CellChange] = []
        cellChanges.reserveCapacity(changes.count)
        for (address, raw) in changes {
            cellChanges.append(CellChange(address: address,
                                          old: self.raw(at: address), new: raw))
            write(raw, at: address, on: sheet)
        }
        // Skip no-ops so ⌘Z always does something visible.
        guard cellChanges.contains(where: { $0.old != $0.new }) else { return }
        pushUndo(SheetEdit(sheetName: store.activeSheet.name, kind: .cells(cellChanges)))
        generation += 1
        persistCellEdits(cellChanges.map { ($0.address, $0.new) },
                         sheetName: store.activeSheet.name)
    }

    // MARK: Undo/redo (grid content only — the log is history, not document)

    struct CellChange {
        let address: CellAddress
        let old: String
        let new: String
    }

    struct FormatChange {
        let address: CellAddress
        let old: CellFormat
        let new: CellFormat
    }

    /// A cell name added (old nil), renamed, or removed (new nil).
    struct NameChange {
        let address: CellAddress
        let old: String?
        let new: String?
    }

    /// One undoable step, tagged with its worksheet — undo jumps there first.
    struct SheetEdit {
        var sheetName: String
        let kind: Kind

        enum Kind {
            case cells([CellChange])
            case formats([FormatChange])
            case name(NameChange)
            /// A worksheet rename (which also rewrote referencing formulas —
            /// those ride as separate .cells steps, pushed AFTER this one).
            case renameSheet(old: String, new: String)
            /// An insert/delete of rows or columns. Undo is the engine's
            /// exact inverse (`store.revert`); redo re-executes the op (the
            /// state at redo time matches op time, so it's deterministic).
            case structure(SheetStore.StructuralChange)
        }

        func renamed(from oldName: String, to newName: String) -> SheetEdit {
            guard sheetName.compare(oldName, options: .caseInsensitive) == .orderedSame else {
                return self
            }
            return SheetEdit(sheetName: newName, kind: kind)
        }
    }

    private static let undoLimit = 100
    private(set) var undoStack: [SheetEdit] = []
    private(set) var redoStack: [SheetEdit] = []

    var canUndo: Bool { !undoStack.isEmpty }
    var canRedo: Bool { !redoStack.isEmpty }

    /// (Internal: formatting/names/worksheets extensions push their own
    /// undo steps.)
    func pushUndo(_ edit: SheetEdit) {
        undoStack.append(edit)
        if undoStack.count > Self.undoLimit {
            undoStack.removeFirst(undoStack.count - Self.undoLimit)
        }
        redoStack.removeAll()
    }

    /// Workbook open/new — a different document; old edits don't apply.
    func clearUndoHistory() {
        undoStack.removeAll()
        redoStack.removeAll()
    }

    /// A worksheet was removed — its edits can no longer be undone.
    func dropUndoHistory(forSheet name: String) {
        undoStack.removeAll {
            $0.sheetName.compare(name, options: .caseInsensitive) == .orderedSame
        }
        redoStack.removeAll {
            $0.sheetName.compare(name, options: .caseInsensitive) == .orderedSame
        }
    }

    /// A worksheet was renamed — keep its history working under the new tag.
    func retagUndoHistory(from oldName: String, to newName: String) {
        undoStack = undoStack.map { $0.renamed(from: oldName, to: newName) }
        redoStack = redoStack.map { $0.renamed(from: oldName, to: newName) }
    }

    func undo() {
        guard let edit = undoStack.popLast() else { return }
        // A rename edit finds its sheet by the name IT knows — the tag can't
        // be trusted across the rename itself.
        var target = edit.sheetName
        if case .renameSheet(_, let new) = edit.kind { target = new }
        guard let sheet = jump(to: target) else { return }
        switch edit.kind {
        case .cells(let changes):
            for change in changes {
                write(change.old, at: change.address, on: sheet)
            }
            generation += 1
            persistCellEdits(changes.map { ($0.address, $0.old) }, sheetName: edit.sheetName)
        case .formats(let changes):
            for change in changes {
                sheet.formats[change.address] = change.old.isDefault ? nil : change.old
            }
            generation += 1
            persistAfterChange()
        case .name(let change):
            // try?: the old name was valid when recorded; if later edits
            // claimed it, skip rather than crash the undo.
            try? sheet.grid.setCellName(change.old, at: change.address)
            generation += 1
            persistAfterChange()
        case .renameSheet(let old, let new):
            if let index = store.sheets.firstIndex(where: { $0 === sheet }) {
                try? store.rename(at: index, to: old) // try?: see .name
                retagUndoHistory(from: new, to: old)
            }
            generation += 1
            persistAfterChange()
        case .structure(let change):
            store.revert(change)
            deselect()
            generation += 1
            persistAfterChange() // snapshot path — the journal can't shift
        }
        redoStack.append(edit)
    }

    func redo() {
        guard let edit = redoStack.popLast() else { return }
        var target = edit.sheetName
        if case .renameSheet(let old, _) = edit.kind { target = old }
        guard let sheet = jump(to: target) else { return }
        switch edit.kind {
        case .cells(let changes):
            for change in changes {
                write(change.new, at: change.address, on: sheet)
            }
            generation += 1
            persistCellEdits(changes.map { ($0.address, $0.new) }, sheetName: edit.sheetName)
        case .formats(let changes):
            for change in changes {
                sheet.formats[change.address] = change.new.isDefault ? nil : change.new
            }
            generation += 1
            persistAfterChange()
        case .name(let change):
            try? sheet.grid.setCellName(change.new, at: change.address)
            generation += 1
            persistAfterChange()
        case .renameSheet(let old, let new):
            if let index = store.sheets.firstIndex(where: { $0 === sheet }) {
                try? store.rename(at: index, to: new)
                retagUndoHistory(from: old, to: new)
            }
            generation += 1
            persistAfterChange()
        case .structure(let change):
            // Deterministic re-execution: post-undo state == pre-op state.
            _ = change.isInsert
                ? try? store.insertSlots(axis: change.axis, at: change.index,
                                         count: change.count, in: sheet)
                : try? store.deleteSlots(axis: change.axis, at: change.index,
                                         count: change.count, in: sheet)
            deselect()
            generation += 1
            persistAfterChange()
        }
        undoStack.append(edit)
    }

    /// Routes a raw edit to the sheet's backing store: SQLite for data sheets
    /// (the workbook's own copy — never the imported source), grid otherwise.
    private func write(_ raw: String, at address: CellAddress, on sheet: Sheet) {
        if let data = sheet.data {
            try? data.setRawValue(raw, row: address.row, column: address.column)
        } else {
            sheet.grid.setCell(raw, at: address)
        }
    }

    /// Activates the edit's sheet (Excel-style: undo shows you what changed).
    /// nil when the sheet was deleted — the entry is silently dropped.
    private func jump(to sheetName: String) -> Sheet? {
        guard let index = store.sheets.firstIndex(where: {
            $0.name.compare(sheetName, options: .caseInsensitive) == .orderedSame
        }) else { return nil }
        if index != store.activeIndex {
            endEditing()
            deselect()
            store.activeIndex = index
        }
        return store.sheets[index]
    }

    /// Values may depend on log variables; call after every log submission.
    func recalculate() {
        store.recalculate()
        generation += 1
    }

    // MARK: Selection & editing state

    func select(_ address: CellAddress) {
        selected = address
        selectionExtent = nil
        // Re-selecting the editing cell must not close its editor: the
        // double-click recognizer runs simultaneously with single-tap, and
        // the second click delivers BOTH callbacks in unspecified order.
        if editing != address {
            editing = nil
        }
    }

    func deselect() {
        selected = nil
        selectionExtent = nil
    }

    /// Shift-click: stretch the rectangle from the anchor to `address`.
    func extendSelection(to address: CellAddress) {
        if selected == nil {
            selected = address
        }
        selectionExtent = address
        editing = nil
    }

    /// Shift-arrows: move the extent corner.
    func extendSelection(rowDelta: Int, columnDelta: Int) {
        guard let anchor = selected else {
            selected = CellAddress(column: 0, row: 0)
            return
        }
        let current = selectionExtent ?? anchor
        selectionExtent = clamped(row: current.row + rowDelta,
                                  column: current.column + columnDelta)
        editing = nil
    }

    func beginEditing(_ address: CellAddress) {
        selected = address
        selectionExtent = nil
        editing = address
        editingDraft = raw(at: address)
        pointModeExpectedDraft = nil
        pendingFocusCommit = false
    }

    func endEditing() {
        editing = nil
        pointModeExpectedDraft = nil
        pendingFocusCommit = false
    }

    /// Arrow keys / Return ↓ / Tab →. Collapses any rectangle, ends editing.
    func moveSelection(rowDelta: Int, columnDelta: Int) {
        editing = nil
        selectionExtent = nil
        guard let current = selected else {
            selected = CellAddress(column: 0, row: 0)
            return
        }
        selected = clamped(row: current.row + rowDelta,
                           column: current.column + columnDelta)
    }

    /// Clamps to the VISIBLE bounds — on a data sheet the selection stops at
    /// the table's edge, not the grid's 1000×26.
    private func clamped(row: Int, column: Int) -> CellAddress {
        CellAddress(column: min(max(column, 0), visibleColumnCount - 1),
                    row: min(max(row, 0), visibleRowCount - 1))
    }
}

extension CGFloat {
    func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}
