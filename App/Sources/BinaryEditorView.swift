import SwiftUI
import SorobanEngine

/// The binary bit-editor overlay (Programmer mode): a clickable bit grid bound
/// to `ans`, macOS-Calculator-style. Clicking bits stages a new value live (no
/// log spam); `Commit` drops it into the log as one entry. The bit logic lives
/// in the engine's `BinaryView`; this is just its presentation.
struct BinaryEditorView: View {
    @Environment(CalculatorSession.self) private var session
    @Environment(ThemeManager.self) private var themeManager
    private var theme: Theme { themeManager.current }

    var body: some View {
        // Re-render when a submission changes `ans` (the engine value isn't
        // observable; `logGeneration` is the bridge, bumped on every submit).
        let _ = session.logGeneration
        return VStack(alignment: .leading, spacing: 6) {
            switch session.binaryView {
            case .success(let view):
                header(view)
                bitGrid(view)
            case .failure(let reason):
                unavailable(reason)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.inputBackground.color.opacity(0.5))
    }

    // MARK: Header — running value, hex, width control, commit

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
            // Drop the value into the expression you're typing (it doesn't post
            // to the log on its own — you submit the line when ready).
            Button("Use") { session.useBinaryValue() }
                .controlSize(.small)
                .help("Insert this value into the input line")
            Button { session.binaryEditorShown = false } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.secondaryText.color)
            .help("Hide the binary editor (⌥⌘B to show again)")
        }
    }

    @ViewBuilder
    private func widthControl(_ view: BinaryView) -> some View {
        if case .plain = view.kind {
            // A custom segmented control so widths too narrow for the current
            // value are GRAYED OUT (disabled) until ans shrinks; the effective
            // width (view.width) is the highlighted one.
            HStack(spacing: 0) {
                ForEach(BinaryView.editableWidths, id: \.self) { w in
                    let tooSmall = w < view.minimumWidth
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
            // A fixed-width int is locked to its own type/width.
            Text("\(view.signed ? "Int" : "UInt")\(view.width)")
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.accent.color)
        }
    }

    // MARK: Bit grid — MSB→LSB, nibble-grouped, 16 bits per row

    private func bitGrid(_ view: BinaryView) -> some View {
        // Bits MSB→LSB as nibble (4-bit) groups that FLOW across the full width
        // and wrap — so a wide window packs more per row. Widths are multiples
        // of 4, so nibbles divide evenly.
        let bits = view.bits
        let nibbleStarts = Array(stride(from: 0, to: view.width, by: 4))
        return NibbleGrid(spacing: 18, lineSpacing: 6) { // gap between nibbles
            ForEach(nibbleStarts, id: \.self) { start in
                HStack(spacing: 3) { // gap between bits within a nibble
                    ForEach(start..<min(start + 4, view.width), id: \.self) { p in
                        bitCell(set: bits[p], index: view.width - 1 - p)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func bitCell(set: Bool, index: Int) -> some View {
        // No fixed frame — the digit sizes to the (zoomable) font with padding,
        // so it never clips; a monospaced font keeps 0 and 1 the same width.
        Button { session.flipBinaryBit(index) } label: {
            Text(set ? "1" : "0")
                .font(theme.font(scale: 0.95))
                .monospacedDigit()
                .foregroundStyle(set ? theme.accent.color : theme.secondaryText.color.opacity(0.5))
                .padding(.horizontal, 2)
                .padding(.vertical, 1)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("bit \(index)")
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
            message = "Value is over 128 bits — too wide to edit."
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

/// Lays the (equal-width) nibble groups in a grid whose columns-per-row is the
/// largest POWER OF TWO that fits the width. Because the nibble count is itself
/// a power of two, every row is then full — no orphaned single nibble on its
/// own line (the failure mode of a greedy flow).
private struct NibbleGrid: Layout {
    var spacing: CGFloat = 18    // between nibbles in a row
    var lineSpacing: CGFloat = 6 // between rows

    /// Largest power of two ≤ what fits the width, capped at the nibble count.
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
