import SwiftUI
import SorobanEngine

/// A single display cell. Deliberately a *value* view: all inputs are
/// Equatable lets and the explicit `==` lets SwiftUI skip the ~1,000 visible
/// unchanged cell bodies on every selection/edit change. Per-cell @State/
/// @FocusState lives only in CellEditorView — exactly one exists at a time.
struct CellView: View, Equatable {
    let address: CellAddress
    let display: CellDisplay
    let raw: String
    let format: CellFormat
    let isSelected: Bool
    let isAnchor: Bool
    let isEditing: Bool
    let theme: Theme
    let width: CGFloat
    let height: CGFloat

    @Environment(CalculatorSession.self) private var session

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.address == rhs.address
            && lhs.display == rhs.display
            && lhs.raw == rhs.raw
            && lhs.format == rhs.format
            && lhs.isSelected == rhs.isSelected
            && lhs.isAnchor == rhs.isAnchor
            && lhs.isEditing == rhs.isEditing
            && lhs.theme == rhs.theme
            && lhs.width == rhs.width
            && lhs.height == rhs.height
    }

    private var isError: Bool {
        if case .error = display { return true }
        return false
    }

    var body: some View {
        Group {
            if isEditing {
                CellEditorView(address: address, initialDraft: raw, theme: theme)
            } else {
                displayView
            }
        }
        .frame(width: width, height: height)
        .background(backgroundColor)
        .overlay {
            Rectangle()
                .strokeBorder(borderColor, lineWidth: isAnchor ? 1.5 : (isError ? 1 : 0.5))
        }
    }

    private var backgroundColor: Color {
        if isEditing { return theme.inputBackground.color }
        if isSelected { return theme.accent.color.opacity(0.12) }
        if isError { return theme.errorText.color.opacity(0.12) }
        if let fill = format.fillColor { return fill.color.opacity(0.25) }
        return .clear
    }

    private var borderColor: Color {
        if isAnchor { return theme.accent.color }
        if isError { return theme.errorText.color.opacity(0.7) }
        return theme.secondaryText.color.opacity(0.15)
    }

    private var displayView: some View {
        content
            .lineLimit(1)
            .padding(.horizontal, 4)
            .contentShape(Rectangle())
            // Right-click menu: editing verbs up front, formatting tucked
            // into one submenu (user feedback: an all-formatting menu read
            // poorly). Cheap closure — built lazily on demand; actions
            // retarget to this cell when it's outside the selection.
            .contextMenu {
                cellMenu
            }
            // The single tap must fire IMMEDIATELY — stacking onTapGesture
            // (count: 2) over (count: 1) makes SwiftUI hold every click for
            // the double-click window (~0.3s), which reads as a laggy grid.
            // Instead the double-tap runs *simultaneously*: click 1 selects
            // instantly; click 2 selects again + opens the editor
            // (SheetModel.select tolerates either delivery order).
            .onTapGesture {
                // Routes through point mode: while a formula edit expects an
                // operand, this click inserts the reference instead.
                session.sheet.handleCellClick(
                    address, isShiftDown: isShiftKeyDown())
            }
            .simultaneousGesture(TapGesture(count: 2).onEnded {
                // While another cell's editor is open, the double-click's
                // component taps are reference insertions — don't hijack.
                if session.sheet.editing == nil {
                    session.sheet.beginEditing(address)
                }
            })
    }

    @ViewBuilder
    private var content: some View {
        switch display {
        case .empty:
            Color.clear

        case .text(let text):
            styled(Text(text), color: theme.expressionText.color)
                .frame(maxWidth: .infinity, alignment: alignment(default: .leading))

        case .value(let value):
            styled(Text(format.numberFormat.rendered(value)), color: theme.resultText.color)
                .frame(maxWidth: .infinity, alignment: alignment(default: .trailing))
                .help(raw) // tooltip shows the formula

        case .error(let message):
            Text("#ERR")
                .font(theme.font(scale: 0.93))
                .foregroundStyle(theme.errorText.color)
                .frame(maxWidth: .infinity, alignment: .center)
                .help("Error: \(message) — \(raw)")

        case .definition(let glyph):
            // λ tax(x) / 𝑖 rate — a sheet-scoped definition. Accent-tinted
            // and italic so it reads as a name, not data; the tooltip and
            // the editor show the full source.
            Text(glyph)
                .font(theme.font(scale: 0.93).italic())
                .foregroundStyle(theme.accent.color)
                .frame(maxWidth: .infinity, alignment: .leading)
                .help(raw)

        case .note(let comment):
            // A comment cell — dim and italic, holds no value (a free-
            // floating annotation, skipped in ranges).
            Text(comment)
                .font(theme.font(scale: 0.93).italic())
                .foregroundStyle(theme.secondaryText.color)
                .frame(maxWidth: .infinity, alignment: .leading)
                .help(raw)

        case .slider(let info):
            // Drag the track to change the value (Return still edits the
            // expression; the value text shows the cell's number format).
            SliderCellContent(address: address, info: info,
                              valueText: format.numberFormat.rendered(info.value),
                              widestValueText: info.widestValueText(format: format.numberFormat),
                              theme: theme)
                .help("\(info.name.map { "\($0) — " } ?? "")\(raw)")

        case .stepper(let info):
            StepperCellContent(address: address, info: info,
                               valueText: format.numberFormat.rendered(info.value),
                               theme: theme)
                .help("\(info.name.map { "\($0) — " } ?? "")\(raw)")

        case .checkbox(let info):
            CheckboxCellContent(address: address, info: info, theme: theme)
                .help("\(info.name.map { "\($0) — " } ?? "")\(raw)")

        case .dropdown(let info):
            DropdownCellContent(address: address, info: info, theme: theme)
                .help("\(info.name.map { "\($0) — " } ?? "")\(raw)")
        }
    }

    @ViewBuilder
    private var cellMenu: some View {
        let sheet = session.sheet
        Button("Cut") {
            sheet.retargetSelection(toInclude: address)
            sheet.cutSelectionToPasteboard()
        }
        Button("Copy") {
            sheet.retargetSelection(toInclude: address)
            sheet.copySelectionToPasteboard()
        }
        Button("Paste") {
            sheet.retargetSelection(toInclude: address)
            sheet.pasteFromPasteboard()
        }
        Button("Delete") {
            sheet.retargetSelection(toInclude: address)
            sheet.clearSelection()
        }

        Divider()

        // Named cells — a name for the LOCATION, usable as 'Projected Rate'
        // in formulas and the log. Hidden on data sheets.
        if !sheet.activeSheetIsData {
            if let name = sheet.cellName(at: address) {
                Button("Rename '\(name)'…") {
                    promptForCellName(sheet: sheet, address: address, current: name)
                }
                Button("Remove Name…") {
                    confirmNameRemoval(sheet: sheet, address: address, name: name)
                }
            } else {
                Button("Name Cell…") {
                    promptForCellName(sheet: sheet, address: address, current: nil)
                }
            }

            Divider()
        }

        Menu("Format") {
            FormatActions(sheet: sheet, clickedCell: address)
        }
    }

    /// NSAlert with a text field — names are rare enough that a modal beats
    /// new chrome. Re-prompts with the error message on invalid input.
    private func promptForCellName(sheet: SheetModel, address: CellAddress, current: String?) {
        sheet.retargetSelection(toInclude: address)
        var message: String? = nil
        while true {
            let alert = NSAlert()
            alert.messageText = current == nil ? "Name Cell \(address)" : "Rename '\(current!)'"
            alert.informativeText = message
                ?? "Reference it as 'The Name' in formulas and the log (up to 64 characters, unique on this sheet)."
            let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
            field.stringValue = current ?? ""
            field.placeholderString = "Projected Rate"
            alert.accessoryView = field
            alert.window.initialFirstResponder = field
            alert.addButton(withTitle: current == nil ? "Name" : "Rename")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
            if let error = sheet.nameCell(field.stringValue, at: address) {
                message = error // show why, ask again
                continue
            }
            return
        }
    }

    /// Your delete flow: break references / replace them with the address /
    /// cancel. Skips the dialog when nothing references the name.
    private func confirmNameRemoval(sheet: SheetModel, address: CellAddress, name: String) {
        sheet.retargetSelection(toInclude: address)
        let references = sheet.referenceCount(toNameAt: address)
        guard references > 0 else {
            sheet.removeCellName(at: address, mode: .breakReferences)
            return
        }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Remove the name '\(name)'?"
        alert.informativeText =
            "\(references) formula\(references == 1 ? "" : "s") reference\(references == 1 ? "s" : "") it. " +
            "Break them (they'll show errors), or replace the name with \(address) everywhere?"
        alert.addButton(withTitle: "Replace with \(address)")
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Break References")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            sheet.removeCellName(at: address, mode: .inlineAddresses)
        case .alertThirdButtonReturn:
            sheet.removeCellName(at: address, mode: .breakReferences)
        default:
            break
        }
    }

    /// Text/value styling from the cell's format (errors and definitions
    /// keep their fixed look; their fill still applies).
    private func styled(_ text: Text, color: Color) -> Text {
        var font = theme.font(scale: 0.93)
        if format.bold { font = font.bold() }
        if format.italic { font = font.italic() }
        var styledText = text.font(font)
            .foregroundStyle(format.textColor?.color ?? color)
        if format.underline { styledText = styledText.underline() }
        if format.strikethrough { styledText = styledText.strikethrough() }
        return styledText
    }

    private func alignment(default automatic: Alignment) -> Alignment {
        switch format.alignment {
        case .auto: return automatic
        case .left: return .leading
        case .center: return .center
        case .right: return .trailing
        }
    }
}

extension PaletteColor {
    /// System colors adapt to light/dark, keeping the palette legible across
    /// the switchable themes (the reason colors are stored semantically).
    var color: Color {
        switch self {
        case .red: return Color(nsColor: .systemRed)
        case .orange: return Color(nsColor: .systemOrange)
        case .yellow: return Color(nsColor: .systemYellow)
        case .green: return Color(nsColor: .systemGreen)
        case .blue: return Color(nsColor: .systemBlue)
        case .purple: return Color(nsColor: .systemPurple)
        case .gray: return Color(nsColor: .systemGray)
        }
    }

    var label: String { rawValue.capitalized }
}
