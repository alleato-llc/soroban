import SwiftUI
import SorobanEngine
import BigInt

/// The seam between the reusable `BinaryEditorView` and whatever app embeds it.
/// The view talks only to this protocol; each host (the calculator, the
/// standalone Tama app) supplies a concrete `@Observable` conformer. The
/// host-neutral LOGIC already lives in the engine (`BinaryView`,
/// `BinaryView.FormatBuilder`); this is the thin host-specific surface — the
/// value source, format storage, and the "emit a value" / theme decisions.
///
/// (Moves into the shared `BinaryEditorKit` package in a later step; kept in the
/// app for the decoupling pass so the calculator keeps building throughout.)
@MainActor public protocol BinaryEditorHost: AnyObject, Observable {
    // The value being edited — the host owns the staged draft, so flips/edits
    // mutate it and `binaryView` reflects them. A `.failure` disables the grid.
    var binaryView: Result<BinaryView, BinaryView.Unavailable> { get }
    var width: Int { get set }
    var hasEdits: Bool { get }
    func flipBit(_ index: Int)
    func setField(_ name: String, to value: BigInt)
    func cancelEdits()

    // Emit a value outward — the host decides what that means (the calculator
    // inserts into its input line; Tama copies to the pasteboard).
    func useValue()                 // the "Use" button — the staged value
    func insert(_ literal: String)  // double-click a decimal/hex readout

    // Formats. Presets ship with the host; saved formats persist however the
    // host wants (a workbook log variable, a JSON file, …).
    var presets: [(name: String, format: Value)] { get }
    var savedFormats: [(name: String, format: Value)] { get }
    var activeFormat: Value? { get }
    var activeLayout: [BinaryView.FieldSpec]? { get }
    var activeFormatName: String? { get }
    func applyFormat(_ format: Value?)
    func applyBuiltFormat(_ layout: [BinaryView.FieldSpec])
    func saveFormat(_ layout: [BinaryView.FieldSpec], named name: String)

    // Whether saved formats are a curated, editable store (rename/delete shown
    // in the menu). A host whose "saved formats" are workbook variables it
    // manages elsewhere (the calculator) leaves this false; a host with its own
    // format file (Tama) sets it true and implements the two below.
    var canManageSavedFormats: Bool { get }
    func renameFormat(_ oldName: String, to newName: String)
    func deleteFormat(_ name: String)

    // Environment.
    var theme: BinaryEditorTheme { get }
    func dismiss() // hide the editor (a no-op where the editor IS the window)
}

/// The colors and fonts the editor needs — a minimal seam so the kit doesn't
/// depend on any app's full theme system. Each app maps its own theme onto it.
@MainActor public protocol BinaryEditorTheme {
    func font(scale: Double) -> Font
    var accent: Color { get }
    var resultText: Color { get }
    var secondaryText: Color { get }
    var inputBackground: Color { get }
}

public extension BinaryEditorHost {
    // Default: saved formats aren't editor-managed (the calculator's case).
    var canManageSavedFormats: Bool { false }
    func renameFormat(_ oldName: String, to newName: String) {}
    func deleteFormat(_ name: String) {}
}

public extension BinaryEditorTheme {
    /// Base size — the common `font()` call site.
    func font() -> Font { font(scale: 1) }
}
