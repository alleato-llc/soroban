import SorobanEngine
import Foundation

/// One line of the calculation log. Codable so the tape persists across
/// launches (the global log file); `id` is omitted from coding and
/// regenerated on load — it's only SwiftUI's row identity.
struct HistoryEntry: Identifiable, Codable {
    enum Outcome: Codable {
        case value(String)
        /// Informational output (man()/help() docs) — rendered as a plain
        /// multi-line block, no "= " prefix, never recallable as a value.
        case info(String)
        case error(message: String, position: Int?)
        /// A comment-only line (`# note`) — a first-class note, rendered
        /// dim, never a value or an error.
        case comment(String)
        /// A dialect-switch marker (e.g. "Programmer mode") — a dim divider in
        /// the tape recording when the input mode changed; not a value, not
        /// recallable. See docs/MODES.md.
        case mode(String)
    }

    let id = UUID()
    let expression: String
    let outcome: Outcome
    /// Display-only suffix after a value — the programmer hex echo
    /// ("(0xC3)"). Never part of Insert/Copy: recallValue must stay a
    /// re-parseable expression.
    var annotation: String? = nil
    /// A trailing `# comment` on a calculation — shown dimmed after the
    /// result, kept out of Insert/Copy like the annotation.
    var note: String? = nil
    /// The canonical, re-parseable form to recall/copy when it differs from the
    /// displayed text — a fixed-width int / decimal shows its plain number
    /// (`343353`) but recalls its typed constructor (`Int32(343353)`) so the
    /// type survives. nil when the display IS the canonical form.
    var recallOverride: String? = nil

    private enum CodingKeys: String, CodingKey {
        case expression, outcome, annotation, note, recallOverride // id regenerates on load
    }

    /// The text to insert when the result is clicked.
    var recallValue: String? {
        if case .value(let text) = outcome { return recallOverride ?? text }
        return nil
    }
}
