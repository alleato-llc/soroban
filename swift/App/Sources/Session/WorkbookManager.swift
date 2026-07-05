import SwiftUI
import SorobanEngine
import Observation
import UniformTypeIdentifiers

/// Document controller for `.soroban` workbooks (cells + user variables).
///
/// Untitled work autosaves to the Application Support scratch file exactly as
/// before; once a file is opened or saved, that file is the ⌘S target and
/// edits flag the title with "— Edited" instead.
///
/// File dialogs are unified across macOS and iPadOS on SwiftUI: the manager
/// publishes *intent* (`importRequest`/`exportRequest`/`alert`) and the view
/// layer (`ContentView.workbookFileDialogs`) presents `.fileImporter`/
/// `.fileExporter`/`.alert`, calling back with the chosen URL. Reads/writes of a
/// user-chosen URL run inside a security-scoped-resource access (required on
/// iOS, harmless on macOS).
@Observable
@MainActor
final class WorkbookManager {
    private(set) var fileURL: URL?
    private(set) var isDirty = false

    /// Intent the view presents. Set by the File-menu/toolbar actions; cleared
    /// by the dialog's completion.
    var importRequest: ImportRequest?
    var exportRequest: ExportRequest?
    var alert: AlertMessage?

    private let sheet: SheetModel

    /// The exported package UTI (declared in Info.plist via project.yml).
    /// `nonisolated` so the `FileDocument`'s static type lists can reference it.
    nonisolated static let fileType = UTType(exportedAs: "com.alleato.soroban.workbook")

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

    // MARK: Intent — what a File action wants (presented by the view)

    /// An open/import: which flow, and the content types the picker allows.
    struct ImportRequest: Identifiable {
        enum Kind { case workbook, csvWorkbook }
        let id = UUID()
        let kind: Kind
        let contentTypes: [UTType]
    }

    /// A save/export: the document to write, its type, and a default filename.
    struct ExportRequest: Identifiable {
        let id = UUID()
        let document: ExportableDocument
        let contentType: UTType
        let defaultName: String
    }

    /// A message to show (open/save failures, an import note).
    struct AlertMessage: Identifiable {
        let id = UUID()
        let title: String
        let detail: String
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

    /// ⌘O — asks the view to present the open picker.
    func open() {
        importRequest = ImportRequest(kind: .workbook, contentTypes: [Self.fileType])
    }

    func open(url: URL) {
        withScopedAccess(url) {
            do {
                let workbook = try WorkbookPackage.read(from: url) // package or legacy flat
                sheet.autosaveToScratch = false // before apply: don't clobber scratch
                sheet.prepareWorkingDatabase(copyFrom: WorkbookPackage.databaseURL(in: url))
                sheet.apply(workbook)
                fileURL = url
                isDirty = false
            } catch {
                present("Couldn't open “\(url.lastPathComponent)”", error)
            }
        }
    }

    /// ⌘S — writes to the current file, or prompts when untitled.
    func save() {
        guard let fileURL else { return saveAs() }
        write(to: fileURL)
    }

    /// ⇧⌘S — build the package document and ask the view to present the exporter.
    func saveAs() {
        do {
            let document = try workbookDocument()
            exportRequest = ExportRequest(
                document: document, contentType: Self.fileType,
                defaultName: fileURL?.deletingPathExtension().lastPathComponent ?? "Workbook")
        } catch {
            present("Couldn't prepare the workbook", error)
        }
    }

    private func write(to url: URL) {
        withScopedAccess(url) {
            do {
                try WorkbookPackage.write(sheet.currentWorkbook(), to: url,
                                          databaseURL: sheet.workingDatabaseURL)
                fileURL = url
                isDirty = false
                sheet.autosaveToScratch = false
            } catch {
                present("Couldn't save “\(url.lastPathComponent)”", error)
            }
        }
    }

    /// The view calls this after `.fileExporter` writes to `url` — adopt it as
    /// the ⌘S target (Save As), or note the CSV export finished.
    func exportCompleted(to url: URL, isWorkbook: Bool) {
        guard isWorkbook else { return }
        fileURL = url
        isDirty = false
        sheet.autosaveToScratch = false
    }

    /// File ▸ Open CSV… — opens the CSV as a new, EDITABLE workbook. Files that
    /// fit the grid become ordinary cells; bigger ones become a SQLite-backed
    /// data sheet. Either way it's a COPY — the source `.csv` is never written
    /// back; edits are saved into the `.soroban` file. (The single CSV-in door;
    /// the old "Import Data" command is gone.)
    func openCSV() {
        importRequest = ImportRequest(kind: .csvWorkbook,
                                      contentTypes: [.commaSeparatedText, .plainText])
    }

    /// Files that fit the grid open as ordinary editable cells, Excel-style;
    /// bigger files fall back to a data sheet automatically.
    func openCSV(url: URL) {
        withScopedAccess(url) {
            guard let text = (try? String(contentsOf: url, encoding: .utf8))
                ?? (try? String(contentsOf: url, encoding: .isoLatin1)) else {
                present("Couldn't open “\(url.lastPathComponent)”",
                        EngineError.domainError(message: "the file isn't readable as text"))
                return
            }
            let rows = CSV.parse(text)
            guard !rows.isEmpty else {
                present("Couldn't open “\(url.lastPathComponent)”",
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
                    present("Couldn't open “\(url.lastPathComponent)”",
                            EngineError.domainError(message: message))
                }
            }
        }
    }

    /// File ▸ Export CSV… — the CURRENT sheet's computed values.
    func exportCSV() {
        let document = ExportableDocument(text: sheet.activeSheetCSV())
        exportRequest = ExportRequest(document: document, contentType: .commaSeparatedText,
                                      defaultName: sheet.activeSheetName)
    }

    /// Dispatches a completed `.fileImporter` selection to the right flow.
    func handleImport(_ request: ImportRequest, url: URL) {
        switch request.kind {
        case .workbook: open(url: url)
        case .csvWorkbook: openCSV(url: url)
        }
    }

    // MARK: Helpers

    /// Runs `body` while holding security-scoped access to `url` (required on
    /// iOS for user-chosen files; a no-op-ish call on macOS). Non-scoped URLs
    /// (our own scratch/temp) simply run `body`.
    private func withScopedAccess(_ url: URL, _ body: () -> Void) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        body()
    }

    /// Writes the current workbook package to a temp location and hands its URL
    /// to the exporter (which builds the `FileWrapper` at write time, preserving
    /// the nested `data.sqlite`). The temp package is left for the OS to reap.
    private func workbookDocument() throws -> ExportableDocument {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("SorobanExport-\(UUID().uuidString).soroban")
        try WorkbookPackage.write(sheet.currentWorkbook(), to: temp,
                                  databaseURL: sheet.workingDatabaseURL)
        return ExportableDocument(packageAt: temp)
    }

    private func present(_ message: String, _ error: Error) {
        alert = AlertMessage(title: message, detail: "\(error)")
    }
}
