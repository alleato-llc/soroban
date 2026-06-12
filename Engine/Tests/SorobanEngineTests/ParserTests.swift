import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Parser")
struct ParserTests {
    private func num(_ s: String) -> Expression { .number(BigDecimal(string: s)!) }

    @Test func precedence() throws {
        // 1 + 2 * 3 → 1 + (2 * 3)
        #expect(try Parser.parse("1 + 2 * 3") ==
            .binary(.add, num("1"), .binary(.multiply, num("2"), num("3"))))
    }

    @Test func leftAssociativity() throws {
        // 8 - 2 - 1 → (8 - 2) - 1
        #expect(try Parser.parse("8 - 2 - 1") ==
            .binary(.subtract, .binary(.subtract, num("8"), num("2")), num("1")))
        // 8 / 4 / 2 → (8 / 4) / 2
        #expect(try Parser.parse("8 / 4 / 2") ==
            .binary(.divide, .binary(.divide, num("8"), num("4")), num("2")))
    }

    @Test func powerIsRightAssociative() throws {
        // 2 ^ 3 ^ 2 → 2 ^ (3 ^ 2)
        #expect(try Parser.parse("2 ^ 3 ^ 2") ==
            .binary(.power, num("2"), .binary(.power, num("3"), num("2"))))
    }

    @Test func unaryMinusBindsLooserThanPower() throws {
        // -2^2 → -(2^2)
        #expect(try Parser.parse("-2^2") == .unaryMinus(.binary(.power, num("2"), num("2"))))
        // 2^-1 → 2^(-1)
        #expect(try Parser.parse("2^-1") == .binary(.power, num("2"), .unaryMinus(num("1"))))
    }

    @Test func parentheses() throws {
        #expect(try Parser.parse("(1 + 2) * 3") ==
            .binary(.multiply, .binary(.add, num("1"), num("2")), num("3")))
    }

    @Test func implicitMultiplication() throws {
        #expect(try Parser.parse("2(3)") == .binary(.multiply, num("2"), num("3")))
        #expect(try Parser.parse("2x") == .binary(.multiply, num("2"), .variable("x")))
        #expect(try Parser.parse("(2)(3)") == .binary(.multiply, num("2"), num("3")))
        #expect(try Parser.parse("2sqrt(4)") ==
            .binary(.multiply, num("2"), .call(name: "sqrt", arguments: [num("4")])))
    }

    @Test func functionCalls() throws {
        #expect(try Parser.parse("min(1, 2, 3)") ==
            .call(name: "min", arguments: [num("1"), num("2"), num("3")]))
        #expect(try Parser.parse("abs(-5)") ==
            .call(name: "abs", arguments: [.unaryMinus(num("5"))]))
    }

    @Test func assignment() throws {
        #expect(try Parser.parse("x = 5 * 3") ==
            .assignment(name: "x", value: .binary(.multiply, num("5"), num("3"))))
    }

    @Test func cannotAssignToReservedNames() {
        #expect(throws: EngineError.self) { try Parser.parse("ans = 5") }
        #expect(throws: EngineError.self) { try Parser.parse("pi = 3") }
    }

    @Test func variablesAndAns() throws {
        #expect(try Parser.parse("ans * 2") ==
            .binary(.multiply, .variable("ans"), num("2")))
    }

    @Test(arguments: ["1 +", "(1", "min(1,", "* 3", "1 2 +", ")", "1 = 2"])
    func rejectsMalformedExpressions(source: String) {
        #expect(throws: EngineError.self) { try Parser.parse(source) }
    }

    @Test func errorsCarryPositions() {
        #expect(throws: EngineError.parseError(message: "expected ')'", position: 6)) {
            try Parser.parse("(1 + 2")
        }
    }
}
