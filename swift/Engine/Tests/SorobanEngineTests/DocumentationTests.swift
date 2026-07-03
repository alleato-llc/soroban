import Testing
@testable import Anzan
@testable import SorobanEngine

/// The docs-can't-drift suite: every registered function must carry complete
/// documentation, and every example — built-in, special form, operator,
/// constant — must actually evaluate. Add a function without docs and the
/// compiler stops you; ship a broken example and this fails.
@Suite("Documentation")
struct DocumentationTests {
    /// A calculator with a seeded sheet so cell/range examples evaluate.
    private func makeDocCalculator() -> (Calculator, Spreadsheet) {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        calc.cellResolver = { [weak sheet] _, column, row in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValue(column: column, row: row)
        }
        calc.rangeResolver = { [weak sheet] _, fc, fr, tc, tr in
            guard let sheet else { throw EngineError.domainError(message: "no sheet") }
            return try sheet.numericValues(fromColumn: fc, fromRow: fr,
                                           toColumn: tc, toRow: tr)
        }
        for (column, row, raw) in [(0, 0, "10"), (0, 1, "20"), (0, 2, "30"),
                                   (1, 0, "100"), (1, 1, "200"), (1, 2, "300")] {
            sheet.setCell(raw, at: CellAddress(column: column, row: row))
        }
        return (calc, sheet)
    }

    @Test func everyBuiltinIsFullyDocumented() {
        for function in FunctionRegistry.standard.all {
            #expect(!function.signature.isEmpty, "\(function.name) has no signature")
            #expect(function.signature.contains(function.name) || function.signature.contains("("),
                    "\(function.name) signature looks wrong: \(function.signature)")
            #expect(function.summary.count > 10, "\(function.name) summary too thin")
            #expect(!function.examples.isEmpty, "\(function.name) has no examples")
        }
    }

    @Test func everyExampleEvaluates() {
        // Examples within one entry run sequentially on a shared calculator
        // (so "1 + 1" then "ans * 2" works); entries are independent.
        for category in Calculator.builtinDocumentation {
            for entry in category.entries {
                let (calc, sheet) = makeDocCalculator()
                defer { _ = sheet }
                for example in entry.examples {
                    if case .failure(let error) = calc.evaluate(example) {
                        Issue.record("\(entry.name) example failed: '\(example)' → \(error)")
                    }
                }
            }
        }
    }

    @Test func categoriesCoverTheWholeRegistry() {
        let documented = Set(Calculator.builtinDocumentation
            .flatMap(\.entries).map { $0.name.lowercased() })
        for function in FunctionRegistry.standard.all {
            #expect(documented.contains(function.name.lowercased()),
                    "\(function.name) missing from documentation categories")
        }
    }

    @Test func userFunctionsAppearLive() {
        let calc = Calculator()
        #expect(!calc.documentation().contains { $0.title == "Your Functions" })

        _ = calc.evaluate("tax(x) = x * 1.0825")
        let categories = calc.documentation()
        let yours = categories.first { $0.title == "Your Functions" }
        #expect(yours?.entries.first?.name == "tax")
        // The definition line is the clickable example; the summary nudges
        // toward a # doc comment until one exists.
        #expect(yours?.entries.first?.examples == ["tax(x) = x * 1.0825"])
        #expect(yours?.entries.first?.summary.contains("trailing comment") == true)
        // And it comes first — your own work tops the reference.
        #expect(categories.first?.title == "Your Functions")
    }

    @Test func singleLookupCoversAllKinds() {
        let calc = Calculator()
        #expect(calc.documentation(for: "pmt")?.signature.contains("pmt(") == true)
        #expect(calc.documentation(for: "PMT") != nil) // case-insensitive
        #expect(calc.documentation(for: "if")?.summary.contains("taken branch") == true)
        #expect(calc.documentation(for: "sigma") != nil)
        _ = calc.evaluate("f(x) = x + 1")
        #expect(calc.documentation(for: "f")?.signature == "f(x)")
        #expect(calc.documentation(for: "nope") == nil)
    }
}
