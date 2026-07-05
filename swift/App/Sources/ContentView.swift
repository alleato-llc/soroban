import SwiftUI
import BinaryEditorKit

struct ContentView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    #if os(iOS)
    // On iPad the menu bar doesn't exist, so New/Open/Save/etc. and the
    // Settings/Reference/About windows are reached from a navigation toolbar
    // and presented as sheets.
    @State private var showSettings = false
    @State private var showReference = false
    @State private var showAbout = false
    #endif

    var body: some View {
        #if os(iOS)
        NavigationStack {
            content
                .navigationTitle(session.workbook.title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { iosToolbar }
        }
        .workbookFileDialogs(session.workbook)
        .sheet(isPresented: $showSettings) {
            NavigationStack {
                SettingsView()
                    .environment(session)
                    .environment(themeManager)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { showSettings = false }
                        }
                    }
            }
        }
        .sheet(isPresented: $showReference) {
            NavigationStack {
                ReferenceView()
                    .environment(session)
                    .environment(themeManager)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { showReference = false }
                        }
                    }
            }
        }
        .sheet(isPresented: $showAbout) {
            AboutView().environment(themeManager)
        }
        #else
        content
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        session.inspectorVisible.toggle()
                    } label: {
                        Image(systemName: "sidebar.trailing")
                    }
                    .help("Toggle the Environment inspector (⌥⌘0)")
                }
            }
            .workbookFileDialogs(session.workbook)
        #endif
    }

    private var content: some View {
        HStack(spacing: 0) {
            main
            if session.inspectorVisible {
                Divider()
                InspectorView()
            }
        }
        .background(themeManager.current.windowBackground.color)
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 320)
        #endif
    }

    private var main: some View {
        VStack(spacing: 0) {
            switch session.activeView {
            case .log:
                HistoryLogView()
                // Binary bit-editor — Programmer mode only, toggle with ⌥⌘B.
                if session.mode == .programmer && session.binaryEditorShown {
                    Divider()
                    // The editor is generic over a host; the calculator supplies
                    // a thin CalculatorSession-backed adapter (TamaKit seam).
                    BinaryEditorView(host: CalculatorBinaryHost(session: session, themeManager: themeManager))
                }
                if !session.suggestions.isEmpty {
                    Divider()
                    SuggestionsView()
                }
                Divider()
                InputBarView()

            case .sheet:
                // No input bar in grid mode — its results would land in the
                // hidden log. The worksheet strip owns the bottom edge (and
                // hosts the view toggle).
                SpreadsheetView()
                Divider()
                SheetTabBar()
            }
        }
        .frame(maxWidth: .infinity)
    }

    #if os(iOS)
    /// The iPad command surface: File / Edit / Format / Sheet menus + the
    /// log↔grid toggle + the inspector toggle + a "more" menu (Settings /
    /// Reference / About). Mirrors the macOS menu bar's actions.
    @ToolbarContentBuilder private var iosToolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Menu {
                Button("New Workbook") { session.workbook.newWorkbook() }
                Button("Open…") { session.workbook.open() }
                Button("Open CSV…") { session.workbook.openCSV() }
                Divider()
                Button("Save") { session.workbook.save() }
                Button("Save As…") { session.workbook.saveAs() }
                Button("Export CSV…") { session.workbook.exportCSV() }
            } label: {
                Label("File", systemImage: "doc")
            }
        }
        ToolbarItem(placement: .topBarLeading) {
            Menu {
                Button("Undo") { session.sheet.undo() }.disabled(!session.sheet.canUndo)
                Button("Redo") { session.sheet.redo() }.disabled(!session.sheet.canRedo)
                Divider()
                Button("Copy") { session.sheet.copySelectionToPasteboard() }
                Button("Cut") { session.sheet.cutSelectionToPasteboard() }
                Button("Paste") { session.sheet.pasteFromPasteboard() }
                Divider()
                Button("Fill Down") { session.sheet.fillDown() }
                Button("Fill Right") { session.sheet.fillRight() }
                Divider()
                Button("Clear Log") { session.clearLog() }
            } label: {
                Label("Edit", systemImage: "pencil")
            }
        }
        ToolbarItem(placement: .topBarLeading) {
            Menu {
                FormatActions(sheet: session.sheet)
            } label: {
                Label("Format", systemImage: "paintpalette")
            }
            .disabled(session.activeView != .sheet || session.sheet.selected == nil)
        }
        ToolbarItem(placement: .topBarLeading) {
            Menu {
                Button("Add Sheet") {
                    session.activeView = .sheet
                    if session.sheet.addSheet() == nil {
                        session.sheet.renameRequested = true
                    }
                }
                .disabled(!session.sheet.canAddSheet)
                Button("Rename Sheet…") {
                    session.activeView = .sheet
                    session.sheet.renameRequested = true
                }
                Button("Delete Sheet…") {
                    session.activeView = .sheet
                    session.sheet.removeRequested = true
                }
                .disabled(!session.sheet.canRemoveSheet)
                Divider()
                Button("Import Data (CSV)…") {
                    session.activeView = .sheet
                    session.workbook.importData()
                }
            } label: {
                Label("Sheet", systemImage: "square.on.square")
            }
        }

        ToolbarItem(placement: .topBarTrailing) {
            Button {
                session.toggleView()
            } label: {
                Label(session.activeView == .log ? "Grid" : "Log",
                      systemImage: session.activeView == .log ? "tablecells" : "list.bullet")
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                session.inspectorVisible.toggle()
            } label: {
                Image(systemName: "sidebar.trailing")
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            Menu {
                Button("Settings…") { showSettings = true }
                Button("Function Reference") { showReference = true }
                Button("About Soroban・算盤") { showAbout = true }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
        }
    }
    #endif
}
