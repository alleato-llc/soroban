import SwiftUI
import SorobanEngine
import BigInt

// The bit grids: the plain (no-format) nibble grid and the banded (format) grid
// with captioned, colored field segments, the individual bit cell, the enum/
// value bindings, and the "not bit-editable" explanation.

extension BinaryEditorView {
    // MARK: Plain grid (no format) — nibble groups in even rows

    func plainGrid(_ view: BinaryView) -> some View {
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

    func bandedGrid(_ view: BinaryView, _ layout: [BinaryView.FieldSpec]) -> some View {
        let bits = view.bits
        let fields = view.fields(layout)
        let total = BinaryView.layoutWidth(layout)
        let unused = view.width - total
        let style = bitStyle
        return VStack(alignment: .leading, spacing: 8) {
            // The unused HIGH band is its own full-width, wrapping row so a wide
            // span (e.g. 128 bits) doesn't overflow the editor.
            if unused > 0 {
                unusedBand(low: total, count: unused, bits: bits, view: view, style: style)
            }
            FlowLayout(spacing: 14, lineSpacing: 8) {
                ForEach(Array(fields.enumerated()), id: \.offset) { i, field in
                    if field.reserved || field.unused {
                        gapSegment(field, color: Self.color(named: layout[i].color, position: i),
                                   bits: bits, view: view, style: style)
                    } else {
                        fieldSegment(field, color: Self.color(named: layout[i].color, position: i),
                                     bits: bits, view: view, style: style)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A named field band: caption, its bits (a flag field captions each bit with
    /// its flag letter), and the readout below — the decoded meaning (`r-x`) for
    /// a flag field, or an editable value for a numeric one.
    func fieldSegment(_ field: BinaryView.Field, color: Color,
                      bits: [Bool], view: BinaryView, style: BitStyle) -> some View {
        let lo = max(field.lowBit, 0)
        let hi = min(field.lowBit + field.width, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        return VStack(spacing: 3) {
            Text(field.name).font(theme.font(scale: 0.7)).foregroundStyle(color)
            // A wide field wraps into rows (16 bits each) so it can't overflow.
            VStack(alignment: .leading, spacing: 2) {
                ForEach(Array(rows(of: indices).enumerated()), id: \.offset) { _, row in
                    HStack(spacing: 3) {
                        ForEach(row, id: \.self) { index in
                            // Each bit gets its flag letter directly above it (the
                            // column sizes to whichever is wider, so alignment holds
                            // for multi-char flags too).
                            VStack(spacing: 2) {
                                let pos = (field.lowBit + field.width - 1) - index // 0 = field's high bit
                                if let flags = field.flags, pos >= 0, pos < flags.count {
                                    Text(flags[pos]).font(theme.font(scale: 0.62)).foregroundStyle(color)
                                }
                                bitButton(bits: bits, view: view, index: index, band: color, style: style)
                            }
                        }
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
            } else if let values = field.values {
                // An enum field: pick a labeled value (the bits follow).
                Picker("", selection: enumBinding(field)) {
                    ForEach(Array(values.enumerated()), id: \.offset) { index, label in
                        Text(label).tag(index)
                    }
                }
                .labelsHidden()
                .font(theme.font(scale: 0.72))
                .frame(maxWidth: 120)
                .help("Select a value (\(field.label))")
            } else {
                TextField("", text: fieldBinding(field))
                    .textFieldStyle(.roundedBorder)
                    .font(theme.font(scale: 0.75))
                    .multilineTextAlignment(.center)
                    .frame(width: max(44, CGFloat(field.width) * 14))
            }
        }
    }

    /// A gap field (reserved or unused): a dim, captioned band with no value
    /// control. Reserved bits are locked; unused bits edit once enabled.
    func gapSegment(_ field: BinaryView.Field, color: Color,
                    bits: [Bool], view: BinaryView, style: BitStyle) -> some View {
        let lo = max(field.lowBit, 0)
        let hi = min(field.lowBit + field.width, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        return VStack(spacing: 3) {
            Text(field.name).font(theme.font(scale: 0.7))
                .foregroundStyle(theme.secondaryText.opacity(0.6))
            // A wide gap (e.g. a 47-bit reserve) wraps into rows, not one line.
            VStack(alignment: .leading, spacing: 3) {
                ForEach(Array(rows(of: indices).enumerated()), id: \.offset) { _, row in
                    HStack(spacing: 3) {
                        ForEach(row, id: \.self) { index in
                            gapBitCell(set: bits[view.width - 1 - index], index: index,
                                       reserved: field.reserved, style: style)
                        }
                    }
                }
            }
            .padding(.horizontal, 4).padding(.vertical, 2)
        }
    }

    /// Chunk bit indices into rows (16 per row) so a wide field band wraps
    /// instead of overflowing the editor.
    func rows(of indices: [Int]) -> [[Int]] {
        stride(from: 0, to: indices.count, by: 16).map {
            Array(indices[$0 ..< min($0 + 16, indices.count)])
        }
    }

    /// The dim band of bits above the format's coverage — wraps in nibble groups
    /// (a 128-bit unused span flows onto several lines instead of overflowing).
    func unusedBand(low: Int, count: Int, bits: [Bool],
                    view: BinaryView, style: BitStyle) -> some View {
        let lo = max(low, 0)
        let hi = min(low + count, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        let starts = Array(stride(from: 0, to: indices.count, by: 4))
        return VStack(alignment: .leading, spacing: 3) {
            Text("unused").font(theme.font(scale: 0.7))
                .foregroundStyle(theme.secondaryText.opacity(0.5))
            NibbleGrid(spacing: 14, lineSpacing: 6) {
                ForEach(starts, id: \.self) { s in
                    HStack(spacing: 3) {
                        ForEach(s..<min(s + 4, indices.count), id: \.self) { k in
                            gapBitCell(set: bits[view.width - 1 - indices[k]], index: indices[k],
                                       reserved: false, style: style)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// One dim gap-bit cell. Reserved → locked (no gesture). Unused → a
    /// double-click enables out-of-format editing (one confirm); once enabled a
    /// single click toggles it.
    func gapBitCell(set: Bool, index: Int, reserved: Bool, style: BitStyle) -> some View {
        Text(set ? "1" : "0")
            .font(style.font).monospacedDigit()
            .foregroundStyle(theme.secondaryText.opacity(set ? 0.7 : 0.4))
            .padding(.horizontal, 2).padding(.vertical, 1)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) { if !reserved, !allowUnused { confirmUnused = true } }
            .onTapGesture(count: 1) { if !reserved, allowUnused { host.flipBit(index) } }
            .help(reserved ? "bit \(index) — reserved (locked)"
                  : allowUnused ? "bit \(index) — outside the format"
                  : "Outside the format — double-click to enable editing")
    }

    // MARK: Bit cell

    /// Bundled visual constants so each cell doesn't re-read the theme.
    struct BitStyle { let accent: Color; let dim: Color; let font: Font }
    var bitStyle: BitStyle {
        BitStyle(accent: theme.accent,
                 dim: theme.secondaryText.opacity(0.5),
                 font: theme.font(scale: 0.95))
    }

    func bitButton(bits: [Bool], view: BinaryView, index: Int,
                   band: Color?, style: BitStyle) -> some View {
        BitButton(set: bits[view.width - 1 - index], accent: style.accent, dim: style.dim,
                  band: band, font: style.font, index: index) {
            host.flipBit(index)
        }
        .equatable() // skip unchanged bits on a flip
    }

    /// Live edit of an ENUM field — the selected index IS the field's value.
    /// An out-of-range current value selects nothing (rare; the builder sizes
    /// widths to fit the labels).
    func enumBinding(_ field: BinaryView.Field) -> Binding<Int> {
        Binding(
            get: { Int(exactly: field.value) ?? -1 },
            set: { host.setField(field.name, to: BigInt($0)) })
    }

    /// Live edit of a field's value — shown in the field's base (`0x…` for hex)
    /// and parsed in that base, but a `0x`/`0o`/`0b` prefix always wins, so a hex
    /// field accepts `1b` or `0x1b`. Rewrites its bit range (clamped engine-side).
    func fieldBinding(_ field: BinaryView.Field) -> Binding<String> {
        Binding(
            get: { field.valueText },
            set: { text in
                let trimmed = text.trimmingCharacters(in: .whitespaces)
                if trimmed.isEmpty {
                    host.setField(field.name, to: BigInt(0))
                } else if let value = BinaryView.parse(trimmed, base: field.base ?? 10) {
                    host.setField(field.name, to: value)
                }
            })
    }

    // MARK: Disabled — explain why this value isn't bit-editable

    func unavailable(_ reason: BinaryView.Unavailable) -> some View {
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
        .foregroundStyle(theme.secondaryText)
    }
}
