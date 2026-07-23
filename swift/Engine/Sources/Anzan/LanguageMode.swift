/// A presentational input/display *dialect* over the one canonical language.
///
/// A mode changes only which glyphs are parsed (input) and rendered (display) —
/// the canonical AST, evaluation, storage, and the workbook codec are all
/// mode-blind. `5 ^ 3` parses to `bitXor(5,3)` in `.programmer` and to a power
/// node in `.normal`/`.scientific`; either way the *stored* form is canonical,
/// and only the surface glyph differs. See `docs/MODES.md`.
public enum LanguageMode: String, Sendable, CaseIterable {
    /// The canonical spelling — today's grammar, unchanged. Cells always use
    /// this; it is the regression oracle (`.normal` must equal the pre-modes
    /// grammar byte-for-byte).
    case normal
    /// `^`=XOR, `&`=AND, `|`=OR, `<<`/`>>`=shift, `%`=modulo, with Python
    /// bitwise precedence. Power renders as `pow(a,b)`; percent as `x * 0.01`.
    case programmer
    /// Grammatically identical to `.normal`; changes only how a plain NUMERIC
    /// result ECHOES — scientific notation (`2.46912e5`), or the engineering
    /// variant (exponent snapped to a multiple of 3) via `ScientificStyle`.
    /// Value-carried display (Money, grouping) still wins.
    case scientific
}

/// The scientific-mode echo variant: plain SCI (`2.46912e5`) or ENG
/// (`246.912e3` — the exponent snapped to a multiple of 3). A display style,
/// not a mode: one `.scientific` dialect, two notations.
public enum ScientificStyle: String, Sendable, CaseIterable {
    case sci
    case eng
}
