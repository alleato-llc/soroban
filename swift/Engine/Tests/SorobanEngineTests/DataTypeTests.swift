import Testing
@testable import Anzan
@testable import SorobanEngine

/// Invariants the feature files can't express: typed-error shapes, codec
/// details, source-text round trips. User-visible behavior lives in
/// Features/datatypes.feature — keep it the point of truth.
@Suite("Data types (invariants)")
struct DataTypeTests {
    @Test func declarationSourceTextRoundTrips() throws {
        let expression = try Parser.parse("data Pt { x: Number, s: String, b: Boolean }")
        #expect(expression.sourceText == "data Pt { x: Number, s: String, b: Boolean }")
        #expect(try Parser.parse(expression.sourceText) == expression)
        #expect(!expression.containsCellReference)
    }

    @Test func documentationComesFromTheTrailingComment() {
        let calc = Calculator()
        _ = calc.evaluate("data A { v: Number } # has docs")
        _ = calc.evaluate("data B { v: Number } #")
        _ = calc.evaluate("data C { v: Number }")
        #expect(calc.environment.dataType(named: "A")?.documentation == "has docs")
        #expect(calc.environment.dataType(named: "B")?.documentation == nil) // empty comment
        #expect(calc.environment.dataType(named: "C")?.documentation == nil)
        // Case-insensitive lookup, like functions.
        #expect(calc.environment.dataType(named: "a")?.name == "A")
    }

    @Test func restoreVariablesDropsUnparseableAndUnresolvableEntries() {
        let calc = Calculator()
        _ = calc.evaluate("data Pt { x: Number }")
        calc.restoreVariables([
            "n": "1.5",                       // literal fast path
            "p": "Pt(x: 3)",                  // evaluated (constructor call)
            "bad": "Nope(x: 1)",              // unknown type — dropped
            "worse": "not ( parseable",       // unparseable — dropped
        ])
        #expect(calc.environment["n"] == .number(BigDecimal(string: "1.5")!))
        #expect(calc.environment["p"]?.kindName == "a Pt")
        #expect(calc.environment["bad"] == nil)
        #expect(calc.environment["worse"] == nil)
        // ans untouched by the whole restore.
        #expect(calc.environment.ans == .number(.zero))
    }

    @Test func recordErrorShapesAreTyped() {
        let calc = Calculator()
        _ = calc.evaluate("data Pt { x: Number }")
        _ = calc.evaluate("p = Pt(x: 1)")

        // Subscript with a non-string key names the kind.
        guard case .failure(let keyError) = calc.evaluate("p[0]") else {
            Issue.record("expected a key-type failure")
            return
        }
        #expect("\(keyError)".contains("map keys are strings"))

        // Member access on a non-structure names records as an option.
        guard case .failure(let memberError) = calc.evaluate("(5).x") else {
            Issue.record("expected a member failure")
            return
        }
        #expect("\(memberError)".contains("needs a map or data value"))

        // Records refuse numeric flattening with their type name.
        guard case .failure(let sumError) = calc.evaluate("sum(p)") else {
            Issue.record("expected a flatten failure")
            return
        }
        #expect("\(sumError)".contains("got a Pt"))
    }

    @Test func multiLineStringResultsOfferARawBlock() throws {
        let calc = Calculator()
        // Pretty JSON (the default) is multi-line → hosts render it raw.
        #expect(try calc.evaluate("toJson([1, 2])").get().rawBlock == "[\n  1,\n  2\n]")
        // Single-line strings keep canonical quoting (no block).
        #expect(try calc.evaluate("toJson([1, 2], Json.Compact)").get().rawBlock == nil)
        #expect(try calc.evaluate("\"plain\"").get().rawBlock == nil)
        // Non-string outcomes never offer one.
        #expect(try calc.evaluate("1 + 1").get().rawBlock == nil)
    }

    @Test func jsonParserHandlesSurrogatePairsAndRefusesBrokenOnes() throws {
        // 😀 is U+1F600 — a surrogate pair in JSON's \u notation; both the
        // escaped and the raw spelling must land on the same character.
        #expect(try JSONParser.parse(#""\uD83D\uDE00""#) == .string("😀"))
        #expect(try JSONParser.parse(#""😀""#) == .string("😀"))
        #expect(throws: EngineError.self) {
            try JSONParser.parse(#""\uD83D""#) // high half alone
        }
        #expect(throws: EngineError.self) {
            try JSONParser.parse(#""\uDE00""#) // low half alone
        }
        // Whitespace tolerance + nested shapes, while we're here.
        #expect(try JSONParser.parse(" { \"a\" : [ 1 , { \"b\" : 2 } ] } ")
            == .map([.init(key: "a", value: .array([
                .number(BigDecimal(1)),
                .map([.init(key: "b", value: .number(BigDecimal(2)))]),
            ]))]))
    }

    @Test func jsonEscapesControlCharactersAndRefusesNestedFunctions() throws {
        // \r and a raw control character take the \uXXXX path (the lexer
        // can't write them, so this exercises the serializer directly).
        let value = Value.string("a\rb\u{1}c")
        #expect(try value.jsonText(pretty: false) == "\"a\\rb\\u0001c\"")

        let withFunction = Value.array([.function(FunctionValue(kind: .builtin("abs")))])
        #expect(throws: EngineError.self) {
            try withFunction.jsonText(pretty: false)
        }
    }

    @Test func jsonParserRefusesEveryMalformedShape() {
        // The user-facing flagships (null, trailing junk, duplicate keys,
        // wrong arg type) are gherkin scenarios; this sweeps the remaining
        // syntax-error branches.
        let malformed = [
            "{1: 2}",            // object keys must be quoted
            "{\"a\" 1}",         // missing ':' after key
            "{\"a\": 1 2}",      // expected ',' or '}'
            "[1 2]",             // expected ',' or ']'
            "\"a",               // unterminated string
            "\"\\q\"",           // unknown escape
            "\"\\u12\"",         // \u needs 4 hex digits
            "-",                 // malformed number
            "tru",               // not a keyword
            "",                  // empty input
            String(repeating: "[", count: 300), // past the depth cap
        ]
        for text in malformed {
            #expect(throws: EngineError.self, "accepted: \(text.prefix(20))") {
                try JSONParser.parse(text)
            }
        }
    }
}
