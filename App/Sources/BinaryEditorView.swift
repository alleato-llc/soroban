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
struct BinaryEditorView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager
    private var theme: Theme { themeManager.current }

    @State private var specDraft = ""
    @State private var saveName = ""
    @State private var showingCustom = false
    @State private var showingSave = false

    /// Distinct band colors cycled per field (system colors adapt to light/dark).
    private static let fieldColors: [Color] = [.blue, .green, .orange, .purple, .pink, .teal]

    var body: some View {
        // Re-render when a submission changes `ans` (the engine value isn't
        // observable; `logGeneration` is the bridge, bumped on every submit).
        let _ = session.logGeneration
        return Group {
            switch session.binaryView { // resolved once; subviews take plain data
            case .success(let view): editor(view)
            case .failure(let reason): unavailable(reason)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.inputBackground.color.opacity(0.5))
        .onChange(of: session.activeFormatSpec) { _, spec in specDraft = spec }
        .onAppear { specDraft = session.activeFormatSpec }
    }

    @ViewBuilder
    private func editor(_ view: BinaryView) -> some View {
        let layout = session.activeLayout
        VStack(alignment: .leading, spacing: 6) {
            header(view)
            if showingCustom { customRow }
            if showingSave { saveRow }
            if let layout { bandedGrid(view, layout) } else { plainGrid(view) }
        }
    }

    // MARK: Header — value, hex, width, actions, Format menu

    private func header(_ view: BinaryView) -> some View {
        let decimal = view.value.displayDescription
        let hex = "0x" + String(view.pattern, radix: 16, uppercase: true)
        return HStack(spacing: 10) {
            Text(decimal)
                .font(theme.font())
                .foregroundStyle(session.binaryHasEdits ? theme.accent.color : theme.resultText.color)
                .onTapGesture(count: 2) { session.insert(value: decimal) }
                .help("Double-click to insert the decimal value into the expression")
            Text(hex)
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.secondaryText.color)
                .onTapGesture(count: 2) { session.insert(value: hex) }
                .help("Double-click to insert the hex value into the expression")
            Spacer()
            widthControl(view)
            if session.binaryHasEdits {
                Button { session.cancelBinaryEdits() } label: {
                    Image(systemName: "arrow.uturn.backward")
                }
                .buttonStyle(.plain)
                .foregroundStyle(theme.secondaryText.color)
                .help("Reset to ans")
            }
            Button("Use") { session.useBinaryValue() }
                .controlSize(.small)
                .help("Insert this value into the input line")
            formatMenu
            Button { session.binaryEditorShown = false } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.secondaryText.color)
            .help("Hide the binary editor (⌥⌘B to show again)")
        }
    }

    private var formatMenu: some View {
        Menu {
            Button("None") { session.applyFormat(nil) }
            Section("Presets") {
                ForEach(CalculatorSession.binaryFormatPresets, id: \.name) { preset in
                    Button(preset.name) { session.applyFormat(preset.format) }
                }
            }
            if !session.savedFormats.isEmpty {
                Section("Saved") {
                    ForEach(session.savedFormats, id: \.name) { saved in
                        Button(saved.name) { session.applyFormat(saved.format) }
                    }
                }
            }
            Divider()
            Button("Custom…") {
                specDraft = session.activeFormatSpec
                showingSave = false
                showingCustom = true
            }
            Button("Save current…") { showingCustom = false; showingSave = true }
                .disabled(session.activeFormat == nil)
        } label: {
            Label(session.activeFormatName ?? "Format", systemImage: "rectangle.split.3x1")
                .font(theme.font(scale: 0.8))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    @ViewBuilder
    private func widthControl(_ view: BinaryView) -> some View {
        if case .plain = view.kind {
            // Widths too narrow for the value — OR for the active format's total
            // — are grayed out; the effective width is highlighted.
            let formatBits = session.activeLayout.map { BinaryView.layoutWidth($0) } ?? 0
            let minWidth = max(view.minimumWidth,
                               BinaryView.editableWidths.first { $0 >= formatBits } ?? 0)
            HStack(spacing: 0) {
                ForEach(BinaryView.editableWidths, id: \.self) { w in
                    let tooSmall = w < minWidth
                    Button("\(w)") { session.binaryWidth = w }
                        .buttonStyle(.plain)
                        .font(theme.font(scale: 0.7))
                        .foregroundStyle(
                            tooSmall ? theme.secondaryText.color.opacity(0.3)
                            : w == view.width ? theme.accent.color : theme.secondaryText.color)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(w == view.width ? theme.accent.color.opacity(0.18) : .clear)
                        .disabled(tooSmall)
                        .help(tooSmall ? "Too narrow for this value" : "\(w)-bit register")
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 4)
                .stroke(theme.secondaryText.color.opacity(0.25)))
            .clipShape(RoundedRectangle(cornerRadius: 4))
        } else {
            Text("\(view.signed ? "Int" : "UInt")\(view.width)")
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.accent.color)
        }
    }

    // MARK: Progressive-disclosure rows (custom spec / save)

    private var customRow: some View {
        HStack(spacing: 8) {
            Text("fields").font(theme.font(scale: 0.8)).foregroundStyle(theme.secondaryText.color)
            TextField("owner:3 group:3 other:3", text: $specDraft)
                .textFieldStyle(.roundedBorder)
                .font(theme.font(scale: 0.8))
                .onSubmit { applyCustom() }
            Button("Apply") { applyCustom() }.controlSize(.small)
            Button { showingCustom = false } label: { Image(systemName: "xmark") }
                .buttonStyle(.plain).foregroundStyle(theme.secondaryText.color)
        }
    }

    private var saveRow: some View {
        HStack(spacing: 8) {
            Text("save as").font(theme.font(scale: 0.8)).foregroundStyle(theme.secondaryText.color)
            TextField("name", text: $saveName)
                .textFieldStyle(.roundedBorder)
                .font(theme.font(scale: 0.8))
                .frame(width: 140)
                .onSubmit { saveCurrent() }
            Button("Save") { saveCurrent() }
                .controlSize(.small)
                .disabled(saveName.trimmingCharacters(in: .whitespaces).isEmpty)
            Button { showingSave = false } label: { Image(systemName: "xmark") }
                .buttonStyle(.plain).foregroundStyle(theme.secondaryText.color)
        }
    }

    private func applyCustom() {
        session.applyFormatSpec(specDraft)
        showingCustom = false
    }

    private func saveCurrent() {
        session.saveFormat(named: saveName)
        saveName = ""
        showingSave = false
    }

    // MARK: Plain grid (no format) — nibble groups in even rows

    private func plainGrid(_ view: BinaryView) -> some View {
        let bits = view.bits
        let nibbleStarts = Array(stride(from: 0, to: view.width, by: 4))
        let style = bitStyle
        return NibbleGrid(spacing: 18, lineSpacing: 6) {
            ForEach(nibbleStarts, id: \.self) { start in
                HStack(spacing: 3) {
                    ForEach(start..<min(start + 4, view.width), id: \.self) { p in
                        bitButton(bits: bits, view: view, index: view.width - 1 - p, band: nil, style: style)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: Banded grid (format) — captioned, colored fields with values

    private func bandedGrid(_ view: BinaryView, _ layout: [BinaryView.FieldSpec]) -> some View {
        let bits = view.bits
        let fields = view.fields(layout)
        let total = BinaryView.layoutWidth(layout)
        let unused = view.width - total
        let style = bitStyle
        return FlowLayout(spacing: 14, lineSpacing: 8) {
            if unused > 0 {
                unusedSegment(low: total, count: unused, bits: bits, view: view, style: style)
            }
            ForEach(Array(fields.enumerated()), id: \.element.name) { i, field in
                fieldSegment(field, color: Self.fieldColors[i % Self.fieldColors.count],
                             bits: bits, view: view, style: style)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A named field band: caption, its bits (a flag field captions each bit with
    /// its flag letter), and the readout below — the decoded meaning (`r-x`) for
    /// a flag field, or an editable value for a numeric one.
    private func fieldSegment(_ field: BinaryView.Field, color: Color,
                              bits: [Bool], view: BinaryView, style: BitStyle) -> some View {
        let lo = max(field.lowBit, 0)
        let hi = min(field.lowBit + field.width, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        return VStack(spacing: 3) {
            Text(field.name).font(theme.font(scale: 0.7)).foregroundStyle(color)
            HStack(spacing: 3) {
                ForEach(indices, id: \.self) { index in
                    // Each bit gets its flag letter directly above it (the column
                    // sizes to whichever is wider, so alignment holds for multi-
                    // char flags too).
                    VStack(spacing: 2) {
                        let pos = (field.lowBit + field.width - 1) - index // 0 = field's high bit
                        if let flags = field.flags, pos >= 0, pos < flags.count {
                            Text(flags[pos]).font(theme.font(scale: 0.62)).foregroundStyle(color)
                        }
                        bitButton(bits: bits, view: view, index: index, band: color, style: style)
                    }
                }
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 2)
            .background(color.opacity(0.12))
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(color.opacity(0.4)))
            if let meaning = field.flagString {
                Text(meaning).font(theme.font(scale: 0.78)).foregroundStyle(color)
                    .help("Toggle bits above to change the flags")
            } else {
                TextField("", text: fieldBinding(field))
                    .textFieldStyle(.roundedBorder)
                    .font(theme.font(scale: 0.75))
                    .multilineTextAlignment(.center)
                    .frame(width: max(44, CGFloat(field.width) * 14))
            }
        }
    }

    /// The dim band of bits above the format's coverage.
    private func unusedSegment(low: Int, count: Int, bits: [Bool],
                               view: BinaryView, style: BitStyle) -> some View {
        let lo = max(low, 0)
        let hi = min(low + count, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        return VStack(spacing: 3) {
            Text("unused").font(theme.font(scale: 0.7))
                .foregroundStyle(theme.secondaryText.color.opacity(0.5))
            HStack(spacing: 3) {
                ForEach(indices, id: \.self) { index in
                    bitButton(bits: bits, view: view, index: index, band: nil, style: style)
                }
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 2)
        }
    }

    // MARK: Bit cell

    /// Bundled visual constants so each cell doesn't re-read the theme.
    private struct BitStyle { let accent: Color; let dim: Color; let font: Font }
    private var bitStyle: BitStyle {
        BitStyle(accent: theme.accent.color,
                 dim: theme.secondaryText.color.opacity(0.5),
                 font: theme.font(scale: 0.95))
    }

    private func bitButton(bits: [Bool], view: BinaryView, index: Int,
                           band: Color?, style: BitStyle) -> some View {
        BitButton(set: bits[view.width - 1 - index], accent: style.accent, dim: style.dim,
                  band: band, font: style.font, index: index) {
            session.flipBinaryBit(index)
        }
        .equatable() // skip unchanged bits on a flip
    }

    /// Live edit of a field's value — parses to BigInt and rewrites its bit
    /// range (clamped engine-side).
    private func fieldBinding(_ field: BinaryView.Field) -> Binding<String> {
        Binding(
            get: { String(field.value) },
            set: { text in
                let trimmed = text.trimmingCharacters(in: .whitespaces)
                if trimmed.isEmpty {
                    session.setBinaryField(field.name, to: BigInt(0))
                } else if let value = BigInt(trimmed) {
                    session.setBinaryField(field.name, to: value)
                }
            })
    }

    // MARK: Disabled — explain why this value isn't bit-editable

    private func unavailable(_ reason: BinaryView.Unavailable) -> some View {
        let message: String
        switch reason {
        case .notAnInteger:
            message = "Binary editing needs an integer result."
        case .negative:
            message = "Negative value — wrap it in a signed type (e.g. Int32(…)) to edit its bits."
        case .tooWide:
            message = "Value is over 256 bits — too wide to edit."
        }
        return HStack(spacing: 6) {
            Image(systemName: "info.circle")
            Text(message)
            Spacer()
        }
        .font(theme.font(scale: 0.8))
        .foregroundStyle(theme.secondaryText.color)
    }
}

/// One bit cell — `Equatable` so SwiftUI skips re-rendering bits that didn't
/// change on a flip (the closure is excluded from equality). A band color tints
/// the cell when the bit belongs to a labeled field.
private struct BitButton: View, Equatable {
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
private struct NibbleGrid: Layout {
    var spacing: CGFloat = 18
    var lineSpacing: CGFloat = 6

    private func columns(maxWidth: CGFloat, nibble: CGSize, count: Int) -> Int {
        guard count > 0, nibble.width > 0 else { return max(count, 1) }
        let fit = max(1, Int((maxWidth + spacing) / (nibble.width + spacing)))
        var columns = 1
        while columns * 2 <= min(fit, count) { columns *= 2 }
        return columns
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
private struct FlowLayout: Layout {
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
