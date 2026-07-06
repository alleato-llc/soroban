import SorobanEngine
import BigInt
import BinaryEditorKit

// The binary bit-editor surface (Programmer mode): the live bit view, bit
// flips, the named-bit-field formats (presets + saved), and the visual format
// builder. The stored state lives on the class body in `CalculatorSession.swift`.

extension CalculatorSession {
    // MARK: Binary bit-editor (Programmer mode)

    /// The bit view the overlay renders: the live draft if editing, else `ans`.
    var binaryView: Result<BinaryView, BinaryView.Unavailable> {
        BinaryView.make(for: binaryDraft ?? ans, preferredWidth: binaryWidth)
    }
    /// True when there are uncommitted bit flips (the commit affordance shows).
    var binaryHasEdits: Bool { binaryDraft != nil }

    /// Flip bit `index` (0 = LSB) of the working value, staging it as a draft
    /// (no log entry — that waits for `commitBinary`).
    func flipBinaryBit(_ index: Int) {
        guard case .success(let view) = binaryView else { return }
        binaryDraft = view.flippingBit(index).value
    }

    /// Insert the current (possibly bit-edited) value into the input line as a
    /// literal — you fold it into an expression and submit when ready, rather
    /// than it landing in the log on its own. A plain integer inserts as a `0b…`
    /// binary literal (you were editing bits); a typed `Int…` inserts its
    /// canonical constructor (which carries the type and sign).
    func useBinaryValue() {
        guard case .success(let view) = binaryView else { return }
        switch view.kind {
        case .plain: insert(value: "0b" + String(view.pattern, radix: 2))
        case .fixed: insert(value: view.value.description)
        }
    }

    /// Reset the grid to `ans`, discarding staged bit edits.
    func cancelBinaryEdits() { binaryDraft = nil }

    // MARK: Binary bit-editor — formats (named bit ranges)

    /// The active format decoded to ordered fields (each with optional per-bit flags).
    var activeLayout: [BinaryView.FieldSpec]? {
        activeFormat.flatMap { BinaryView.layout(from: $0) }
    }

    /// Built-in formats — the shared set in `BinaryEditorKit` (so the calculator
    /// and Tama present the same presets).
    static let binaryFormatPresets = BinaryEditorPresets.standard

    /// Custom/saved formats persisted in the workbook — any environment variable
    /// that is a map of positive-integer widths reads back as a format.
    var savedFormats: [(name: String, format: Value)] {
        logVariables
            .compactMap { BinaryView.layout(from: $0.value) != nil ? ($0.key, $0.value) : nil }
            .sorted { $0.0 < $1.0 }
    }

    /// The display name of the active format for the menu label — a preset/saved
    /// name when it matches one, else "Custom"; nil when no format is active.
    var activeFormatName: String? {
        guard let format = activeFormat else { return nil }
        if let preset = Self.binaryFormatPresets.first(where: { $0.format == format }) { return preset.name }
        if let saved = savedFormats.first(where: { $0.format == format }) { return saved.name }
        return "Custom"
    }

    func applyFormat(_ value: Value?) {
        activeFormat = value.flatMap { BinaryView.layout(from: $0) != nil ? $0 : nil }
        fitWidthToFormat()
    }

    /// Widen a plain register to at least the active format's total, so the
    /// fields aren't clipped by a too-narrow width (a fixed-width int can't grow).
    private func fitWidthToFormat() {
        guard let layout = activeLayout else { return }
        // A format owns its register width — snap to it (grow OR shrink), so
        // IPv4 is 32 bits, MAC 48, IPv6 128 — never wider.
        if let fit = BinaryView.editableWidths.first(where: { $0 >= BinaryView.layoutWidth(layout) }) {
            binaryWidth = fit
        }
    }

    /// Defines the `Bits` schema once per workbook (a one-time log line),
    /// preserving any in-progress input. A no-op once `Bits::BitFormat` exists.
    /// (Schema + serializer are the shared `BinaryEditorBits` in the engine.)
    private func ensureBitsSchema() {
        guard calculator.environment.dataType(named: "Bits::BitFormat") == nil else { return }
        let stash = input
        input = BinaryEditorBits.schemaSource
        submit()
        suppressNextSuggestionRefresh = true
        input = stash
    }

    /// Persists a layout as a typed `name = Bits::BitFormat(...)` log assignment,
    /// so it lives in the workbook and reappears in `savedFormats`; re-points the
    /// active format at the saved record (a map and a record never compare equal,
    /// so the menu would otherwise read "Custom"). Preserves the input line.
    private func persistFormat(_ layout: [BinaryView.FieldSpec], named name: String) {
        let stash = input
        ensureBitsSchema()
        input = "\(name) = \(BinaryEditorBits.formatSource(layout))"
        submit()
        if let saved = calculator.environment.userVariables[name] { activeFormat = saved }
        suppressNextSuggestionRefresh = true
        input = stash
    }

    /// Persist the active format under a name (the "Save current…" path).
    func saveFormat(named name: String) {
        guard let layout = activeLayout else { return }
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        persistFormat(layout, named: trimmed)
    }

    // MARK: Binary bit-editor — visual format builder

    /// Apply a freshly-built layout WITHOUT saving it (transient session state,
    /// like a preset). Defines the schema if needed, then evaluates the typed
    /// constructor off the log (`evaluateFormula` never logs or touches `ans`).
    func applyBuiltFormat(_ layout: [BinaryView.FieldSpec]) {
        guard !layout.isEmpty else { return }
        ensureBitsSchema()
        if case .success(let value) = calculator.evaluateFormula(BinaryEditorBits.formatSource(layout)) {
            applyFormat(value)
        }
    }

    /// Save a freshly-built layout under a name (persists + applies).
    func saveBuiltFormat(_ layout: [BinaryView.FieldSpec], named name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !layout.isEmpty, !trimmed.isEmpty else { return }
        persistFormat(layout, named: trimmed)
        fitWidthToFormat()
    }

    /// Rename a saved format (host-managed): re-store its value under the new
    /// name and drop the old. The active format follows automatically — its
    /// value is unchanged, so the menu re-labels via the value match.
    func renameSavedFormat(_ oldName: String, to newName: String) {
        let trimmed = newName.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, trimmed != oldName,
              let value = calculator.environment.userVariables[oldName],
              BinaryView.layout(from: value) != nil else { return }
        calculator.setUserVariable(trimmed, to: value)
        calculator.removeUserVariable(oldName)
        environmentGeneration += 1
        workbook.noteContentChanged()
    }

    /// Delete a saved format (host-managed): drop the variable. If it was the
    /// active format, clear it (the value would otherwise dangle as "Custom").
    func deleteSavedFormat(_ name: String) {
        guard calculator.environment.userVariables[name] != nil else { return }
        let wasActive = activeFormatName == name
        calculator.removeUserVariable(name)
        if wasActive { applyFormat(nil) }
        environmentGeneration += 1
        workbook.noteContentChanged()
    }

    /// The current binary value decoded into the active format's fields.
    var binaryFields: [BinaryView.Field] {
        guard let layout = activeLayout, case .success(let view) = binaryView else { return [] }
        return view.fields(layout)
    }

    /// Edit a field by value (writes only its bit range, clamped), staging a draft.
    func setBinaryField(_ name: String, to value: BigInt) {
        guard let layout = activeLayout, case .success(let view) = binaryView else { return }
        binaryDraft = view.setting(field: name, to: value, layout: layout).value
    }
}
