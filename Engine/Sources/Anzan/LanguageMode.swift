/// A presentational input/display *dialect* over the one canonical language.
///
/// A mode changes only which glyphs are parsed (input) and rendered (display) —
/// the canonical AST, evaluation, storage, and the workbook codec are all
/// mode-blind. `5 ^ 3` parses to `bitXor(5,3)` in `.programmer` and to a power
/// node in `.normal`/`.finance`; either way the *stored* form is canonical, and
/// only the surface glyph differs. See `docs/MODES.md`.
public enum LanguageMode: String, Sendable, CaseIterable {
    /// The canonical spelling — today's grammar, unchanged. Cells always use
    /// this; it is the regression oracle (`.normal` must equal the pre-modes
    /// grammar byte-for-byte).
    case normal
    /// `^`=XOR, `&`=AND, `|`=OR, `<<`/`>>`=shift, `%`=modulo, with Python
    /// bitwise precedence. Power renders as `pow(a,b)`; percent as `x * 0.01`.
    case programmer
    /// Finance-oriented display. Grammatically identical to `.normal` today
    /// (the operator set doesn't differ); a home for future finance display
    /// defaults.
    case finance
}
