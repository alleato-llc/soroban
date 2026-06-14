/// A lexical token with its half-open character range in the source line.
public struct Token: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case number(BigDecimal)
        case identifier(String)
        case string(String)    // "…" — double-quoted string literal, escapes applied
        // A:1 — column as typed, row 1-based. The pins ($A:$1) are COPY-TIME
        // data for the reference rewriter (fill/paste hold pinned axes);
        // evaluation ignores them — $A:$1 and A:1 are the same cell.
        case cellReference(column: String, row: Int, pinColumn: Bool, pinRow: Bool)
        case dotDot            // .. — cell range separator (A:1..A:9)
        case dot               // . — map member access (m.name); .5 stays a number
        case arrow             // -> — lambda (x -> x * 2); lexes before minus
        case bang              // ! — sheet qualifier (Budget!A:1); != lexes first
        case quotedName(String) // 'Q1 Budget' — sheet names with spaces
        case plus, minus, star, slash, percent, caret
        case shiftLeft, shiftRight     // << >> — Programmer-mode bit shifts (lexed mode-agnostically; parser gates)
        case ampersand, pipe           // & | — Programmer-mode bitwise AND/OR
        case tilde                     // ~ — Programmer-mode bitwise NOT (needs a fixed width)
        case sqrtSign          // √ — prefix square root
        case lessThan, greaterThan, lessOrEqual, greaterOrEqual, equalEqual, notEqual
        case assign            // =
        case leftParen, rightParen
        case leftBracket, rightBracket // [ ] — array literals and indexing
        case leftBrace, rightBrace     // { } — map literals
        case colon             // : — map key separator (A:1 consumes its own ':')
        case comma
        case end               // synthesized end-of-input marker
    }

    public let kind: Kind
    public let range: Range<Int>

    public var position: Int { range.lowerBound }
}
