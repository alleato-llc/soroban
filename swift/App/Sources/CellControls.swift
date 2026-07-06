import SwiftUI
import SorobanEngine

/// One slider cell's content: track + knob + formatted value. Stateless on
/// purpose (the per-cell @State ban): the knob position comes from the model
/// (mid-drag via Spreadsheet.sliderOverrides), and the drag gesture routes
/// through SheetModel.previewSlider/commitSlider — preview while dragging,
/// ONE undoable raw rewrite on release, like column resizing.
struct SliderCellContent: View {
    let address: CellAddress
    let info: SliderInfo
    let valueText: String
    /// The longest label this slider's format can produce. Reserved as the
    /// label's width so the TRACK never resizes mid-drag: a value text that
    /// changes length ("0.08" → "0.1") would re-layout the GeometryReader,
    /// and its width arrives one pass late — the knob lands a few pixels
    /// off, then corrects. (Themes are monospace-only, so widest text =
    /// widest width.)
    let widestValueText: String
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        HStack(spacing: 6) {
            GeometryReader { geometry in
                let width = geometry.size.width
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(theme.secondaryText.color.opacity(0.3))
                        .frame(height: 3)
                    Capsule()
                        .fill(theme.accent.color)
                        .frame(width: max(CGFloat(info.fraction) * width, 0), height: 3)
                    Circle()
                        .fill(theme.accent.color)
                        .frame(width: 9, height: 9)
                        .offset(x: CGFloat(info.fraction) * max(width - 9, 0))
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                // Child gestures outrank the cell's tap/double-tap, so the
                // track drags instead of selecting. minimumDistance 0 makes
                // a plain click jump the knob.
                .gesture(DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        guard session.sheet.editing == nil else { return } // point mode owns clicks
                        session.sheet.previewSlider(at: address, info: info,
                                                    fraction: drag.location.x / max(width - 9, 1))
                    }
                    .onEnded { drag in
                        if session.sheet.editing != nil {
                            // An open editor means this click was a reference
                            // insertion, not a drag.
                            session.sheet.handleCellClick(
                                address, isShiftDown: isShiftKeyDown())
                            return
                        }
                        session.sheet.commitSlider(at: address, info: info,
                                                   fraction: drag.location.x / max(width - 9, 1))
                    })
            }
            Text(widestValueText)
                .font(theme.font(scale: 0.85))
                .lineLimit(1)
                .fixedSize()
                .hidden() // width template — keeps the track width constant
                .overlay(alignment: .trailing) {
                    Text(valueText)
                        .font(theme.font(scale: 0.85))
                        .foregroundStyle(theme.resultText.color)
                        .lineLimit(1)
                        .fixedSize()
                }
        }
        .padding(.vertical, 2)
    }
}

/// Stepper cell: − value + . Clicks commit immediately (one undoable
/// rewrite each); stateless like every cell control.
struct StepperCellContent: View {
    let address: CellAddress
    let info: SliderInfo
    let valueText: String
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        HStack(spacing: 5) {
            button("minus.circle", enabled: info.value > info.minimum, delta: -1)
            Text(valueText)
                .font(theme.font(scale: 0.9))
                .foregroundStyle(theme.resultText.color)
                .lineLimit(1)
                .frame(maxWidth: .infinity)
            button("plus.circle", enabled: info.value < info.maximum, delta: 1)
        }
    }

    private func button(_ symbol: String, enabled: Bool, delta: Int) -> some View {
        Image(systemName: symbol)
            .foregroundStyle(enabled ? theme.accent.color
                                     : theme.secondaryText.color.opacity(0.4))
            .contentShape(Rectangle())
            .onTapGesture {
                let sheet = session.sheet
                guard sheet.editing == nil else { // point mode owns clicks
                    sheet.handleCellClick(address,
                                          isShiftDown: isShiftKeyDown())
                    return
                }
                guard enabled else { return }
                let stepped = info.value + (delta > 0 ? info.step : -info.step)
                let clamped = min(max(stepped, info.minimum), info.maximum)
                sheet.commitControl(at: address, literal: clamped.description)
            }
    }
}

/// Checkbox cell: clicking the box flips the stored true/false literal.
struct CheckboxCellContent: View {
    let address: CellAddress
    let info: CheckboxInfo
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        Image(systemName: info.isOn ? "checkmark.square.fill" : "square")
            .foregroundStyle(info.isOn ? theme.accent.color : theme.secondaryText.color)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .onTapGesture {
                let sheet = session.sheet
                guard sheet.editing == nil else { // point mode owns clicks
                    sheet.handleCellClick(address,
                                          isShiftDown: isShiftKeyDown())
                    return
                }
                sheet.commitControl(at: address, literal: info.isOn ? "false" : "true")
            }
    }
}

/// Dropdown cell: a borderless menu of the options; choosing rewrites the
/// selected literal (strings stay quoted via Value's canonical rendering).
struct DropdownCellContent: View {
    let address: CellAddress
    let info: DropdownInfo
    let theme: Theme

    @Environment(CalculatorSession.self) private var session

    var body: some View {
        Menu {
            ForEach(Array(info.options.enumerated()), id: \.offset) { _, option in
                Toggle(option.displayText, isOn: Binding(
                    get: { option == info.value },
                    set: { _ in
                        session.sheet.commitControl(at: address,
                                                    literal: option.description)
                    }))
            }
        } label: {
            HStack(spacing: 4) {
                Text(info.value.displayText)
                    .font(theme.font(scale: 0.9))
                    .foregroundStyle(theme.resultText.color)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: theme.fontSize * 0.55))
                    .foregroundStyle(theme.secondaryText.color)
            }
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
    }
}
