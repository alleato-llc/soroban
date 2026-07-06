import SwiftUI
import SorobanEngine
import BigInt

/// The binary bit-editor overlay (Programmer mode): a clickable bit grid bound
/// to `ans`, macOS-Calculator-style. Clicking bits stages a new value live (no
/// log spam); `Use` drops it into the expression.
///
/// A bit-field FORMAT (a map like `{owner: 3, …}`) turns the grid into a labeled
/// register diagram: each field is a captioned, colored band with its value
/// shown and editable beneath it. The format controls progressively disclose —
/// the resting state is just the value, the grid, and a `Format ▾` menu.
///
/// Perf: `binaryView` is resolved ONCE per render; bit cells are an `Equatable`
/// `BitButton` so a flip only re-renders the bit that changed.
///
/// The view is split across sibling files: `BinaryEditorView+Header.swift`
/// (value/hex/width/actions/Format menu + save & rename rows),
/// `BinaryEditorView+Builder.swift` (the visual format builder), and
/// `BinaryEditorView+Grid.swift` (plain & banded bit grids). This file holds
/// the state, the palette helpers, and the top-level `body`/`editor` layout.
public struct BinaryEditorView<Host: BinaryEditorHost>: View {
    let host: Host
    var theme: BinaryEditorTheme { host.theme }

    public init(host: Host) { self.host = host }

    @State var saveName = ""
    @State var showingSave = false

    // Out-of-format ("unused") bits are locked until the user enables editing
    // with a deliberate double-click (confirmed once per session).
    @State var allowUnused = false
    @State var confirmUnused = false

    // Renaming a saved format (hosts that manage their own format store).
    @State var renameTarget: String?
    @State var renameText = ""

    // Visual format builder (build mode): drag/click free bits to carve a group,
    // detail it (name/kind/labels), add it; live-preview as you go; save. The
    // logic lives in the engine's BinaryView.FormatBuilder; this view is bindings.
    @State var building = false
    // Placeholder palette; startBuilding() rebuilds it with the real palette
    // (a generic type's static can't seed a @State default).
    @State var builder = BinaryView.FormatBuilder(palette: [])
    @State var builderName = ""

    /// Distinct band colors cycled per field (system colors adapt to light/dark),
    /// paired with the NAME persisted in a field's `color`. Computed (a generic
    /// type can't hold a static STORED property).
    static var fieldColors: [(name: String, color: Color)] {
        // Order + names are the engine's canonical palette; this only adds the
        // SwiftUI Color for each (the engine treats color names as opaque).
        BinaryEditorPalette.names.map { ($0, paletteColor(named: $0)) }
    }
    private static func paletteColor(named name: String) -> Color {
        switch name {
        case "blue": .blue
        case "green": .green
        case "orange": .orange
        case "purple": .purple
        case "pink": .pink
        case "teal": .teal
        default: .gray
        }
    }

    /// Map a persisted color name to a color; unknown/nil falls back to the
    /// palette cycled by `position`.
    static func color(named name: String?, position: Int) -> Color {
        if let name, let match = fieldColors.first(where: { $0.name == name }) { return match.color }
        return fieldColors[position % fieldColors.count].color
    }

    public var body: some View {
        Group {
            switch host.binaryView { // resolved once; subviews take plain data
            case .success(let view): editor(view)
            case .failure(let reason): unavailable(reason)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.inputBackground.opacity(0.5))
    }

    @ViewBuilder
    func editor(_ view: BinaryView) -> some View {
        let layout = host.activeLayout
        VStack(alignment: .leading, spacing: 6) {
            header(view)
            if building {
                buildMode(view)
            } else {
                if showingSave { saveRow }
                if renameTarget != nil { renameRow }
                if let layout { bandedGrid(view, layout) } else { plainGrid(view) }
            }
        }
        .confirmationDialog("Edit bits outside the format?", isPresented: $confirmUnused,
                            titleVisibility: .visible) {
            Button("Enable editing") { allowUnused = true }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("These bits are outside the active format — values set here will exceed the format's range.")
        }
    }
}

/// One bit cell — `Equatable` so SwiftUI skips re-rendering bits that didn't
/// change on a flip (the closure is excluded from equality). A band color tints
/// the cell when the bit belongs to a labeled field.
struct BitButton: View, Equatable {
    let set: Bool
    let accent: Color
    let dim: Color
    let band: Color?
    let font: Font
    let index: Int
    let onTap: () -> Void

    nonisolated static func == (a: BitButton, b: BitButton) -> Bool {
        a.set == b.set && a.band == b.band && a.index == b.index
            && a.accent == b.accent && a.dim == b.dim && a.font == b.font
    }

    var body: some View {
        Button(action: onTap) {
            Text(set ? "1" : "0")
                .font(font)
                .monospacedDigit()
                .foregroundStyle(band.map { set ? $0 : $0.opacity(0.45) } ?? (set ? accent : dim))
                .padding(.horizontal, 2)
                .padding(.vertical, 1)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("bit \(index)")
    }
}

/// Even, power-of-two nibble rows (no orphaned single nibble) — for the plain
/// (no-format) grid.
struct NibbleGrid: Layout {
    var spacing: CGFloat = 18
    var lineSpacing: CGFloat = 6

    private func columns(maxWidth: CGFloat, nibble: CGSize, count: Int) -> Int {
        nibbleColumnCount(maxWidth: maxWidth, itemWidth: nibble.width, spacing: spacing, count: count)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let first = subviews.first else { return .zero }
        let nibble = first.sizeThatFits(.unspecified)
        let maxWidth = proposal.width ?? .greatestFiniteMagnitude
        let columns = columns(maxWidth: maxWidth, nibble: nibble, count: subviews.count)
        let rows = (subviews.count + columns - 1) / columns
        let width = CGFloat(columns) * nibble.width + CGFloat(columns - 1) * spacing
        let height = CGFloat(rows) * nibble.height + CGFloat(max(rows - 1, 0)) * lineSpacing
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let first = subviews.first else { return }
        let nibble = first.sizeThatFits(.unspecified)
        let columns = columns(maxWidth: bounds.width, nibble: nibble, count: subviews.count)
        for (i, view) in subviews.enumerated() {
            let x = bounds.minX + CGFloat(i % columns) * (nibble.width + spacing)
            let y = bounds.minY + CGFloat(i / columns) * (nibble.height + lineSpacing)
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(nibble))
        }
    }
}

/// A greedy left-to-right flow that wraps, top-aligned — for the variable-width
/// field bands (whole fields wrap as units).
struct FlowLayout: Layout {
    var spacing: CGFloat = 14
    var lineSpacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .greatestFiniteMagnitude
        var x: CGFloat = 0, y: CGFloat = 0, lineHeight: CGFloat = 0, widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > maxWidth { x = 0; y += lineHeight + lineSpacing; lineHeight = 0 }
            x += size.width + spacing
            widest = max(widest, x - spacing)
            lineHeight = max(lineHeight, size.height)
        }
        return CGSize(width: min(widest, maxWidth), height: y + lineHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let maxWidth = bounds.width
        var x: CGFloat = 0, y: CGFloat = 0, lineHeight: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > maxWidth { x = 0; y += lineHeight + lineSpacing; lineHeight = 0 }
            view.place(at: CGPoint(x: bounds.minX + x, y: bounds.minY + y),
                       anchor: .topLeading, proposal: ProposedViewSize(size))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}
