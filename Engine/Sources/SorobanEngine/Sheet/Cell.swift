import Anzan
/// One cell's content, parsed and statically classified exactly once at
/// commit time. The *dynamic* half of classification — what a formula
/// evaluates to, and whether an ambiguous candidate is a formula or a label —
/// happens per recalculation in `Spreadsheet`, because it depends on the
/// current sheet and variable environment (`12 * rte` is a label until the
/// log defines `rte`).
public struct Cell: Sendable {
    /// Exactly what the user typed (markers included) — what editing shows
    /// and what persistence stores.
    public let raw: String

    enum Content: Sendable {
        /// `=…` — always a formula; carries the parse outcome so even a
        /// malformed explicit formula renders as an error, not text.
        case explicitFormula(Result<Expression, EngineError>)
        /// `"…"` — always text, quotes stripped.
        case explicitText(String)
        /// Doesn't parse — always text.
        case plainText(String)
        /// Parses without an explicit marker; formula vs label is decided at
        /// evaluation time by the auto-detect rules.
        case candidate(Expression)
        /// `tax(x) = x * 2` / `rate = 0.0825` / `data Pt { x: Number, … }`,
        /// typed plain — a SHEET-SCOPED definition. The cell renders λ/𝑖/𝑫;
        /// the name resolves from formulas on the owning sheet and is
        /// immutable from the log.
        case definition(Definition)
        /// `# a note` — a comment-only cell: a free-floating annotation that
        /// holds no value (skipped in ranges, errors on direct reference,
        /// like text). The string is the comment without its `#`.
        case note(String)
    }

    /// What a definition cell defines.
    public struct Definition: Sendable {
        public enum Kind: Sendable {
            case function(parameters: [String], body: Expression)
            case variable(Expression)
            case dataType(fields: [DataField])
        }

        /// As typed; lookup is case-insensitive (one namespace per sheet).
        public let name: String
        public let kind: Kind
        /// The original line — keeps a function's trailing `# doc comment`.
        public let source: String
    }

    let content: Content

    /// nil for blank input — the cell should be removed, not stored.
    public init?(raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return nil }
        self.raw = trimmed

        if trimmed.hasPrefix("=") {
            let formula = trimmed.dropFirst().trimmingCharacters(in: .whitespaces)
            if formula.isEmpty {
                content = .explicitFormula(.failure(.domainError(message: "empty formula")))
            } else {
                content = .explicitFormula(Result { try Parser.parse(formula) }
                    .mapError { $0 as? EngineError ?? .domainError(message: "\($0)") })
            }
            return
        }

        if trimmed.hasPrefix("\"") {
            var text = String(trimmed.dropFirst())
            if text.hasSuffix("\"") { text.removeLast() }
            content = .explicitText(text)
            return
        }

        // A comment-only cell (`# a note`) is an annotation, not a value.
        if let comment = Calculator.standaloneComment(in: trimmed) {
            content = .note(comment)
            return
        }

        if let expression = try? Parser.parse(trimmed) {
            switch expression {
            case .functionDefinition(let name, let parameters, let body):
                // λ cells store untyped parameter names for now; typed dispatch
                // applies to log functions (see the typed-dispatch milestone).
                content = .definition(Definition(
                    name: name,
                    kind: .function(parameters: parameters.map { $0.name }, body: body),
                    source: trimmed))
            case .assignment(let name, let value):
                content = .definition(Definition(
                    name: name, kind: .variable(value), source: trimmed))
            case .dataDefinition(let name, let fields):
                content = .definition(Definition(
                    name: name, kind: .dataType(fields: fields), source: trimmed))
            default:
                content = .candidate(expression)
            }
        } else {
            content = .plainText(trimmed)
        }
    }

    var isDefinition: Bool {
        if case .definition = content { return true }
        return false
    }
}
