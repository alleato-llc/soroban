import SwiftUI
import UniformTypeIdentifiers

/// A `FileDocument` that writes either a CSV text file or the `.soroban`
/// package (a directory built earlier at a temp URL, nested `data.sqlite`
/// included) to a user-chosen URL via `.fileExporter`, on both macOS and
/// iPadOS. The payload is `Sendable` (a String or a URL); the `FileWrapper` is
/// built at export time. Reads never go through here (the manager reads its own
/// URL via `.fileImporter`), so `init(configuration:)` is unused.
struct ExportableDocument: FileDocument {
    static var readableContentTypes: [UTType] {
        [WorkbookManager.fileType, .commaSeparatedText, .plainText]
    }
    static var writableContentTypes: [UTType] {
        [WorkbookManager.fileType, .commaSeparatedText]
    }

    private enum Payload: Sendable {
        case text(String)          // CSV export
        case package(URL)          // a .soroban package already written to temp
    }
    private let payload: Payload

    init(text: String) { self.payload = .text(text) }
    init(packageAt url: URL) { self.payload = .package(url) }

    init(configuration: ReadConfiguration) throws {
        throw CocoaError(.fileReadUnsupportedScheme)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        switch payload {
        case .text(let string):
            return FileWrapper(regularFileWithContents: Data(string.utf8))
        case .package(let url):
            return try FileWrapper(url: url)
        }
    }
}

extension View {
    /// Presents the workbook's open/save/alert dialogs, driven by the manager's
    /// intent state. Attach once, near the root (ContentView).
    func workbookFileDialogs(_ manager: WorkbookManager) -> some View {
        modifier(WorkbookFileDialogs(manager: manager))
    }
}

private struct WorkbookFileDialogs: ViewModifier {
    @Bindable var manager: WorkbookManager

    private var importing: Binding<Bool> {
        Binding(get: { manager.importRequest != nil },
                set: { if !$0 { manager.importRequest = nil } })
    }
    private var exporting: Binding<Bool> {
        Binding(get: { manager.exportRequest != nil },
                set: { if !$0 { manager.exportRequest = nil } })
    }
    private var alerting: Binding<Bool> {
        Binding(get: { manager.alert != nil },
                set: { if !$0 { manager.alert = nil } })
    }

    func body(content: Content) -> some View {
        content
            .fileImporter(
                isPresented: importing,
                allowedContentTypes: manager.importRequest?.contentTypes ?? [],
                allowsMultipleSelection: false
            ) { result in
                let request = manager.importRequest
                manager.importRequest = nil
                if let request, case .success(let urls) = result, let url = urls.first {
                    manager.handleImport(request, url: url)
                }
            }
            .fileExporter(
                isPresented: exporting,
                document: manager.exportRequest?.document,
                contentType: manager.exportRequest?.contentType ?? .data,
                defaultFilename: manager.exportRequest?.defaultName
            ) { result in
                let isWorkbook = manager.exportRequest?.contentType == WorkbookManager.fileType
                manager.exportRequest = nil
                if case .success(let url) = result {
                    manager.exportCompleted(to: url, isWorkbook: isWorkbook)
                }
            }
            .alert(
                manager.alert?.title ?? "",
                isPresented: alerting,
                presenting: manager.alert
            ) { _ in
                Button("OK", role: .cancel) { manager.alert = nil }
            } message: { message in
                Text(message.detail)
            }
    }
}
