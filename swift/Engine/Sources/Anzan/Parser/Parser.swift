/// Pratt (precedence-climbing) parser.
///
/// Grammar, low to high precedence:
///   assignment:  IDENT = expr          (only at top level)
///   additive:    + -
///   term:        * / %  and implicit multiplication (`2(3+4)`, `2x`, `(a)(b)`)
///   unary:       -x
///   power:       ^ (right-associative, binds tighter than unary: -2^2 == -4)
///   primary:     number | ident | ident(args) | (expr)
///
/// The productions are split across `Parser+Definitions.swift` (statements,
/// data/namespace/function definitions), `Parser+Expressions.swift` (the
/// operator-precedence ladder), and `Parser+Primary.swift` (primaries,
/// literals, reductions, references) — this file holds the core scanning
/// state and the top-level `statement` dispatch.
package struct Parser {
    let tokens: [Token]
    /// The dialect to parse under — affects only overloaded glyphs (`^ % & | << >>`).
    /// `.normal` (the default) is today's grammar exactly. See `docs/MODES.md`.
    let mode: LanguageMode
    var index = 0

    private init(tokens: [Token], mode: LanguageMode) {
        self.tokens = tokens
        self.mode = mode
    }

    package static func parse(_ source: String, mode: LanguageMode = .normal) throws(EngineError) -> Expression {
        var parser = Parser(tokens: try Lexer.tokenize(source, mode: mode), mode: mode)
        let expr = try parser.statement()
        try parser.expectEnd()
        return expr
    }

    var current: Token { tokens[index] }

    mutating func advance() -> Token {
        defer { if index < tokens.count - 1 { index += 1 } }
        return current
    }

    private mutating func expectEnd() throws(EngineError) {
        guard case .end = current.kind else {
            throw EngineError.parseError(message: "unexpected trailing input", position: current.position)
        }
    }

    // MARK: Productions

    /// `x = expr`, `f(a, b) = expr`, or a plain expression.
    mutating func statement() throws(EngineError) -> Expression {
        if case .identifier(let name) = current.kind,
           index + 1 < tokens.count, case .assign = tokens[index + 1].kind,
           !Parser.isReductionName(name) { // sigma_i=1^10(…) is a summation, not an assignment
            let position = current.position
            guard !ReservedNames.contains(name.lowercased()) else {
                throw EngineError.parseError(message: "cannot assign to '\(name)'", position: position)
            }
            index += 2
            return .assignment(name: name, value: try comparison())
        }
        // `import Bits` — contextual: only `import` followed by a name commits,
        // so `import` stays usable as a variable and `import = 5` is an assignment.
        if case .identifier(let keyword) = current.kind, keyword.lowercased() == "import",
           index + 1 < tokens.count, case .identifier(let namespace) = tokens[index + 1].kind {
            index += 2
            return .importDirective(name: namespace)
        }
        if let namespaceDefinition = try namespaceDefinition() {
            return namespaceDefinition
        }
        if let dataDefinition = try dataDefinition() {
            return dataDefinition
        }
        if let definition = try functionDefinition() {
            return definition
        }
        return try comparison()
    }
}

/// Identifiers that cannot be assigned to (or defined as functions).
/// `sigma` is the special summation form, so a user function would be
/// uncallable anyway.
let ReservedNames: Set<String> = ["ans", "pi", "e", "tau", "π", "τ", "sigma", "if", "man", "manual", "help",
                                  "true", "false", "json", "rounding"]

extension Parser {
    /// `sigma_x`/`product_x` spellings are reserved for the indexed
    /// reduction forms.
    static func isReductionName(_ name: String) -> Bool {
        let lowered = name.lowercased()
        return lowered.hasPrefix("sigma_") || lowered.hasPrefix("product_")
    }
}
