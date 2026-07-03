import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Lexer")
struct LexerTests {
    private func kinds(_ source: String) throws -> [Token.Kind] {
        try Lexer.tokenize(source).map(\.kind)
    }

    private func num(_ s: String) -> Token.Kind { .number(BigDecimal(string: s)!) }

    @Test func tokenizesArithmetic() throws {
        #expect(try kinds("1 + 2*3") == [num("1"), .plus, num("2"), .star, num("3"), .end])
    }

    @Test func tokenizesAllOperators() throws {
        #expect(try kinds("+-*/%^=(),") == [
            .plus, .minus, .star, .slash, .percent, .caret,
            .assign, .leftParen, .rightParen, .comma, .end,
        ])
    }

    @Test func tokenizesNumberFormats() throws {
        #expect(try kinds("1_000 2.5e-3 .5 1E2") == [
            num("1000"), num("0.0025"), num("0.5"), num("100"), .end,
        ])
    }

    @Test func eNotFollowedByDigitsIsNotAnExponent() throws {
        // `2e` lexes as number 2 then identifier e (Euler's constant) —
        // implicit multiplication handles the rest.
        #expect(try kinds("2e") == [num("2"), .identifier("e"), .end])
        #expect(try kinds("2e+x") == [num("2"), .identifier("e"), .plus, .identifier("x"), .end])
    }

    @Test func tokenizesIdentifiers() throws {
        #expect(try kinds("rate_2 = pmt(x)") == [
            .identifier("rate_2"), .assign,
            .identifier("pmt"), .leftParen, .identifier("x"), .rightParen, .end,
        ])
    }

    @Test func recordsPositions() throws {
        let tokens = try Lexer.tokenize("12 + ab")
        #expect(tokens.map(\.range) == [0..<2, 3..<4, 5..<7, 7..<7])
    }

    @Test func rejectsUnknownCharacters() {
        // '#' became the comment marker — '@' is still illegal.
        #expect(throws: EngineError.lexError(message: "unexpected character '@'", position: 2)) {
            try Lexer.tokenize("1 @ 2")
        }
    }

    @Test func commentsRunToEndOfLine() throws {
        #expect(try Lexer.tokenize("1 + 2 # three # four").map(\.kind).count == 4) // 1 + 2 end
        #expect(try Lexer.tokenize("# only a comment").map(\.kind) == [.end])
    }

    @Test func rejectsMalformedNumbers() {
        #expect(throws: EngineError.self) { try Lexer.tokenize("1.2.3") }
    }
}
