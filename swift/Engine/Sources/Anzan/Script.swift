/// Splits multi-line SOURCE into logical statements — the engine primitive
/// behind `.anzan` script files, statement-aware pipes, and (later) multi-line
/// paste in the apps. A statement ends at a newline UNLESS a `(` `[` `{` is
/// still open; continuation lines JOIN INTO ONE LOGICAL LINE, so everything
/// downstream — the parser, caret columns, error echoes, doc comments — behaves
/// exactly as if the statement had been typed on one line (newlines are already
/// insignificant to the lexer; this just decides where statements END).
///
/// Streaming by design: `push(_:)` one physical line at a time (a pipe, a REPL,
/// a file), collect completed statements, then `finish()` — which is where an
/// unclosed block becomes a loud error instead of a silent swallow.
public struct StatementAccumulator: Sendable {
    /// A completed logical statement.
    public struct Statement: Equatable, Sendable {
        /// The joined one-line form (first-line trailing comment re-attached).
        public let text: String
        /// 1-based physical line where the statement began — for `file:line`
        /// error reporting and continuation prompts.
        public let line: Int
    }

    private var parts: [String] = []
    private var depth = 0
    private var startLine = 0
    private var currentLine = 0
    private var firstComment: String?

    public init() {}

    /// True while a statement is open (brackets unbalanced) — REPLs show a
    /// continuation prompt; `finish()` would error.
    public var isPending: Bool { !parts.isEmpty }

    /// Feed the next physical line. Returns a completed statement, or nil
    /// while blank or continuing. Comment-only lines (including a `#!` shebang)
    /// are statements of their own — the engine treats them as notes.
    public mutating func push(_ line: String) -> Statement? {
        currentLine += 1
        let split = Lexer.splitComment(line)
        let code = split.code.trimmingCharacters(in: .whitespaces)

        if parts.isEmpty {
            if code.isEmpty {
                guard split.comment != nil else { return nil } // blank line
                // Comment-only: a standalone note, passed through as written.
                return Statement(text: line.trimmingCharacters(in: .whitespaces),
                                 line: currentLine)
            }
            startLine = currentLine
            firstComment = split.comment
        } else if code.isEmpty {
            return nil // blank or comment-only line INSIDE a continuation
        }

        parts.append(code)
        depth = max(0, depth + Self.bracketDelta(of: code))
        guard depth == 0 else { return nil }

        let joined = parts.joined(separator: " ")
        let comment = firstComment.map { "  # \($0)" } ?? ""
        let statement = Statement(text: joined + comment, line: startLine)
        reset()
        return statement
    }

    /// End of input. A no-op when nothing is pending; an unclosed block is a
    /// parse error naming the line that opened it (statements complete in
    /// `push` the moment depth returns to zero, so a pending statement at EOF
    /// is ALWAYS unterminated).
    public mutating func finish() throws(EngineError) {
        guard isPending else { return }
        let opened = startLine
        reset()
        throw EngineError.parseError(
            message: "unterminated statement — the block opened at line \(opened) is missing a closing bracket",
            position: 0)
    }

    /// The pending statement's text so far — for the unterminated-error echo.
    public var pendingText: String { parts.joined(separator: " ") }

    private mutating func reset() {
        parts = []
        depth = 0
        firstComment = nil
    }

    /// Net bracket depth of a comment-stripped line, ignoring bracket
    /// characters inside string literals (the same string walk
    /// `Lexer.splitComment` uses).
    private static func bracketDelta(of code: String) -> Int {
        var delta = 0
        var inString = false
        var index = code.startIndex
        while index < code.endIndex {
            let c = code[index]
            if inString {
                if c == "\\" { // skip the escaped character
                    index = code.index(after: index)
                    if index == code.endIndex { break }
                } else if c == "\"" {
                    inString = false
                }
            } else {
                switch c {
                case "\"": inString = true
                case "(", "[", "{": delta += 1
                case ")", "]", "}": delta -= 1
                default: break
                }
            }
            index = code.index(after: index)
        }
        return delta
    }
}

extension StatementAccumulator {
    /// Convenience for whole-source callers (the SDK one-liner): split
    /// `source` into its logical statements. Throws on an unclosed block.
    public static func statements(of source: String) throws(EngineError) -> [Statement] {
        var accumulator = StatementAccumulator()
        var result: [Statement] = []
        for line in source.split(separator: "\n", omittingEmptySubsequences: false) {
            if let statement = accumulator.push(String(line)) {
                result.append(statement)
            }
        }
        try accumulator.finish()
        return result
    }
}
