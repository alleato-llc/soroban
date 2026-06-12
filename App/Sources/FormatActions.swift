import SwiftUI
import SorobanEngine

/// The formatting actions — ONE definition shared verbatim by the menu-bar
/// Format menu (where the keyboard shortcuts register globally) and every
/// cell's right-click context menu, so the two can never drift.
struct FormatActions: View {
    let sheet: SheetModel
    /// Set by the context menu: right-clicking a cell OUTSIDE the current
    /// selection retargets the selection to that cell first (Excel-style —
    /// the action applies where you clicked).
    var clickedCell: CellAddress? = nil

    private var current: CellFormat { sheet.selectionFormat ?? CellFormat() }

    var body: some View {
        toggle("Bold", \.bold)
            .keyboardShortcut("b", modifiers: .command)
        toggle("Italic", \.italic)
            .keyboardShortcut("i", modifiers: .command)
        toggle("Underline", \.underline)
            .keyboardShortcut("u", modifiers: .command)
        toggle("Strikethrough", \.strikethrough)
            .keyboardShortcut("x", modifiers: [.command, .shift])

        Menu("Alignment") {
            alignment("Automatic", .auto)
            alignment("Left", .left)
                .keyboardShortcut("{", modifiers: .command)
            alignment("Center", .center)
                .keyboardShortcut("|", modifiers: .command)
            alignment("Right", .right)
                .keyboardShortcut("}", modifiers: .command)
        }
        Menu("Text Color") {
            colorButton("None", nil, \.textColor)
            ForEach(PaletteColor.allCases, id: \.self) { palette in
                colorButton(palette.label, palette, \.textColor)
            }
        }
        Menu("Fill Color") {
            colorButton("None", nil, \.fillColor)
            ForEach(PaletteColor.allCases, id: \.self) { palette in
                colorButton(palette.label, palette, \.fillColor)
            }
        }

        Divider()

        Menu("Number Format") {
            numberFormat("General", .general, matches: { $0 == .general })
            numberFormat("Number", .number(decimals: 2), matches: {
                if case .number = $0 { return true }; return false
            })
            Menu("Currency") {
                ForEach(Self.currencySymbols, id: \.self) { symbol in
                    numberFormat(symbol, .currency(symbol: symbol, decimals: 2), matches: {
                        if case .currency(let s, _) = $0 { return s == symbol }; return false
                    })
                }
            }
            numberFormat("Percent", .percent(decimals: 2), matches: {
                if case .percent = $0 { return true }; return false
            })
            numberFormat("Date", .date, matches: { $0 == .date })
            numberFormat("Hex", .hex, matches: { $0 == .hex })
            numberFormat("Binary", .binary, matches: { $0 == .binary })
        }
        Button("Increase Decimals") {
            act { sheet.applyFormat { $0.numberFormat = $0.numberFormat.adjustingDecimals(by: 1) } }
        }
        .keyboardShortcut(".", modifiers: [.command, .control])
        Button("Decrease Decimals") {
            act { sheet.applyFormat { $0.numberFormat = $0.numberFormat.adjustingDecimals(by: -1) } }
        }
        .keyboardShortcut(",", modifiers: [.command, .control])

        Divider()

        Button("Clear Formatting") {
            act { sheet.applyFormat { $0 = CellFormat() } }
        }
    }

    /// The locale's symbol first (the default most people want), then the
    /// fixed set; the choice is STORED in the cell so workbooks render the
    /// same everywhere.
    static var currencySymbols: [String] {
        let locale = Locale.current.currencySymbol ?? "$"
        var symbols = [locale]
        for fixed in ["$", "€", "£", "¥"] where !symbols.contains(fixed) {
            symbols.append(fixed)
        }
        return symbols
    }

    // MARK: Pieces

    private func toggle(_ title: String, _ keyPath: WritableKeyPath<CellFormat, Bool>) -> some View {
        Toggle(title, isOn: Binding(
            get: { current[keyPath: keyPath] },
            set: { _ in act { sheet.toggleStyle(keyPath) } }))
    }

    private func alignment(_ title: String, _ value: CellAlignment) -> some View {
        Toggle(title, isOn: Binding(
            get: { current.alignment == value },
            set: { _ in act { sheet.applyFormat { $0.alignment = value } } }))
    }

    private func colorButton(_ title: String, _ value: PaletteColor?,
                             _ keyPath: WritableKeyPath<CellFormat, PaletteColor?>) -> some View {
        Toggle(isOn: Binding(
            get: { current[keyPath: keyPath] == value },
            set: { _ in act { sheet.applyFormat { $0[keyPath: keyPath] = value } } })) {
            if let value {
                Label {
                    Text(title)
                } icon: {
                    Image(systemName: "square.fill").foregroundStyle(value.color)
                }
            } else {
                Text(title)
            }
        }
    }

    private func numberFormat(_ title: String, _ format: NumberFormat,
                              matches: @escaping (NumberFormat) -> Bool) -> some View {
        Toggle(title, isOn: Binding(
            get: { matches(current.numberFormat) },
            set: { _ in act { sheet.applyFormat { $0.numberFormat = format } } }))
    }

    /// Retarget the selection to the right-clicked cell when it's outside
    /// the current rectangle, then run.
    private func act(_ run: () -> Void) {
        if let cell = clickedCell {
            sheet.retargetSelection(toInclude: cell)
        }
        run()
    }
}
