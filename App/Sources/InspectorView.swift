import SwiftUI
import SorobanEngine

/// Trailing sidebar showing the live environment. This OUTER view owns only
/// the width + resize handle; the entry list lives in `InspectorContent`, a
/// separate child. The split is load-bearing: dragging changes `dragWidth`
/// (this view's @State) but NOT anything InspectorContent reads, so SwiftUI
/// skips the content's body during a resize — the entries (each evaluating
/// definedValue/numericValue) don't recompute per tick.
struct InspectorView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    /// Live drag width — committed to the session only on release.
    @State private var dragWidth: CGFloat?

    var body: some View {
        InspectorContent()
            .frame(width: dragWidth ?? session.inspectorWidth)
            .background(themeManager.current.windowBackground.color)
            // Drag the leading edge to resize (the divider sits to our left
            // in ContentView; this handle overlays it). Dragging RIGHT narrows.
            .overlay(alignment: .leading) {
                Rectangle()
                    .fill(.clear)
                    .frame(width: 8)
                    .contentShape(Rectangle())
                    .onHover { $0 ? NSCursor.resizeLeftRight.push() : NSCursor.pop() }
                    .gesture(DragGesture(minimumDistance: 1)
                        .onChanged { drag in
                            // session.inspectorWidth is the stable base — not
                            // committed until release, and translation is
                            // cumulative from the drag's start.
                            dragWidth = clamped(session.inspectorWidth - drag.translation.width)
                        }
                        .onEnded { _ in
                            if let dragWidth { session.inspectorWidth = dragWidth }
                            dragWidth = nil
                        })
            }
    }

    private func clamped(_ width: CGFloat) -> CGFloat {
        min(max(width, CalculatorSession.inspectorWidthRange.lowerBound),
            CalculatorSession.inspectorWidthRange.upperBound)
    }
}

/// The inspector's scrolling entry list. Reads the two observation bridges
/// (session.environmentGeneration for the log half, sheet.generation for the
/// sheet half) so it refreshes on any environment change — but NOT on resize
/// (it never reads the drag width).
private struct InspectorContent: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager

    private var theme: Theme { themeManager.current }

    var body: some View {
        // Touch both generations so SwiftUI re-reads on any change.
        let _ = (session.environmentGeneration, session.sheet.generation)

        VStack(alignment: .leading, spacing: 0) {
            Text("Environment")
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.secondaryText.color)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    section("Variables", entries: variableEntries)
                    section("Functions", entries: functionEntries)
                    section("Data Types", entries: dataTypeEntries)
                    section("Named Cells", entries: session.sheet.namedCells())
                    if isEmpty {
                        Text("Define a variable or function to see it here.")
                            .font(theme.font(scale: 0.82))
                            .foregroundStyle(theme.secondaryText.color)
                            .padding(12)
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: Sections

    @ViewBuilder
    private func section(_ title: String, entries: [SheetModel.EnvEntry]) -> some View {
        if !entries.isEmpty {
            Text(title.uppercased())
                .font(theme.font(scale: 0.7))
                .foregroundStyle(theme.secondaryText.color)
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 3)
            ForEach(entries) { entry in
                EntryRow(entry: entry, theme: theme)
            }
        }
    }

    // MARK: Data sources (log half + sheet half, merged)

    private var variableEntries: [SheetModel.EnvEntry] {
        let log = session.logVariables.sorted { $0.key < $1.key }.map { name, value in
            SheetModel.EnvEntry(id: "log.var.\(name)", name: name,
                                detail: value.description, provenance: .log)
        }
        return log + session.sheet.sheetVariables()
    }

    private var functionEntries: [SheetModel.EnvEntry] {
        let log = session.logFunctions.values.sorted { $0.name < $1.name }.map { fn in
            SheetModel.EnvEntry(id: "log.fn.\(fn.name)", name: fn.signature,
                                detail: "ƒ", provenance: .log)
        }
        return log + session.sheet.sheetFunctions()
    }

    private var dataTypeEntries: [SheetModel.EnvEntry] {
        let log = session.logDataTypes.values.sorted { $0.name < $1.name }.map { type in
            SheetModel.EnvEntry(id: "log.type.\(type.name)", name: type.name,
                                detail: "𝑫", provenance: .log)
        }
        return log + session.sheet.sheetDataTypes()
    }

    private var isEmpty: Bool {
        variableEntries.isEmpty && functionEntries.isEmpty
            && dataTypeEntries.isEmpty && session.sheet.namedCells().isEmpty
    }
}

/// One inspector row, three click targets:
///  • double-click the NAME → insert the name into the active editor
///    (the log input, or the open cell editor in grid mode)
///  • double-click the VALUE → insert the value the same way
///  • click the source BADGE (A:7 ↗) → navigate to the defining cell
/// Stateless — no per-row @State.
private struct EntryRow: View {
    let entry: SheetModel.EnvEntry
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 1) {
                insertable(entry.name, inserting: insertableName,
                           color: theme.expressionText.color, scale: 0.85)
                insertable(entry.detail, inserting: entry.detail,
                           color: theme.resultText.color, scale: 0.78)
            }
            Spacer(minLength: 4)
            badge
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .help(helpText)
        .contextMenu {
            Button("Insert Name") { insert(insertableName) }
            Button("Insert Value") { insert(entry.detail) }
            Button("Copy Value") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(entry.detail, forType: .string)
            }
            if case .cell = entry.provenance {
                Button("Go to Definition", action: jump)
            }
        }
    }

    /// A label whose double-click inserts `text`. The double-tap is a
    /// simultaneousGesture (never stacked onTapGesture(count:2)) per the
    /// grid-perf latency rule.
    private func insertable(_ label: String, inserting text: String,
                            color: Color, scale: CGFloat) -> some View {
        Text(label)
            .font(theme.font(scale: scale))
            .foregroundStyle(color)
            .lineLimit(1)
            .contentShape(Rectangle())
            .simultaneousGesture(TapGesture(count: 2).onEnded { insert(text) })
    }

    /// Insert text into whatever expression field is active: the open cell
    /// editor (grid mode, mid-edit) or the log input. No-op if neither —
    /// in grid mode you must be editing a cell (the input bar is hidden).
    private func insert(_ text: String) {
        if session.sheet.insertIntoEditor(text) { return }
        guard session.activeView == .log else { return }
        session.insert(value: text)
    }

    @ViewBuilder
    private var badge: some View {
        switch entry.provenance {
        case .log:
            Text("log")
                .font(theme.font(scale: 0.68))
                .foregroundStyle(theme.secondaryText.color)
        case .cell(_, let address):
            // The source link — a single click navigates to the cell.
            Button(action: jump) {
                HStack(spacing: 1) {
                    Text("\(address)")
                    Image(systemName: "arrow.up.right")
                }
                .font(theme.font(scale: 0.68))
                .foregroundStyle(theme.accent.color)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Go to \(address)")
        }
    }

    /// Named cells insert as 'Name'; functions drop their params; everything
    /// else is its bare name.
    private var insertableName: String {
        entry.name.hasPrefix("'") ? entry.name
            : String(entry.name.prefix { $0 != "(" })
    }

    private var helpText: String {
        switch entry.provenance {
        case .log: return "Defined in the calculation log"
        case .cell(let sheet, let address): return "Defined in \(sheet)!\(address)"
        }
    }

    private func jump() {
        if case .cell(let sheet, let address) = entry.provenance {
            session.jumpTo(sheetNamed: sheet, address: address)
        }
    }
}
