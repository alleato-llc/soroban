import SorobanEngine
import Foundation

// MARK: Point mode (Excel-style reference insertion while editing)

extension SheetModel {
    /// Single entry point for cell clicks (plain and shift).
    func handleCellClick(_ address: CellAddress, isShiftDown: Bool) {
        if editing != nil, editing != address, wantsReferenceInsertion(isShiftDown: isShiftDown) {
            insertReference(to: address, extendRange: isShiftDown)
            return
        }
        // Not a reference insert: close any open editor deterministically
        // (its focus-loss commit is still in its grace window).
        if editing != nil, editing != address {
            commitOpenEditor()
        }
        if isShiftDown {
            extendSelection(to: address)
        } else {
            select(address)
        }
    }

    /// Inserts literal text into the OPEN cell editor's draft (the inspector
    /// double-click, when editing a cell). Like point mode, it appends to the
    /// draft and re-grabs editor focus — but it's plain text, not a reference,
    /// so it doesn't arm the re-click-replaces machinery.
    /// Returns false when no editor is open (the caller falls back to the log).
    func insertIntoEditor(_ text: String) -> Bool {
        guard editing != nil else { return false }
        pendingFocusCommit = false // cancel the in-flight focus-loss commit
        let needsSpace = !editingDraft.isEmpty && !editingDraft.hasSuffix(" ")
            && !Calculator.expectsOperand(editingDraft)
        editingDraft += (needsSpace ? " " : "") + text
        // A literal insert isn't a reference — clear the point-mode anchor so
        // a following cell click starts fresh.
        lastInsertedReference = nil
        pointModeExpectedDraft = nil
        editorRefocusTrigger += 1
        return true
    }

    private func wantsReferenceInsertion(isShiftDown: Bool) -> Bool {
        if Calculator.expectsOperand(editingDraft) { return true }
        // Re-click replaces the just-inserted reference; shift-click after an
        // insert extends it into a range.
        return pointModeExpectedDraft == editingDraft
    }

    private func insertReference(to address: CellAddress, extendRange: Bool) {
        pendingFocusCommit = false // cancel the in-flight focus-loss commit

        // Named cells insert their name — names spread naturally. Ranges
        // don't support names, so extension falls back to plain addresses.
        let reference = cellName(at: address).map { "'\($0)'" } ?? "\(address)"
        if pointModeExpectedDraft == editingDraft, let last = lastInsertedReference {
            if extendRange, !last.contains(".."), let lastAddress = lastInsertedAddress {
                // B:1 → B:1..B:4 (address form even if the anchor was named).
                let range = "\(lastAddress)..\(address)"
                editingDraft = String(editingDraft.dropLast(last.count)) + range
                lastInsertedReference = range
            } else {
                editingDraft = String(editingDraft.dropLast(last.count)) + reference
                lastInsertedReference = reference
                lastInsertedAddress = address
            }
        } else {
            editingDraft += reference
            lastInsertedReference = reference
            lastInsertedAddress = address
        }
        pointModeExpectedDraft = editingDraft
        editorRefocusTrigger += 1
    }

    /// Editor's TextField changed — if it wasn't our own splice, the user
    /// typed, which ends the replace-on-click window.
    func noteDraftChanged() {
        if editingDraft != pointModeExpectedDraft {
            pointModeExpectedDraft = nil
            lastInsertedReference = nil
        }
    }

    /// Focus left the editor (mouseDown elsewhere). The commit waits out a
    /// grace window: mouseDown and mouseUp are separate runloop events, so a
    /// single async hop fires BETWEEN them — before the tap that may mean
    /// "insert a reference" has arrived. Cell clicks resolve the edit
    /// explicitly (insert or commitOpenEditor); the timeout covers clicks
    /// that land outside the grid entirely.
    func editorLostFocus(at address: CellAddress) {
        guard editing == address else { return }
        pendingFocusCommit = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            guard let self, self.pendingFocusCommit, self.editing == address else { return }
            self.pendingFocusCommit = false
            self.commit(self.editingDraft, at: address)
            self.endEditing()
        }
    }

    /// Focus came back (e.g. after a reference insert, or the user clicked
    /// the editor again) — the pending commit no longer applies.
    func editorRegainedFocus() {
        pendingFocusCommit = false
    }

    private func commitOpenEditor() {
        guard let address = editing else { return }
        pendingFocusCommit = false
        commit(editingDraft, at: address)
        endEditing()
    }
}
