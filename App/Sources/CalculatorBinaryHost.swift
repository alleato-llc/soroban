import SwiftUI
import SorobanEngine
import BigInt
import BinaryEditorKit

/// The calculator's `BinaryEditorHost`: a thin adapter over `CalculatorSession`
/// (and the active theme). Every member is a one-line forward — the proof that
/// the editor's coupling to the calculator is exactly this surface. The
/// standalone Tama app supplies a different host with the same shape.
@Observable @MainActor final class CalculatorBinaryHost: BinaryEditorHost {
    private let session: CalculatorSession
    private let themeManager: ThemeManager

    init(session: CalculatorSession, themeManager: ThemeManager) {
        self.session = session
        self.themeManager = themeManager
    }

    var binaryView: Result<BinaryView, BinaryView.Unavailable> { session.binaryView }
    var width: Int {
        get { session.binaryWidth }
        set { session.binaryWidth = newValue }
    }
    var hasEdits: Bool { session.binaryHasEdits }
    func flipBit(_ index: Int) { session.flipBinaryBit(index) }
    func setField(_ name: String, to value: BigInt) { session.setBinaryField(name, to: value) }
    func cancelEdits() { session.cancelBinaryEdits() }

    func useValue() { session.useBinaryValue() }        // → the input line
    func insert(_ literal: String) { session.insert(value: literal) }

    var presets: [(name: String, format: Value)] { CalculatorSession.binaryFormatPresets }
    var savedFormats: [(name: String, format: Value)] { session.savedFormats }
    var activeFormat: Value? { session.activeFormat }
    var activeLayout: [BinaryView.FieldSpec]? { session.activeLayout }
    var activeFormatName: String? { session.activeFormatName }
    func applyFormat(_ format: Value?) { session.applyFormat(format) }
    func applyBuiltFormat(_ layout: [BinaryView.FieldSpec]) { session.applyBuiltFormat(layout) }
    func saveFormat(_ layout: [BinaryView.FieldSpec], named name: String) {
        session.saveBuiltFormat(layout, named: name)    // → a workbook log variable
    }

    var theme: BinaryEditorTheme { CalculatorEditorTheme(theme: themeManager.current) }
    func dismiss() { session.binaryEditorShown = false }
}

/// Maps the app's `Theme` onto the editor's minimal theme seam.
struct CalculatorEditorTheme: BinaryEditorTheme {
    let theme: Theme
    func font(scale: Double) -> Font { theme.font(scale: scale) }
    var accent: Color { theme.accent.color }
    var resultText: Color { theme.resultText.color }
    var secondaryText: Color { theme.secondaryText.color }
    var inputBackground: Color { theme.inputBackground.color }
}
