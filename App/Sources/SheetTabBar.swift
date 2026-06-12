import SwiftUI
import SorobanEngine

/// The minimal worksheet strip (user design): only the ACTIVE tab is shown —
/// a menu on its name reaches every sheet, +/− beside it add and remove
/// (remove confirms), double-click renames inline. Long names truncate to
/// the window with the full name in a tooltip.
struct SheetTabBar: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    @State private var isRenaming = false
    @State private var renameDraft = ""
    @State private var renameError: String?
    @State private var confirmingRemoval = false

    private var theme: Theme { themeManager.current }
    private var sheet: SheetModel { session.sheet }

    var body: some View {
        HStack(spacing: 6) {
            if isRenaming {
                renameField
            } else {
                // The tab (menu + its own close ×), with + hugging its
                // trailing edge — controls live AT the tab, not mid-bar.
                tabGroup
            }

            Button {
                if let message = sheet.addSheet() {
                    renameError = message // surfaced in the same hint slot
                }
            } label: {
                Image(systemName: "plus")
                    .foregroundStyle(theme.secondaryText.color)
            }
            .buttonStyle(.plain)
            .disabled(!sheet.canAddSheet)
            .help(sheet.canAddSheet ? "Add a sheet" : "A workbook holds at most 256 sheets")

            if let renameError {
                Text(renameError)
                    .font(.system(size: theme.fontSize * 0.8))
                    .foregroundStyle(theme.errorText.color)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            ViewToggleButton(floating: false)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(theme.inputBackground.color)
        .confirmationDialog(
            "Delete “\(sheet.activeSheetName)”?",
            isPresented: $confirmingRemoval, titleVisibility: .visible
        ) {
            Button("Delete Sheet", role: .destructive) {
                renameError = sheet.removeActiveSheet()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Its cells are removed permanently. Formulas referencing it will show errors.")
        }
    }

    /// The single visible "tab": the sheet menu plus its own close ×, drawn
    /// as one bordered group so the × reads as part of the tab. The tab hugs
    /// its content (long names truncate at 280pt) instead of stretching.
    private var tabGroup: some View {
        HStack(spacing: 0) {
            sheetMenu
            if sheet.canRemoveSheet {
                Button {
                    confirmingRemoval = true
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: theme.fontSize * 0.55, weight: .semibold))
                        .foregroundStyle(theme.secondaryText.color)
                }
                .buttonStyle(.plain)
                .padding(.trailing, 7)
                .help("Remove this sheet…")
            }
        }
        .background(theme.windowBackground.color,
                    in: RoundedRectangle(cornerRadius: 5))
        .overlay {
            RoundedRectangle(cornerRadius: 5)
                .strokeBorder(theme.secondaryText.color.opacity(0.35), lineWidth: 1)
        }
        .help(sheet.activeSheetName) // the full 128 chars live here
        // The menu-bar Sheet menu routes its Rename/Delete here.
        .onChange(of: sheet.renameRequested) {
            if sheet.renameRequested {
                sheet.renameRequested = false
                beginRename()
            }
        }
        .onChange(of: sheet.removeRequested) {
            if sheet.removeRequested {
                sheet.removeRequested = false
                confirmingRemoval = true
            }
        }
    }

    private var sheetMenu: some View {
        Menu {
            ForEach(Array(sheet.sheetNames.enumerated()), id: \.offset) { index, name in
                Button {
                    sheet.activateSheet(at: index)
                    renameError = nil
                } label: {
                    if index == sheet.activeSheetIndex {
                        Label(name, systemImage: "checkmark")
                    } else {
                        Text(name)
                    }
                }
            }
            Divider()
            Button("Rename…") { beginRename() }
        } label: {
            HStack(spacing: 5) {
                Image(systemName: sheet.activeSheetIsData ? "cylinder.split.1x2" : "square.on.square")
                    .font(.system(size: theme.fontSize * 0.7))
                    .foregroundStyle(theme.accent.color)
                Text(sheet.activeSheetName)
                    .font(theme.font(scale: 0.93))
                    .foregroundStyle(theme.resultText.color)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 280)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: theme.fontSize * 0.6))
                    .foregroundStyle(theme.secondaryText.color)
            }
            .padding(.leading, 8)
            .padding(.trailing, 6)
            .padding(.vertical, 3)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        // Double-click renames; simultaneous so the menu click stays instant
        // (per the gesture-latency invariant).
        .simultaneousGesture(TapGesture(count: 2).onEnded {
            beginRename()
        })
    }

    private func beginRename() {
        renameDraft = sheet.activeSheetName
        renameError = nil
        isRenaming = true
    }

    private var renameField: some View {
        TextField("Sheet name", text: $renameDraft)
            .textFieldStyle(.roundedBorder)
            .font(theme.font(scale: 0.93))
            .frame(maxWidth: 260)
            .onSubmit {
                if let message = sheet.renameActiveSheet(to: renameDraft) {
                    renameError = message // stay in the field; show why
                } else {
                    renameError = nil
                    isRenaming = false
                }
            }
            .onKeyPress(.escape) {
                isRenaming = false
                renameError = nil
                return .handled
            }
            .onAppear { renameError = nil }
    }
}
