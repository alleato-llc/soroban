import SwiftUI
import SorobanEngine

/// The one live cell editor (only the editing cell instantiates it). The
/// draft lives in SheetModel so point-mode clicks can splice references in.
struct CellEditorView: View {
    let address: CellAddress
    let initialDraft: String
    let theme: Theme

    @Environment(CalculatorSession.self) private var session
    @FocusState private var focused: Bool

    private var sheet: SheetModel { session.sheet }

    var body: some View {
        @Bindable var sheet = session.sheet

        TextField("", text: $sheet.editingDraft)
            .textFieldStyle(.plain)
            .font(theme.font(scale: 0.93))
            .foregroundStyle(theme.resultText.color)
            .padding(.horizontal, 4)
            .onAppear {
                // Grab focus one tick later: onAppear fires mid-update inside
                // the lazy grid, where FocusState changes don't stick — the
                // input bar kept the keyboard until a second click.
                DispatchQueue.main.async {
                    focused = true
                }
            }
            .focused($focused)
            .onChange(of: sheet.editingDraft) {
                sheet.noteDraftChanged()
            }
            // A point-mode insert pulled focus to the clicked cell; take it back.
            .onChange(of: sheet.editorRefocusTrigger) {
                DispatchQueue.main.async {
                    focused = true
                }
            }
            .onSubmit {
                sheet.commit(sheet.editingDraft, at: address)
                sheet.endEditing()
                sheet.moveSelection(rowDelta: 1, columnDelta: 0) // Return ↓
            }
            .onKeyPress(.tab) {
                sheet.commit(sheet.editingDraft, at: address)
                sheet.endEditing()
                sheet.moveSelection(rowDelta: 0, columnDelta: 1) // Tab →
                return .handled
            }
            .onKeyPress(.escape) {
                sheet.endEditing() // discard draft, keep the selection
                return .handled
            }
            .onChange(of: focused) {
                // Clicking elsewhere commits — after a grace window, so a
                // reference-inserting click can cancel it (focus is lost on
                // mouseDown, before the tap that means "insert B:3" arrives).
                if focused {
                    sheet.editorRegainedFocus()
                } else if sheet.editing == address {
                    sheet.editorLostFocus(at: address)
                }
            }
    }
}
