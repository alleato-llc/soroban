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
public struct BinaryEditorView<Host: BinaryEditorHost>: View {
    let host: Host
    private var theme: BinaryEditorTheme { host.theme }

    public init(host: Host) { self.host = host }

    @State private var saveName = ""
    @State private var showingSave = false

    // Out-of-format ("unused") bits are locked until the user enables editing
    // with a deliberate double-click (confirmed once per session).
    @State private var allowUnused = false
    @State private var confirmUnused = false

    // Renaming a saved format (hosts that manage their own format store).
    @State private var renameTarget: String?
    @State private var renameText = ""

    // Visual format builder (build mode): drag/click free bits to carve a group,
    // detail it (name/kind/labels), add it; live-preview as you go; save. The
    // logic lives in the engine's BinaryView.FormatBuilder; this view is bindings.
    @State private var building = false
    // Placeholder palette; startBuilding() rebuilds it with the real palette
    // (a generic type's static can't seed a @State default).
    @State private var builder = BinaryView.FormatBuilder(palette: [])
    @State private var builderName = ""

    /// Distinct band colors cycled per field (system colors adapt to light/dark),
    /// paired with the NAME persisted in a field's `color`. Computed (a generic
    /// type can't hold a static STORED property).
    private static var fieldColors: [(name: String, color: Color)] {
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
    private static func color(named name: String?, position: Int) -> Color {
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
    private func editor(_ view: BinaryView) -> some View {
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

    // MARK: Header — value, hex, width, actions, Format menu

    private func header(_ view: BinaryView) -> some View {
        let decimal = view.value.displayDescription
        let hex = "0x" + String(view.pattern, radix: 16, uppercase: true)
        return HStack(spacing: 10) {
            Text(decimal)
                .font(theme.font())
                .foregroundStyle(host.hasEdits ? theme.accent : theme.resultText)
                .onTapGesture(count: 2) { host.insert(decimal) }
                .help("Double-click to insert the decimal value into the expression")
            Text(hex)
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.secondaryText)
                .onTapGesture(count: 2) { host.insert(hex) }
                .help("Double-click to insert the hex value into the expression")
            Spacer()
            widthControl(view)
            if host.hasEdits {
                Button { host.cancelEdits() } label: {
                    Image(systemName: "arrow.uturn.backward")
                }
                .buttonStyle(.plain)
                .foregroundStyle(theme.secondaryText)
                .help("Reset to ans")
            }
            Button("Use") { host.useValue() }
                .controlSize(.small)
                .help("Insert this value into the input line")
            formatMenu
            Button { host.dismiss() } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.secondaryText)
            .help("Hide the binary editor (⌥⌘B to show again)")
        }
    }

    private var formatMenu: some View {
        Menu {
            Button("None") { host.applyFormat(nil) }
            Section("Presets") {
                ForEach(host.presets, id: \.name) { preset in
                    Button(preset.name) { host.applyFormat(preset.format) }
                }
            }
            if !host.savedFormats.isEmpty {
                Section("Saved") {
                    ForEach(host.savedFormats, id: \.name) { saved in
                        if host.canManageSavedFormats {
                            Menu(saved.name) {
                                Button("Apply") { host.applyFormat(saved.format) }
                                Button("Rename…") {
                                    renameTarget = saved.name; renameText = saved.name; showingSave = false
                                }
                                Button("Delete", role: .destructive) { host.deleteFormat(saved.name) }
                            }
                        } else {
                            Button(saved.name) { host.applyFormat(saved.format) }
                        }
                    }
                }
            }
            Divider()
            Button("Build new…") { startBuilding(seedFromActive: false) }
            Button("Edit current…") { startBuilding(seedFromActive: true) }
                .disabled(host.activeFormat == nil)
            Button("Save current…") { showingSave = true }
                .disabled(host.activeFormat == nil)
        } label: {
            Label(host.activeFormatName ?? "Format", systemImage: "rectangle.split.3x1")
                .font(theme.font(scale: 0.8))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .disabled(building) // can't switch formats mid-build
    }

    @ViewBuilder
    private func widthControl(_ view: BinaryView) -> some View {
        if case .plain = view.kind {
            // Widths too narrow for the value — OR for the active format's total
            // — are grayed out; the effective width is highlighted.
            let formatBits = host.activeLayout.map { BinaryView.layoutWidth($0) } ?? 0
            let minWidth = max(view.minimumWidth,
                               BinaryView.editableWidths.first { $0 >= formatBits } ?? 0)
            HStack(spacing: 0) {
                ForEach(BinaryView.editableWidths, id: \.self) { w in
                    let tooSmall = w < minWidth
                    Button("\(w)") { host.width = w }
                        .buttonStyle(.plain)
                        .font(theme.font(scale: 0.7))
                        .foregroundStyle(
                            tooSmall ? theme.secondaryText.opacity(0.3)
                            : w == view.width ? theme.accent : theme.secondaryText)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(w == view.width ? theme.accent.opacity(0.18) : .clear)
                        .disabled(tooSmall)
                        .help(tooSmall ? "Too narrow for this value" : "\(w)-bit register")
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 4)
                .stroke(theme.secondaryText.opacity(0.25)))
            .clipShape(RoundedRectangle(cornerRadius: 4))
        } else {
            Text("\(view.signed ? "Int" : "UInt")\(view.width)")
                .font(theme.font(scale: 0.8))
                .foregroundStyle(theme.accent)
        }
    }

    // MARK: Progressive-disclosure rows (custom spec / save)

    private var saveRow: some View {
        HStack(spacing: 8) {
            Text("save as").font(theme.font(scale: 0.8)).foregroundStyle(theme.secondaryText)
            TextField("name", text: $saveName)
                .textFieldStyle(.roundedBorder)
                .font(theme.font(scale: 0.8))
                .frame(width: 140)
                .onSubmit { saveCurrent() }
            Button("Save") { saveCurrent() }
                .controlSize(.small)
                .disabled(saveName.trimmingCharacters(in: .whitespaces).isEmpty)
            Button { showingSave = false } label: { Image(systemName: "xmark") }
                .buttonStyle(.plain).foregroundStyle(theme.secondaryText)
        }
    }

    private func saveCurrent() {
        host.saveFormat(host.activeLayout ?? [], named: saveName)
        saveName = ""
        showingSave = false
    }

    private var renameRow: some View {
        HStack(spacing: 8) {
            Text("rename to").font(theme.font(scale: 0.8)).foregroundStyle(theme.secondaryText)
            TextField("name", text: $renameText)
                .textFieldStyle(.roundedBorder)
                .font(theme.font(scale: 0.8))
                .frame(width: 140)
                .onSubmit { commitRename() }
            Button("Rename") { commitRename() }
                .controlSize(.small)
                .disabled(renameText.trimmingCharacters(in: .whitespaces).isEmpty)
            Button { renameTarget = nil } label: { Image(systemName: "xmark") }
                .buttonStyle(.plain).foregroundStyle(theme.secondaryText)
        }
    }

    private func commitRename() {
        if let old = renameTarget { host.renameFormat(old, to: renameText) }
        renameTarget = nil
    }

    // MARK: Visual format builder (build mode) — bindings over BinaryView.FormatBuilder

    /// Enter build mode, seeding the builder from the active format so an existing
    /// one can be tweaked. Closes the other disclosure rows.
    private func startBuilding(seedFromActive: Bool) {
        var seeded = BinaryView.FormatBuilder(palette: Self.fieldColors.map(\.name))
        if seedFromActive {
            seeded.seed(from: host.activeLayout ?? [])
            let name = host.activeFormatName
            builderName = (name == "Custom" || name == nil) ? "" : name!
        } else {
            builderName = "" // a fresh format
        }
        builder = seeded
        showingSave = false
        building = true
    }

    @ViewBuilder
    private func buildMode(_ view: BinaryView) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Click the open bits to claim a group, then name it below.")
                .font(theme.font(scale: 0.7)).foregroundStyle(theme.secondaryText)
            builderStrip(view)
            if builder.pendingWidth > 0 { pendingDetail }
            HStack(spacing: 8) {
                Text("format").font(theme.font(scale: 0.8)).foregroundStyle(theme.secondaryText)
                TextField("name", text: $builderName)
                    .textFieldStyle(.roundedBorder).font(theme.font(scale: 0.8)).frame(width: 130)
                Button("Save") {
                    host.saveFormat(builder.layout, named: builderName); building = false
                }
                .controlSize(.small)
                .disabled(builder.isEmpty || builderName.trimmingCharacters(in: .whitespaces).isEmpty)
                Button("Apply") {
                    host.applyBuiltFormat(builder.layout); building = false
                }
                .controlSize(.small).disabled(builder.isEmpty)
                Spacer()
                Button("Cancel") { building = false }.controlSize(.small)
            }
        }
    }

    /// The register strip: committed field bands (high→low, left→right) followed
    /// by the open bits as clickable cells. Clicking the j-th open cell claims a
    /// j-bit pending group; the claimed cells highlight.
    private func builderStrip(_ view: BinaryView) -> some View {
        let free = builder.freeBits(registerWidth: view.width)
        // One group claims up to 32 bits at a time (plenty for any real subfield);
        // more open bits become further groups, so the whole register is reachable.
        let shown = min(free, 32)
        return FlowLayout(spacing: 8, lineSpacing: 8) {
            ForEach(Array(builder.fields.enumerated()), id: \.element.id) { i, field in
                committedBand(field, position: i)
            }
            ForEach(1...max(shown, 1), id: \.self) { j in
                if j <= shown { openCell(index: j) }
            }
            if free == 0 && builder.isEmpty {
                Text("widen the register to add bits")
                    .font(theme.font(scale: 0.7)).foregroundStyle(theme.secondaryText)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func committedBand(_ field: BinaryView.FormatBuilder.Field, position: Int) -> some View {
        let color = Self.color(named: field.colorName, position: position)
        return VStack(spacing: 2) {
            Text(field.name).font(theme.font(scale: 0.68)).foregroundStyle(color).lineLimit(1)
            HStack(spacing: 4) {
                // The swatch is a menu: recolor this field.
                Menu {
                    ForEach(Self.fieldColors, id: \.name) { entry in
                        Button { builder.recolor(field.id, to: entry.name) } label: {
                            Label(entry.name.capitalized, systemImage:
                                field.colorName == entry.name ? "checkmark.circle.fill" : "circle.fill")
                        }
                    }
                } label: {
                    Circle().fill(color).frame(width: 8, height: 8)
                }
                .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                Text("\(field.width)b · \(field.kind.rawValue.lowercased())")
                    .font(theme.font(scale: 0.6)).foregroundStyle(theme.secondaryText)
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 5)
        .background(color.opacity(0.15))
        .overlay(RoundedRectangle(cornerRadius: 5).stroke(color.opacity(0.5)))
        .overlay(alignment: .topTrailing) {
            Button { builder.remove(field.id) } label: {
                Image(systemName: "xmark.circle.fill").font(.system(size: 11))
            }
            .buttonStyle(.plain).foregroundStyle(theme.secondaryText).offset(x: 4, y: -4)
        }
    }

    /// A row of selectable color swatches for the pending field.
    private var colorSwatches: some View {
        HStack(spacing: 4) {
            ForEach(Self.fieldColors, id: \.name) { entry in
                Button { builder.draftColor = entry.name } label: {
                    Circle().fill(entry.color)
                        .frame(width: 14, height: 14)
                        .overlay(Circle().stroke(theme.resultText,
                                                 lineWidth: builder.draftColor == entry.name ? 2 : 0))
                }
                .buttonStyle(.plain)
                .help(entry.name.capitalized)
            }
        }
    }

    private func openCell(index j: Int) -> some View {
        let claimed = j <= builder.pendingWidth
        let accent = theme.accent
        return Button {
            builder.claim(j)
        } label: {
            RoundedRectangle(cornerRadius: 3)
                .fill(claimed ? accent.opacity(0.3) : theme.inputBackground)
                .frame(width: 18, height: 26)
                .overlay(RoundedRectangle(cornerRadius: 3)
                    .stroke(claimed ? accent : theme.secondaryText.opacity(0.35)))
        }
        .buttonStyle(.plain)
        .help("Claim \(j) bit\(j == 1 ? "" : "s") for the next group")
    }

    private var pendingDetail: some View {
        HStack(spacing: 8) {
            Text("\(builder.pendingWidth)b").font(theme.font(scale: 0.78)).foregroundStyle(theme.accent)
            TextField("name", text: $builder.draftName)
                .textFieldStyle(.roundedBorder).font(theme.font(scale: 0.8)).frame(width: 90)
            Picker("", selection: $builder.draftKind) {
                ForEach(BinaryView.FormatBuilder.FieldKind.allCases) { Text($0.rawValue).tag($0) }
            }
            .labelsHidden().fixedSize()
            switch builder.draftKind {
            case .numeric:
                Picker("", selection: $builder.draftBase) {
                    Text("dec").tag(10)
                    Text("hex").tag(16)
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize()
                .help("How this field's value reads — decimal or hex (0x…)")
            case .flags:
                TextField("r, w, x", text: $builder.draftLabels)
                    .textFieldStyle(.roundedBorder).font(theme.font(scale: 0.8))
                    .help("One name per bit, high→low (extra are dropped, missing become ?)")
            case .enumeration:
                TextField("idle, run, halt, max", text: $builder.draftLabels)
                    .textFieldStyle(.roundedBorder).font(theme.font(scale: 0.8))
                    .help("A label per value, starting at 0")
            case .reserved:
                Text("locked, must-be-zero")
                    .font(theme.font(scale: 0.7)).foregroundStyle(theme.secondaryText)
            case .unused:
                Text("don't-care, editable")
                    .font(theme.font(scale: 0.7)).foregroundStyle(theme.secondaryText)
            }
            colorSwatches
            Button("Add field") { builder.addField() }
                .controlSize(.small)
                .disabled(!builder.canAddField)
        }
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
        return VStack(alignment: .leading, spacing: 8) {
            // The unused HIGH band is its own full-width, wrapping row so a wide
            // span (e.g. 128 bits) doesn't overflow the editor.
            if unused > 0 {
                unusedBand(low: total, count: unused, bits: bits, view: view, style: style)
            }
            FlowLayout(spacing: 14, lineSpacing: 8) {
                ForEach(Array(fields.enumerated()), id: \.element.name) { i, field in
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
    private func gapSegment(_ field: BinaryView.Field, color: Color,
                            bits: [Bool], view: BinaryView, style: BitStyle) -> some View {
        let lo = max(field.lowBit, 0)
        let hi = min(field.lowBit + field.width, view.width)
        let indices = lo < hi ? Array((lo..<hi).reversed()) : []
        return VStack(spacing: 3) {
            Text(field.name).font(theme.font(scale: 0.7))
                .foregroundStyle(theme.secondaryText.opacity(0.6))
            HStack(spacing: 3) {
                ForEach(indices, id: \.self) { index in
                    gapBitCell(set: bits[view.width - 1 - index], index: index,
                               reserved: field.reserved, style: style)
                }
            }
            .padding(.horizontal, 4).padding(.vertical, 2)
        }
    }

    /// The dim band of bits above the format's coverage — wraps in nibble groups
    /// (a 128-bit unused span flows onto several lines instead of overflowing).
    private func unusedBand(low: Int, count: Int, bits: [Bool],
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
    private func gapBitCell(set: Bool, index: Int, reserved: Bool, style: BitStyle) -> some View {
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
    private struct BitStyle { let accent: Color; let dim: Color; let font: Font }
    private var bitStyle: BitStyle {
        BitStyle(accent: theme.accent,
                 dim: theme.secondaryText.opacity(0.5),
                 font: theme.font(scale: 0.95))
    }

    private func bitButton(bits: [Bool], view: BinaryView, index: Int,
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
    private func enumBinding(_ field: BinaryView.Field) -> Binding<Int> {
        Binding(
            get: { Int(exactly: field.value) ?? -1 },
            set: { host.setField(field.name, to: BigInt($0)) })
    }

    /// Live edit of a field's value — shown in the field's base (`0x…` for hex)
    /// and parsed in that base, but a `0x`/`0o`/`0b` prefix always wins, so a hex
    /// field accepts `1b` or `0x1b`. Rewrites its bit range (clamped engine-side).
    private func fieldBinding(_ field: BinaryView.Field) -> Binding<String> {
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
        .foregroundStyle(theme.secondaryText)
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
