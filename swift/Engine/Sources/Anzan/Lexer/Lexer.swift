import BigInt
/// Converts a source line into tokens. Whitespace separates tokens but is
/// otherwise insignificant.
package struct Lexer {
    private let chars: [Character]
    private var index = 0

    init(_ source: String) {
        self.chars = Array(source)
    }

    /// Splits a source line into its code and its trailing `#` comment,
    /// respecting string literals (a `#` inside `"…"` is not a comment). The
    /// comment is returned WITHOUT the leading `#`, trimmed; nil when absent.
    /// Both hosts use this to display/store comments; the calculator uses it
    /// to recognize a comment-only line as a note rather than a parse error.
    package static func splitComment(_ source: String) -> (code: String, comment: String?) {
        let chars = Array(source)
        var index = 0
        var inString = false
        while index < chars.count {
            let c = chars[index]
            if inString {
                if c == "\\" { index += 2; continue } // skip the escaped char
                if c == "\"" { inString = false }
            } else if c == "\"" {
                inString = true
            } else if c == "#" {
                let code = String(chars[..<index])
                let comment = String(chars[(index + 1)...])
                    .trimmingCharacters(in: .whitespaces)
                return (code, comment)
            }
            index += 1
        }
        return (source, nil)
    }

    /// Tokenizes the whole input, appending a final `.end` token.
    package static func tokenize(_ source: String) throws(EngineError) -> [Token] {
        var lexer = Lexer(source)
        var tokens: [Token] = []
        while let token = try lexer.next() {
            tokens.append(token)
        }
        tokens.append(Token(kind: .end, range: lexer.index..<lexer.index))
        return tokens
    }

    private var current: Character? {
        index < chars.count ? chars[index] : nil
    }

    private mutating func next() throws(EngineError) -> Token? {
        while let c = current, c.isWhitespace { index += 1 }
        guard let c = current else { return nil }
        let start = index

        // '#' starts a comment that runs to the end of the line. A trailing
        // comment on a function definition doubles as its documentation.
        if c == "#" {
            index = chars.count
            return nil
        }

        // Two-character operators before the single-character table
        // (so `==` never lexes as two assigns, `<=` as less-then-assign).
        if index + 1 < chars.count {
            let pair = String([c, chars[index + 1]])
            let twoChar: [String: Token.Kind] = [
                "==": .equalEqual, "!=": .notEqual,
                "<=": .lessOrEqual, ">=": .greaterOrEqual,
                "<<": .shiftLeft, ">>": .shiftRight, // before the single-char `<`/`>`
                "..": .dotDot, // before the number scanner grabs the first '.'
                "->": .arrow,  // before '-' lexes as minus
                "::": .colonColon, // namespace qualifier, before the single ':'
            ]
            if let kind = twoChar[pair] {
                index += 2
                return Token(kind: kind, range: start..<index)
            }
        }

        // A lone '.' is member access (m.name); '.' before a digit starts a
        // number (.5). Must run after the two-char pass ('..' is a range) and
        // before the simple table would otherwise claim it.
        if c == "." {
            if index + 1 < chars.count, chars[index + 1].isNumber {
                return try number(from: start)
            }
            index += 1
            return Token(kind: .dot, range: start..<index)
        }

        // Single-character operators and punctuation. The typographic forms
        // (× ÷ − ·) arrive constantly via copy/paste from documents.
        let simple: [Character: Token.Kind] = [
            "+": .plus, "-": .minus, "*": .star, "/": .slash,
            "%": .percent, "^": .caret, "=": .assign,
            "(": .leftParen, ")": .rightParen, ",": .comma,
            "[": .leftBracket, "]": .rightBracket,
            "{": .leftBrace, "}": .rightBrace,
            ":": .colon, // a cell reference consumes its own ':' (A:1)
            ";": .semicolon, // namespace member separator
            "×": .star, "·": .star, "÷": .slash, "−": .minus,
            "√": .sqrtSign,
            "<": .lessThan, ">": .greaterThan,
            "&": .ampersand, "|": .pipe, // Programmer-mode bitwise; parser errors elsewhere
            "~": .tilde, // Programmer-mode bitwise NOT
            "≤": .lessOrEqual, "≥": .greaterOrEqual, "≠": .notEqual,
            "!": .bang, // sheet qualifier — "!=" was caught by the two-char pass
            "∑": .identifier("sigma"),   // math symbols aren't letters, so the
            "∏": .identifier("product"), // identifier scanner can't pick them up
        ]
        if let kind = simple[c] {
            index += 1
            return Token(kind: kind, range: start..<index)
        }

        // "…" string literal with \" \\ \n \t escapes.
        if c == "\"" {
            return try stringLiteral(from: start)
        }

        // 'Quoted Sheet Name' — for names the identifier syntax can't carry.
        if c == "'" {
            index += 1
            let nameStart = index
            while let c = current, c != "'" { index += 1 }
            guard current == "'" else {
                throw EngineError.lexError(message: "unterminated sheet name quote", position: start)
            }
            let name = String(chars[nameStart..<index])
            index += 1
            return Token(kind: .quotedName(name), range: start..<index)
        }

        if c.isNumber {
            return try number(from: start)
        }

        // '$' pins a cell reference's column ($A:1); the row pin rides the
        // reference tail. Anything else after '$' is a loud lex error.
        if c == "$" {
            index += 1
            let nameStart = index
            while let c = current, c.isLetter { index += 1 }
            let name = String(chars[nameStart..<index])
            guard !name.isEmpty, current == ":",
                  let token = try cellReferenceTail(column: name, start: start,
                                                    pinColumn: true) else {
                throw EngineError.lexError(
                    message: "'$' pins a cell reference — write $A:1, A:$1, or $A:$1",
                    position: start)
            }
            return token
        }

        if c.isLetter || c == "_" {
            while let c = current, c.isLetter || c.isNumber || c == "_" { index += 1 }
            let name = String(chars[start..<index])

            // Letters-only identifier followed by ":<digits>" (or ":$digits"
            // — a pinned row) is a cell reference: A:1, b:12, A:$1.
            // Anything else keeps the ':' unconsumed.
            if name.allSatisfy(\.isLetter), current == ":",
               let token = try cellReferenceTail(column: name, start: start,
                                                 pinColumn: false) {
                return token
            }
            return Token(kind: .identifier(name), range: start..<index)
        }

        throw EngineError.lexError(message: "unexpected character '\(c)'", position: start)
    }

    /// Consumes `:[$]digits` after a column name, or returns nil leaving the
    /// ':' unconsumed (the caller's name is then a plain identifier).
    /// Expects `current == ":"`.
    private mutating func cellReferenceTail(column: String, start: Int,
                                            pinColumn: Bool) throws(EngineError) -> Token? {
        var peek = index + 1 // past ':'
        var pinRow = false
        if peek < chars.count, chars[peek] == "$" {
            pinRow = true
            peek += 1
        }
        guard peek < chars.count, chars[peek].isNumber else { return nil }
        index = peek
        let rowStart = index
        while let c = current, c.isNumber { index += 1 }
        guard let row = Int(String(chars[rowStart..<index])) else {
            throw EngineError.lexError(message: "cell row out of range", position: rowStart)
        }
        // Column case is preserved (resolution is case-insensitive anyway) —
        // map literals decompose compact `{b:1}` into key "b" + value 1 and
        // want the key exactly as typed.
        return Token(kind: .cellReference(column: column, row: row,
                                          pinColumn: pinColumn, pinRow: pinRow),
                     range: start..<index)
    }

    /// Scans `"…"`, applying `\"` `\\` `\n` `\t` escapes. Unterminated
    /// strings and unknown escapes are lex errors.
    private mutating func stringLiteral(from start: Int) throws(EngineError) -> Token {
        index += 1 // opening quote
        var text = ""
        while let c = current {
            if c == "\"" {
                index += 1
                return Token(kind: .string(text), range: start..<index)
            }
            if c == "\\" {
                index += 1
                switch current {
                case "\\": text.append("\\")
                case "\"": text.append("\"")
                case "n": text.append("\n")
                case "t": text.append("\t")
                case .some(let escaped):
                    throw EngineError.lexError(message: "unknown escape '\\\(escaped)'",
                                               position: index - 1)
                case nil:
                    break // falls out to the unterminated error below
                }
                index += 1
                continue
            }
            text.append(c)
            index += 1
        }
        throw EngineError.lexError(message: "unterminated string", position: start)
    }

    /// Scans `123`, `1.5`, `1_000`, `2.5e-3` — and the programmer literals
    /// `0xFF` / `0b1010` (any width; `_` separators welcome). The leading
    /// sign is not part of the literal — the parser handles unary minus.
    private mutating func number(from start: Int) throws(EngineError) -> Token {
        if chars[index] == "0", index + 1 < chars.count,
           let radix = ["x": 16, "X": 16, "b": 2, "B": 2][String(chars[index + 1])] {
            return try radixLiteral(from: start, radix: radix)
        }
        var sawDot = false
        scan: while let c = current {
            switch c {
            case "0"..."9", "_":
                index += 1
            case "." where !sawDot:
                sawDot = true
                index += 1
            case "e", "E":
                // Exponent only counts if followed by digits (with optional sign);
                // otherwise it's the start of an identifier (e.g. `2e` → `2 * e`? No — error).
                var peek = index + 1
                if peek < chars.count, chars[peek] == "+" || chars[peek] == "-" { peek += 1 }
                guard peek < chars.count, chars[peek].isNumber else { break scan }
                index = peek + 1
                while let c = current, c.isNumber { index += 1 }
                break scan
            default:
                break scan
            }
        }

        // A second dot directly after the literal ("1.2.3") would otherwise
        // lex as two numbers and silently become implicit multiplication.
        if sawDot, current == "." {
            throw EngineError.lexError(message: "malformed number '\(String(chars[start...index]))'",
                                       position: start)
        }

        let text = String(chars[start..<index])
        guard let value = BigDecimal(string: text) else {
            throw EngineError.lexError(message: "malformed number '\(text)'", position: start)
        }
        return Token(kind: .number(value), range: start..<index)
    }

    /// `0xDEAD_BEEF` / `0b1010_1010` — exact integers at any width. A letter,
    /// digit, or '.' left dangling after the digits is a lex error (so
    /// `0xFG` and `0x1.5` fail loudly instead of becoming implicit
    /// multiplications).
    private mutating func radixLiteral(from start: Int,
                                       radix: Int) throws(EngineError) -> Token {
        index += 2 // "0x" / "0b"
        var value = BigInt(0)
        var digits = 0
        scan: while let c = current {
            if c == "_" {
                index += 1
                continue
            }
            let digit: Int
            switch radix {
            case 16:
                guard let hex = c.hexDigitValue else { break scan }
                digit = hex
            default:
                guard c == "0" || c == "1" else { break scan }
                digit = c == "1" ? 1 : 0
            }
            value = value * BigInt(radix) + BigInt(digit)
            digits += 1
            index += 1
        }
        let name = radix == 16 ? "hex" : "binary"
        guard digits > 0 else {
            throw EngineError.lexError(message: "\(name) literal needs digits", position: start)
        }
        if let c = current, c.isLetter || c.isNumber || c == "." {
            throw EngineError.lexError(
                message: "malformed \(name) literal '\(String(chars[start...index]))'",
                position: start)
        }
        return Token(kind: .number(BigDecimal(significand: value, exponent: 0)),
                     range: start..<index)
    }
}
