import Testing
@testable import Anzan

/// The statement accumulator — what scenarios can't easily pin: exact joined
/// text, line attribution, and the streaming push/finish contract.
/// User-visible splitting behavior lives in spec/anzan/scripting.feature.
@Suite("Statement accumulator")
struct ScriptTests {
    @Test func balancedLinesPassStraightThrough() throws {
        let statements = try StatementAccumulator.statements(of: "1 + 1\nx = 3\nx")
        #expect(statements.map(\.text) == ["1 + 1", "x = 3", "x"])
        #expect(statements.map(\.line) == [1, 2, 3])
    }

    @Test func openBracketsJoinToOneLogicalLine() throws {
        let statements = try StatementAccumulator.statements(of: """
            sum(
                1, 2,
                3
            )
            """)
        #expect(statements.map(\.text) == ["sum( 1, 2, 3 )"])
        #expect(statements.first?.line == 1)
    }

    @Test func blankAndCommentLinesInsideContinuationAreSkipped() throws {
        let statements = try StatementAccumulator.statements(of: """
            sum(

                1,
                # a note mid-block
                2
            )
            """)
        #expect(statements.map(\.text) == ["sum( 1, 2 )"])
    }

    @Test func firstLineCommentReattachesToTheJoinedStatement() throws {
        let statements = try StatementAccumulator.statements(of: """
            triple(x) = (    # three of x
                x * 3
            )
            """)
        #expect(statements.map(\.text) == ["triple(x) = ( x * 3 )  # three of x"])
    }

    @Test func bracketsInsideStringsAreText() throws {
        let statements = try StatementAccumulator.statements(of: "s = \"{ ( [\"\nlen(s)")
        #expect(statements.count == 2)
        #expect(statements[0].text == "s = \"{ ( [\"")
    }

    @Test func commentOnlyLinesAreStandaloneStatements() throws {
        let statements = try StatementAccumulator.statements(of: "#!/usr/bin/env soroban\n# note\n1")
        #expect(statements.map(\.text) == ["#!/usr/bin/env soroban", "# note", "1"])
        #expect(statements.map(\.line) == [1, 2, 3])
    }

    @Test func strayCloserDoesNotUnderflow() throws {
        // The parser owns the error; the splitter must not swallow the NEXT line.
        let statements = try StatementAccumulator.statements(of: ") + 1\n2 + 2")
        #expect(statements.map(\.text) == [") + 1", "2 + 2"])
    }

    @Test func unterminatedBlockThrowsNamingTheOpeningLine() {
        #expect(throws: EngineError.parseError(
            message: "unterminated statement — the block opened at line 2 is missing a closing bracket",
            position: 0)
        ) {
            try StatementAccumulator.statements(of: "1 + 1\nnamespace Broken {\n    x() = 1")
        }
    }

    @Test func streamingPushReportsPendingState() throws {
        var accumulator = StatementAccumulator()
        #expect(accumulator.push("sum(") == nil)
        #expect(accumulator.isPending)
        #expect(accumulator.pendingText == "sum(")
        let statement = accumulator.push("1, 2)")
        #expect(statement?.text == "sum( 1, 2)")
        #expect(!accumulator.isPending)
        try accumulator.finish() // no-op when nothing pending
    }
}
