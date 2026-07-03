import Testing
@testable import Anzan
@testable import SorobanEngine

/// `Expression.sourceText` is the persistence contract for lambdas (a saved
/// `f = x -> …` re-enters through it), so the bar is RE-PARSEABILITY for
/// every AST shape: parse → print → parse must reproduce the tree.
@Suite("Expression source printing")
struct SourceTextTests {
    @Test(arguments: [
        // numbers, variables, operators (each binary + comparison + unary)
        "1.5 + 2", "a - b", "a * b", "a / b", "a % b", "2 ^ 10", "-x",
        "a < b", "a > b", "a <= b", "a >= b", "a == b", "a != b",
        // strings, structures, access
        "\"say \\\"hi\\\"\\n\"", "[1, 2, [3]]", "{a: 1, \"two words\": 2}",
        "arr[0]", "m.key", "people[1].age", "{a: [1, 2]}.a[1]",
        // cells: bare, qualified, quoted-qualified, ranges in calls
        "A:1 + 2", "Budget!B:7", "'Q1 Budget'!C:3",
        "sum(A:1..B:9, 5)", "sum('Q1 Budget'!A:1..A:9)",
        // calls, conditionals, definitions, assignment, reductions, man
        "round(pmt(0.05/12, 360, 200000), 2)",
        "if(a < b, 1, 2 / 0)",
        "x = a + 1",
        "f(x, y) = x * y + 1",
        "sigma_i=1^10(i^2)", "product_k=(n - 1)^(m + 1)(k)",
        "man pmt",
        // lambdas (the original consumer), with structures inside
        "x -> x * 2", "(a, b) -> a + b", "() -> 7",
        "f = x -> sum([x, A:1])",
        // named cells
        "'Projected Rate' * 12", "Budget!'Rate' + 1",
    ])
    func roundTrips(_ source: String) throws {
        let parsed = try Parser.parse(source)
        let printed = parsed.sourceText
        let reparsed = try Parser.parse(printed)
        #expect(reparsed == parsed, "printed form '\(printed)' didn't round-trip")
    }
}

/// `containsCellReference` decides formula-vs-label classification — every
/// node kind must propagate it.
@Suite("containsCellReference propagation")
struct ContainsCellReferenceTests {
    @Test(arguments: [
        "A:1", "Budget!A:1", "sum(A:1..A:9)", "-A:1", "A:1 + 1", "1 + A:1",
        "f(A:1)", "x = A:1", "g(x) = A:1 * x", "if(A:1, 1, 2)",
        "if(1, A:1, 2)", "if(1, 2, A:1)", "sigma_i=A:1^2(i)",
        "sigma_i=1^A:1(i)", "sigma_i=1^2(A:1)", "A:1 < 2",
        "[A:1]", "{a: A:1}", "[1][A:1]", "(x -> A:1)", "{a: 1}[\"a\"] + A:1",
        "'A Name'", // named cells ARE cell references
    ])
    func detectsRefs(_ source: String) throws {
        #expect(try Parser.parse(source).containsCellReference)
    }

    @Test(arguments: [
        "1 + 2", "x * y", "f(1)", "x = 1", "f(x) = x", "if(1, 2, 3)",
        "sigma_i=1^3(i)", "\"text\"", "[1, {a: 2}]", "m.key", "arr[0]",
        "x -> x * 2", "a < b", "man pmt",
    ])
    func cleanOfRefs(_ source: String) throws {
        #expect(!(try Parser.parse(source).containsCellReference))
    }
}
