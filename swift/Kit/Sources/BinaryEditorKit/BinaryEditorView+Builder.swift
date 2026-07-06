import SwiftUI
import SorobanEngine
import BigInt

// The visual format builder (build mode): bindings over the engine's
// BinaryView.FormatBuilder — claim open bits into a group, detail it
// (name/kind/labels/color), add it, live-preview the register strip, then
// save or apply.

extension BinaryEditorView {
    // MARK: Visual format builder (build mode) — bindings over BinaryView.FormatBuilder

    /// Enter build mode, seeding the builder from the active format so an existing
    /// one can be tweaked. Closes the other disclosure rows.
    func startBuilding(seedFromActive: Bool) {
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
    func buildMode(_ view: BinaryView) -> some View {
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
    func builderStrip(_ view: BinaryView) -> some View {
        let free = builder.freeBits(registerWidth: view.width)
        // Claim up to ALL the free bits in one group — a reserved/unused gap can
        // span most of a wide register (e.g. 47 of 48). The cells wrap.
        let shown = free
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

    func committedBand(_ field: BinaryView.FormatBuilder.Field, position: Int) -> some View {
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
    var colorSwatches: some View {
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

    func openCell(index j: Int) -> some View {
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

    var pendingDetail: some View {
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
}
