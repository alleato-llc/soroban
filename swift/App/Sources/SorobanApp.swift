import SwiftUI

@main
struct SorobanApp: App {
    @State private var session = CalculatorSession()
    @State private var themeManager = ThemeManager()
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(session)
                .environment(themeManager)
                .navigationTitle(session.workbook.title)
                .onAppear {
                    appDelegate.workbook = session.workbook
                    appDelegate.sheet = session.sheet
                }
                // Finder double-click / drag onto Dock icon.
                .onOpenURL { url in
                    session.workbook.open(url: url)
                }
        }
        .commands {
            CommandGroup(replacing: .appInfo) {
                OpenAboutButton()
            }
            CommandGroup(replacing: .undoRedo) {
                Button("Undo") { session.sheet.undo() }
                    .keyboardShortcut("z", modifiers: .command)
                    .disabled(!session.sheet.canUndo)
                Button("Redo") { session.sheet.redo() }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .disabled(!session.sheet.canRedo)
            }
            CommandGroup(replacing: .newItem) {
                Button("New Workbook") { session.workbook.newWorkbook() }
                    .keyboardShortcut("n", modifiers: .command)
                Button("Open…") { session.workbook.open() }
                    .keyboardShortcut("o", modifiers: .command)
                Button("Open CSV…") { session.workbook.openCSV() }
                    .keyboardShortcut("o", modifiers: [.command, .shift])
            }
            CommandGroup(replacing: .saveItem) {
                Button("Save") { session.workbook.save() }
                    .keyboardShortcut("s", modifiers: .command)
                Button("Save As…") { session.workbook.saveAs() }
                    .keyboardShortcut("s", modifiers: [.command, .shift])
                Divider()
                Button("Export CSV…") { session.workbook.exportCSV() }
            }
            CommandGroup(after: .pasteboard) {
                Divider()
                // The formula-propagation gesture: top row / left column of
                // the selection is the source; relative refs adjust, $ pins
                // hold. A single-cell selection fills from its neighbor.
                Button("Fill Down") { session.sheet.fillDown() }
                    .keyboardShortcut("d", modifiers: .command)
                    .disabled(session.activeView != .sheet
                              || session.sheet.selected == nil
                              || session.sheet.editing != nil)
                Button("Fill Right") { session.sheet.fillRight() }
                    .keyboardShortcut("r", modifiers: .command)
                    .disabled(session.activeView != .sheet
                              || session.sheet.selected == nil
                              || session.sheet.editing != nil)
                Divider()
                Button("Clear Log") { session.clearLog() }
                    .keyboardShortcut("k", modifiers: .command)
            }
            CommandGroup(after: .sidebar) {
                Button(session.activeView == .log ? "Show Grid" : "Show Log") {
                    session.toggleView()
                }
                .keyboardShortcut("\\", modifiers: .command)
                Button(session.inspectorVisible ? "Hide Environment" : "Show Environment") {
                    session.inspectorVisible.toggle()
                }
                .keyboardShortcut("0", modifiers: [.command, .option])
                // Binary bit-editor (Programmer mode) — switch to Programmer
                // first; ⌥⌘B then hides/shows it.
                Button(session.binaryEditorShown ? "Hide Binary Editor" : "Show Binary Editor") {
                    session.binaryEditorShown.toggle()
                }
                .keyboardShortcut("b", modifiers: [.command, .option])
                .disabled(session.mode != .programmer)
                Divider()
                Button("Zoom In") {
                    themeManager.fontSizeOverride =
                        ThemeManager.clampedFontSize(themeManager.current.fontSize + 1)
                }
                // Bind "=" (not "+"): "+" is ⇧= on most keyboards, so a plain ⌘+
                // never fired. ⌘= is the standard zoom-in press (as in browsers);
                // the menu shows ⌘=, and it works without holding Shift.
                .keyboardShortcut("=", modifiers: .command)
                Button("Zoom Out") {
                    themeManager.fontSizeOverride =
                        ThemeManager.clampedFontSize(themeManager.current.fontSize - 1)
                }
                .keyboardShortcut("-", modifiers: .command)
                Button("Actual Size") {
                    themeManager.fontSizeOverride = nil // back to the theme default
                }
                .keyboardShortcut("0", modifiers: .command)
            }
            // Example expressions, grouped by language component — always
            // reachable (the empty-state welcome only shows on a fresh tape).
            // Choosing one fills the input bar.
            CommandMenu("Examples") {
                ForEach(Array(CalculatorSession.welcomeCategories.enumerated()), id: \.offset) { _, group in
                    Menu(group.name) {
                        ForEach(group.examples, id: \.self) { example in
                            Button(example) { session.useExample(example) }
                        }
                    }
                }
            }

            // Cell formatting — the same actions as the cells' right-click
            // menu (FormatActions is the single source); shortcuts register
            // here. Needs a selection in grid mode.
            CommandMenu("Format") {
                FormatActions(sheet: session.sheet)
                    .disabled(session.activeView != .sheet || session.sheet.selected == nil)
            }

            // Worksheet operations, discoverable from anywhere — they switch
            // to grid mode and route through the tab strip's UI.
            CommandMenu("Sheet") {
                Button("Add Sheet") {
                    session.activeView = .sheet
                    if session.sheet.addSheet() == nil {
                        session.sheet.renameRequested = true // name it right away
                    }
                }
                .keyboardShortcut("n", modifiers: [.command, .option])
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

                Divider()

                Button("Next Sheet") {
                    session.activeView = .sheet
                    session.sheet.activateSheet(
                        at: (session.sheet.activeSheetIndex + 1) % session.sheet.sheetCount)
                }
                .keyboardShortcut("]", modifiers: [.command, .option])
                Button("Previous Sheet") {
                    session.activeView = .sheet
                    session.sheet.activateSheet(
                        at: (session.sheet.activeSheetIndex + session.sheet.sheetCount - 1)
                            % session.sheet.sheetCount)
                }
                .keyboardShortcut("[", modifiers: [.command, .option])
            }
        }

        Settings {
            SettingsView()
                .environment(session)
                .environment(themeManager)
        }

        Window("Function Reference", id: "reference") {
            ReferenceView()
                .environment(session)
                .environment(themeManager)
        }
        .keyboardShortcut("/", modifiers: .command)
        .defaultSize(width: 720, height: 520)

        Window("About Soroban・算盤", id: "about") {
            AboutView()
                .environment(themeManager)
        }
        .windowResizability(.contentSize)
    }
}

/// The About menu item needs `openWindow`, which is only readable from a
/// View — hence this one-button wrapper inside the command group.
private struct OpenAboutButton: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("About Soroban・算盤") { openWindow(id: "about") }
    }
}

/// Quit guard: a *named* workbook with unsaved changes prompts Save/Discard/
/// Cancel. Untitled work needs no prompt — it autosaves to the scratch file.
final class AppDelegate: NSObject, NSApplicationDelegate {
    var workbook: WorkbookManager?
    var sheet: SheetModel?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        MainActor.assumeIsolated {
            // Autosave is debounced — don't lose the last ~0.75s of edits.
            sheet?.flushScratchNow()

            guard let workbook, workbook.isDirty, workbook.fileURL != nil else {
                return .terminateNow
            }

            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Save changes to “\(workbook.title)”?"
            alert.informativeText = "Your changes will be lost if you don't save them."
            alert.addButton(withTitle: "Save")
            alert.addButton(withTitle: "Cancel")
            alert.addButton(withTitle: "Discard")

            switch alert.runModal() {
            case .alertFirstButtonReturn:
                workbook.save()
                return workbook.isDirty ? .terminateCancel : .terminateNow
            case .alertThirdButtonReturn:
                return .terminateNow
            default:
                return .terminateCancel
            }
        }
    }
}
