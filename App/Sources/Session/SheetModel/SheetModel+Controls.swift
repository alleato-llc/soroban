import SorobanEngine
import Foundation

// MARK: Controls (slider drag = preview, release = one undoable rewrite)

extension SheetModel {
    /// Mid-drag: feed the quantized value into resolution WITHOUT touching
    /// the cell's raw. Invalidation is TARGETED — setSliderOverride drops
    /// only this cell and its recorded readers, never the whole workbook —
    /// and renders are gated on the value actually changing (step
    /// quantization caps a whole drag at ~100 updates). No time throttle:
    /// one existed when each tick cost a full recalc, but it held the knob
    /// up to 33ms behind the cursor and dropped trailing updates.
    func previewSlider(at address: CellAddress, info: SliderInfo, fraction: Double) {
        let grid = store.activeSheet.grid
        let value = info.value(atFraction: fraction)
        guard grid.sliderOverrides[address] != value else { return }
        grid.setSliderOverride(value, at: address)
        generation += 1
    }

    /// Release: clear the preview and rewrite the value literal in the raw —
    /// ONE undoable edit (journal, dirty marking, and invalidation all ride
    /// the normal applyEdit path; setCell keeps it targeted for a same-name
    /// 𝑖 redefinition, which is exactly what a slider rewrite is).
    func commitSlider(at address: CellAddress, info: SliderInfo, fraction: Double) {
        let grid = store.activeSheet.grid
        let value = info.value(atFraction: fraction)
        grid.clearSliderOverride(at: address)
        let old = raw(at: address)
        if let rewritten = Slider.rewriting(old, to: value), rewritten != old {
            applyEdit([(address, rewritten)])
        } else {
            generation += 1 // unchanged value: just drop the preview rendering
        }
    }

    /// Checkbox toggles, stepper clicks, and dropdown picks commit
    /// immediately (no preview state): rewrite the storage literal — one
    /// undoable edit through the normal path.
    func commitControl(at address: CellAddress, literal: String) {
        let old = raw(at: address)
        guard let rewritten = Control.rewriting(old, toLiteral: literal),
              rewritten != old else { return }
        applyEdit([(address, rewritten)])
    }
}
