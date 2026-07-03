import AppKit
import SorobanEngine
import Observation
import UniformTypeIdentifiers

/// Document controller for `.soroban` workbooks (cells + user variables).
///
/// Untitled work autosaves to the Application Support scratch file exactly as
/// before; once a file is opened or saved, that file is the ⌘S target and
/// edits flag the title with "— Edited" instead.
@Observable
@MainActor
final class WorkbookManager {
    private(set) var fileURL: URL?
    private(set) var isDirty = false

    private let sheet: SheetModel

    /// The exported package UTI (declared in Info.plist via project.yml).
    static let fileType = UTType(exportedAs: "com.alleato.soroban.workbook")

    init(sheet: SheetModel) {
        self.sheet = sheet
        sheet.onContentChange = { [weak self] in
            self?.noteContentChanged()
        }
    }

    /// Window title: filename (or "Untitled") plus a dirty marker for files.
    var title: String {
        guard let fileURL else { return "Soroban・算盤 — Untitled" }
        let name = fileURL.deletingPathExtension().lastPathComponent
        return isDirty ? "\(name) — Edited" : name
    }

    /// Called for any content change (cell commit or variable assignment).
    func noteContentChanged() {
        if fileURL != nil {
            isDirty = true
        }
    }

    // MARK: File operations

    /// ⌘N — back to a blank untitled scratch workbook.
    func newWorkbook() {
        fileURL = nil
        isDirty = false
        sheet.autosaveToScratch = true
        sheet.prepareWorkingDatabase(copyFrom: nil)
        sheet.apply(Workbook(cells: [:], variables: [:])) // also clears the scratch file
    }

    /// ⌘O
    func open() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [Self.fileType]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        open(url: url)
    }

    func open(url: URL) {
        do {
            let workbook = try WorkbookPackage.read(from: url) // package or legacy flat
            sheet.autosaveToScratch = false // before apply: don't clobber scratch
            sheet.prepareWorkingDatabase(copyFrom: WorkbookPackage.databaseURL(in: url))
            sheet.apply(workbook)
            fileURL = url
            isDirty = false
        } catch {
            presentError("Couldn't open “\(url.lastPathComponent)”", error)
        }
    }

    /// ⌘S — writes to the current file, or prompts when untitled.
    func save() {
        guard let fileURL else { return saveAs() }
        write(to: fileURL)
    }

    /// ⇧⌘S
    func saveAs() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [Self.fileType]
        panel.nameFieldStringValue = fileURL?.lastPathComponent ?? "Workbook.soroban"
        panel.isExtensionHidden = false // show ".soroban" in the name field
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        write(to: url)
    }

    private func write(to url: URL) {
        do {
            try WorkbookPackage.write(sheet.currentWorkbook(), to: url,
                                      databaseURL: sheet.workingDatabaseURL)
            fileURL = url
            isDirty = false
            sheet.autosaveToScratch = false
        } catch {
            presentError("Couldn't save “\(url.lastPathComponent)”", error)
        }
    }

    /// File ▸ Open CSV… — the CSV becomes a NEW workbook (vs Import Data,
    /// which adds a data sheet to the CURRENT one). Files that fit the grid
    /// open as ordinary editable cells, Excel-style; bigger files fall back
    /// to a data sheet automatically.
    func openCSV() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.commaSeparatedText, .plainText]
        panel.allowsMultipleSelection = false
        panel.message = "Choose a CSV file to open as a new workbook"
        guard panel.runModal() == .OK, let url = panel.url else { return }

        guard let text = (try? String(contentsOf: url, encoding: .utf8))
            ?? (try? String(contentsOf: url, encoding: .isoLatin1)) else {
            presentError("Couldn't open “\(url.lastPathComponent)”",
                         EngineError.domainError(message: "the file isn't readable as text"))
            return
        }
        let rows = CSV.parse(text)
        guard !rows.isEmpty else {
            presentError("Couldn't open “\(url.lastPathComponent)”",
                         EngineError.domainError(message: "the file has no rows"))
            return
        }

        // Fresh untitled workbook, like ⌘N.
        fileURL = nil
        isDirty = false
        sheet.autosaveToScratch = true
        sheet.prepareWorkingDatabase(copyFrom: nil)

        let fits = rows.count <= Spreadsheet.rowCount
            && (rows.map(\.count).max() ?? 0) <= Spreadsheet.columnCount
        if fits {
            var cells: [String: String] = [:]
            for (rowIndex, row) in rows.enumerated() {
                for (columnIndex, field) in row.enumerated() where !field.isEmpty {
                    cells["\(CellAddress(column: columnIndex, row: rowIndex))"] = field
                }
            }
            let name = url.deletingPathExtension().lastPathComponent
            sheet.apply(Workbook(
                sheets: [Workbook.SheetPayload(name: name, cells: cells)],
                variables: [:])) // invalid names fall back to "Sheet 1" in apply
        } else {
            // Beyond the grid — a data sheet keeps every row.
            sheet.apply(Workbook(cells: [:], variables: [:]))
            if let message = sheet.importCSV(from: url) {
                presentError("Couldn't open “\(url.lastPathComponent)”",
                             EngineError.domainError(message: message))
            }
        }
    }

    /// File ▸ Export CSV… — the CURRENT sheet's computed values.
    func exportCSV() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.commaSeparatedText]
        panel.nameFieldStringValue = "\(sheet.activeSheetName).csv"
        panel.isExtensionHidden = false
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try sheet.activeSheetCSV().write(to: url, atomically: true, encoding: .utf8)
        } catch {
            presentError("Couldn't export “\(url.lastPathComponent)”", error)
        }
    }

    /// File ▸ Import Data (CSV)… — copies the file into a data sheet backed
    /// by the package's SQLite store. It's a COPY: edits land in the
    /// workbook's own database, never in the imported source file.
    func importData() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.commaSeparatedText, .plainText]
        panel.allowsMultipleSelection = false
        panel.message = "Choose a CSV file to import as a data sheet"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if let message = sheet.importCSV(from: url) {
            let alert = NSAlert()
            alert.messageText = "Import"
            alert.informativeText = message
            alert.runModal()
        }
        noteContentChanged()
    }

    private func presentError(_ message: String, _ error: Error) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = message
        alert.informativeText = "\(error)"
        alert.runModal()
    }
}
