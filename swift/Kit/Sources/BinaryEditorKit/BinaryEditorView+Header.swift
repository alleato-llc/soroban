import SwiftUI
import SorobanEngine
import BigInt

// The header row (decimal/hex readout, width control, Use/reset/close actions,
// the Format ▾ menu) and the progressive-disclosure save & rename rows.

extension BinaryEditorView {
    // MARK: Header — value, hex, width, actions, Format menu

    func header(_ view: BinaryView) -> some View {
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

    var formatMenu: some View {
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
    func widthControl(_ view: BinaryView) -> some View {
        if case .plain = view.kind {
            // With a format active the width is FIXED to the format's size (a
            // format defines how many bits it covers — IPv4 is 32, MAC 48, IPv6
            // 128). Otherwise widths too narrow for the value are grayed out.
            let locked: Int? = host.activeLayout.map { layout -> Int in
                let total = BinaryView.layoutWidth(layout)
                return BinaryView.editableWidths.first { $0 >= total } ?? BinaryView.maxWidth
            }
            let minWidth = view.minimumWidth
            let active = locked ?? view.width
            HStack(spacing: 0) {
                ForEach(BinaryView.editableWidths, id: \.self) { w in
                    let enabled = locked == nil ? w >= minWidth : w == locked
                    Button("\(w)") { if locked == nil { host.width = w } }
                        .buttonStyle(.plain)
                        .font(theme.font(scale: 0.7))
                        .foregroundStyle(
                            !enabled ? theme.secondaryText.opacity(0.3)
                            : w == active ? theme.accent : theme.secondaryText)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(w == active ? theme.accent.opacity(0.18) : .clear)
                        .disabled(!enabled)
                        .help(locked != nil ? (w == active ? "\(w)-bit — fixed by the active format" : "Width is fixed by the active format")
                              : enabled ? "\(w)-bit register" : "Too narrow for this value")
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

    var saveRow: some View {
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

    func saveCurrent() {
        host.saveFormat(host.activeLayout ?? [], named: saveName)
        saveName = ""
        showingSave = false
    }

    var renameRow: some View {
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

    func commitRename() {
        if let old = renameTarget { host.renameFormat(old, to: renameText) }
        renameTarget = nil
    }
}
