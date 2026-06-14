import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Calculator facade")
struct CalculatorTests {
    @Test func successUpdatesAns() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("6 * 7").get() == .value(BigDecimal(42)))
        #expect(calc.environment.ans == .number(BigDecimal(42)))
    }

    @Test func errorsComeBackAsFailures() {
        let calc = Calculator()
        #expect(calc.evaluate("1 +").isFailure)
        #expect(calc.evaluate("").isFailure)
    }

    @Test func errorDescriptionsAreHumanReadable() {
        guard case .failure(let error) = Calculator().evaluate("1 / 0") else {
            Issue.record("expected failure")
            return
        }
        #expect("\(error)" == "division by zero")
    }
}

@Suite("Programmer notation")
struct ProgrammerNotationTests {
    /// The CLI's hex-echo trigger: 0x/0b at a token boundary, or the
    /// base/bit functions — display-only, so a heuristic is acceptable,
    /// but it must not fire on ordinary arithmetic.
    @Test func detectsProgrammerLines() {
        for line in ["0xFF + 1", "bitAnd(a, b)", "fromBase(\"ff\", 16)",
                     "x + 0b1010", "BITOR(1, 2)"] {
            #expect(Calculator.usesProgrammerNotation(line), "\(line)")
        }
        for line in ["1 + 2", "10x", "a0b", "orbit(3)", "box = 5", "pmt(0.05, 12, 1)"] {
            #expect(!Calculator.usesProgrammerNotation(line), "\(line)")
        }
    }
}

@Suite("Autocomplete")
struct CompletionTests {
    @Test func completesFunctionsCaseInsensitively() {
        let names = Calculator().completions(forPrefix: "PER").map(\.name)
        #expect(names == ["percent", "percentChange", "percentile", "percentOf", "perm"])
    }

    @Test func includesConstantsAndVariables() {
        let calc = Calculator()
        _ = calc.evaluate("price = 100")
        let completions = calc.completions(forPrefix: "p")
        #expect(completions.contains(Completion(name: "pi", kind: .constant)))
        #expect(completions.contains(Completion(name: "price", kind: .variable)))
        #expect(completions.contains(Completion(name: "pmt", kind: .function)))
        // Sorted case-insensitively by name.
        #expect(completions.map(\.name) == completions.map(\.name)
            .sorted { $0.lowercased() < $1.lowercased() })
    }

    @Test func kindBadges() {
        #expect(Completion.Kind.function.badge == "ƒ")
        #expect(Completion.Kind.variable.badge == "var")
        #expect(Completion.Kind.constant.badge == "const")
    }

    @Test func emptyPrefixAndExactMatchesYieldNothing() {
        let calc = Calculator()
        #expect(calc.completions(forPrefix: "").isEmpty)
        // "abs" is the only match for itself — nothing left to complete.
        #expect(calc.completions(forPrefix: "abs").isEmpty)
        // "max" exact but "max" is the lone candidate, while "m" has many.
        #expect(!calc.completions(forPrefix: "m").isEmpty)
        #expect(calc.completions(forPrefix: "zzz").isEmpty)
    }

    @Test func exactPrefixWithLongerSiblingsStillCompletes() {
        // "percent" matches percent, percentChange, percentile, percentOf —
        // the exact match must not collapse the list.
        let calc = Calculator()
        #expect(calc.completions(forPrefix: "percent").count == 4)
    }

    @Test func expectsOperandPointModeHeuristic() {
        // Drafts that should accept a clicked cell reference…
        for draft in ["=", "B:1 +", "sum(", "sum(B:1,", "if(B:1 >", "2 *",
                      "B:1..", "= B:1 ×", "1 ≤", "√"] {
            #expect(Calculator.expectsOperand(draft), "\(draft)")
        }
        // …and drafts that should commit instead.
        for draft in ["", "   ", "Q1 revenue", "B:1 + B:2", "sum(B:1)", "42"] {
            #expect(!Calculator.expectsOperand(draft), "\(draft)")
        }
    }

    @Test func trailingIdentifierExtraction() {
        #expect(Calculator.trailingIdentifier(of: "1 + sq") == "sq")
        #expect(Calculator.trailingIdentifier(of: "rate_2") == "rate_2")
        #expect(Calculator.trailingIdentifier(of: "2p") == "p")     // 2 is a literal
        #expect(Calculator.trailingIdentifier(of: "1 + 2") == "")
        #expect(Calculator.trailingIdentifier(of: "") == "")
        #expect(Calculator.trailingIdentifier(of: "sqrt(") == "")
        #expect(Calculator.trailingIdentifier(of: "x + _tmp") == "_tmp")
    }
}

@Suite("Environment change tracking")
struct ChangeCountTests {
    @Test func mutationsBumpThePlainEvaluationsDont() throws {
        let calc = Calculator()
        let env = calc.environment

        let start = env.changeCount
        _ = calc.evaluate("1 + 1")                  // pure calculation
        #expect(env.changeCount == start)           // ans doesn't count

        _ = calc.evaluate("x = 5")
        #expect(env.changeCount == start + 1)

        _ = calc.evaluate("x = 5")                  // same value — no change
        #expect(env.changeCount == start + 1)

        _ = calc.evaluate("x = 6")
        #expect(env.changeCount == start + 2)

        _ = calc.evaluate("f(a) = a * 2")           // definitions count
        #expect(env.changeCount == start + 3)

        let beforeReplace = env.changeCount
        env.replaceUserVariables([:])
        env.replaceUserFunctions([:])
        #expect(env.changeCount == beforeReplace + 2)
    }
}
