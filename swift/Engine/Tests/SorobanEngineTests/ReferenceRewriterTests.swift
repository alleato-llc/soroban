import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Reference rewriting ($ pins, shifts, renames)")
struct ReferenceRewriterTests {

    // MARK: $ pins (lexer + evaluation transparency)

    @Test func pinsLexAndEvaluateLikePlainReferences() throws {
        let calc = Calculator()
        let store = SheetStore(calculator: calc)
        store.activeSheet.grid.setCell("42", at: CellAddress(column: 0, row: 0))
        #expect(try calc.evaluate("$A:$1 + A:1").get() == .value(BigDecimal(84)))
        #expect(try calc.evaluate("$A:1 * 2").get() == .value(BigDecimal(84)))
        #expect(try calc.evaluate("A:$1 * 2").get() == .value(BigDecimal(84)))
    }

    @Test func dollarAloneIsALoudLexError() {
        // `$5` is NOT here — `$`+digit is the core currency literal (any mode);
        // only `$`+letter shapes are the cell pin, and a dangling `$` is loud.
        let calc = Calculator()
        for input in ["$", "$x", "2 + $", "$A", "$A:"] {
            guard case .failure(let error) = calc.evaluate(input) else {
                Issue.record("'\(input)' should be a lex error"); return
            }
            #expect("\(error)".contains("$"))
        }
    }

    @Test func pinnedTokenCarriesItsPins() throws {
        let tokens = try Lexer.tokenize("$A:1 + B:$2 + $C:$3 + D:4")
        var pins: [(Bool, Bool)] = []
        for token in tokens {
            if case .cellReference(_, _, let pinColumn, let pinRow) = token.kind {
                pins.append((pinColumn, pinRow))
            }
        }
        #expect(pins.count == 4)
        #expect(pins[0] == (true, false))
        #expect(pins[1] == (false, true))
        #expect(pins[2] == (true, true))
        #expect(pins[3] == (false, false))
    }

    // MARK: adjustingRelative (fill / paste)

    @Test func adjustsUnpinnedAxesAndHoldsPins() {
        #expect(ReferenceRewriter.adjustingRelative("=A:2 * rate", byRows: 1, byColumns: 0)
                == "=A:3 * rate")
        #expect(ReferenceRewriter.adjustingRelative("=A:2 * $C:$1", byRows: 2, byColumns: 0)
                == "=A:4 * $C:$1")
        #expect(ReferenceRewriter.adjustingRelative("=$A:2 + B:$5", byRows: 3, byColumns: 1)
                == "=$A:5 + C:$5")
        // Comments and spacing survive (token-precise splices).
        #expect(ReferenceRewriter.adjustingRelative("= A:1  + 2  # note", byRows: 1, byColumns: 0)
                == "= A:2  + 2  # note")
        // Nothing to adjust → nil.
        #expect(ReferenceRewriter.adjustingRelative("= 1 + 2", byRows: 1, byColumns: 0) == nil)
        #expect(ReferenceRewriter.adjustingRelative("=A:1", byRows: 0, byColumns: 0) == nil)
    }

    @Test func adjustingMovesQualifiedRefsAndSkipsNamedCells() {
        #expect(ReferenceRewriter.adjustingRelative("=Budget!A:1 * 2", byRows: 1, byColumns: 0)
                == "=Budget!A:2 * 2")
        #expect(ReferenceRewriter.adjustingRelative("='Q1 Budget'!B:3", byRows: 0, byColumns: 1)
                == "='Q1 Budget'!C:3")
        // Named cells are the absolute-by-meaning reference.
        #expect(ReferenceRewriter.adjustingRelative("='Projected Rate' * 2", byRows: 5, byColumns: 0) == nil)
        #expect(ReferenceRewriter.adjustingRelative("=Budget!'Rate' + A:1", byRows: 1, byColumns: 0)
                == "=Budget!'Rate' + A:2")
    }

    @Test func adjustingOffTheGridBecomesRefError() {
        #expect(ReferenceRewriter.adjustingRelative("=A:1 * 2", byRows: -1, byColumns: 0)
                == "=refError() * 2")
        #expect(ReferenceRewriter.adjustingRelative("=A:1 + B:1", byRows: 0, byColumns: -1)
                == "=refError() + A:1")
        #expect(ReferenceRewriter.adjustingRelative("=Z:1", byRows: 0, byColumns: 1)
                == "=refError()")
        #expect(ReferenceRewriter.adjustingRelative("=A:1000", byRows: 1, byColumns: 0)
                == "=refError()")
        // A dead corner kills the whole range; the qualifier goes with it.
        #expect(ReferenceRewriter.adjustingRelative("=sum(A:1..A:9)", byRows: -1, byColumns: 0)
                == "=sum(refError())")
        #expect(ReferenceRewriter.adjustingRelative("=sum(Budget!A:1..A:9)", byRows: -1, byColumns: 0)
                == "=sum(refError())")
    }

    @Test func adjustingSkipsMapKeysAndNamedArguments() {
        // {b:1} lexes as a cell-reference token but is a map KEY.
        #expect(ReferenceRewriter.adjustingRelative("={b:1}", byRows: 1, byColumns: 0) == nil)
        #expect(ReferenceRewriter.adjustingRelative("={a:1, b:2}", byRows: 1, byColumns: 0) == nil)
        // …while a map VALUE that's a real reference adjusts.
        #expect(ReferenceRewriter.adjustingRelative("={x: A:1}", byRows: 1, byColumns: 0)
                == "={x: A:2}")
        // Multi-letter "columns" are named-argument sugar, never cells.
        #expect(ReferenceRewriter.adjustingRelative("=Person(age:36)", byRows: 1, byColumns: 0) == nil)
        // Compact single-letter call args ARE cell references (documented).
        #expect(ReferenceRewriter.adjustingRelative("=f(a:1)", byRows: 1, byColumns: 0)
                == "=f(a:2)")
        // An array of refs adjusts each element (brackets aren't braces).
        #expect(ReferenceRewriter.adjustingRelative("=[A:1, B:2]", byRows: 1, byColumns: 0)
                == "=[A:2, B:3]")
    }

    // MARK: shifting (insert/delete rows & columns)

    @Test func insertShiftsReferencesAtOrBelow() {
        // Insert one row at row 3: refs to 3+ move down.
        #expect(ReferenceRewriter.shifting("=A:2 + A:3 + A:9", axis: .row, from: 3, by: 1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=A:2 + A:4 + A:10")
        // Unqualified refs on OTHER sheets stay put…
        #expect(ReferenceRewriter.shifting("=A:3", axis: .row, from: 3, by: 1,
                                           editedSheet: "Sheet 1", onEditedSheet: false) == nil)
        // …but qualified ones follow the edited sheet from anywhere.
        #expect(ReferenceRewriter.shifting("='Sheet 1'!A:3 + A:3", axis: .row, from: 3, by: 1,
                                           editedSheet: "Sheet 1", onEditedSheet: false)
                == "='Sheet 1'!A:4 + A:3")
        // Columns shift by index (insert at B pushes B→C).
        #expect(ReferenceRewriter.shifting("=B:1 * $B:$2", axis: .column, from: 1, by: 1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=C:1 * $C:$2")
    }

    @Test func deleteRewritesDeadRefsToRefError() {
        // Delete row 3: refs above stay, below shift up, AT it die loudly.
        #expect(ReferenceRewriter.shifting("=A:2 + A:3 + A:9", axis: .row, from: 3, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=A:2 + refError() + A:8")
        // The qualifier dies with the reference.
        #expect(ReferenceRewriter.shifting("=Budget!A:3 * 2", axis: .row, from: 3, by: -1,
                                           editedSheet: "Budget", onEditedSheet: false)
                == "=refError() * 2")
        // Delete column B: C slides into B; B itself dies.
        #expect(ReferenceRewriter.shifting("=B:1 + C:1", axis: .column, from: 1, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=refError() + B:1")
    }

    @Test func deleteShrinksRangesInward() {
        // Interior delete: the range just shortens at the far end.
        #expect(ReferenceRewriter.shifting("=sum(A:1..A:5)", axis: .row, from: 3, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(A:1..A:4)")
        // Endpoint deletes clamp inward.
        #expect(ReferenceRewriter.shifting("=sum(A:3..A:5)", axis: .row, from: 3, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(A:3..A:4)")
        #expect(ReferenceRewriter.shifting("=sum(A:1..A:5)", axis: .row, from: 5, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(A:1..A:4)")
        // Reversed corners keep their orientation.
        #expect(ReferenceRewriter.shifting("=sum(A:5..A:1)", axis: .row, from: 5, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(A:4..A:1)")
        // Deleting the whole span kills the range.
        #expect(ReferenceRewriter.shifting("=sum(A:3..A:4) + 1", axis: .row, from: 3, by: -2,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(refError()) + 1")
        // Multi-row delete spanning an endpoint.
        #expect(ReferenceRewriter.shifting("=sum(A:2..A:6)", axis: .row, from: 4, by: -3,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=sum(A:2..A:3)")
    }

    @Test func rangePairingNeedsTheDotDotToken() {
        // Two refs an operator apart are NOT a range — each shifts alone.
        #expect(ReferenceRewriter.shifting("=A:3 + A:5", axis: .row, from: 3, by: -1,
                                           editedSheet: "Sheet 1", onEditedSheet: true)
                == "=refError() + A:4")
    }

    // MARK: renamingSheet

    @Test func renameRewritesBothQuotingStyles() {
        #expect(ReferenceRewriter.renamingSheet("=Budget!A:1 + budget!B:2 * 'Budget'!C:3",
                                                from: "Budget", to: "Plan")
                == "=Plan!A:1 + Plan!B:2 * Plan!C:3")
        // A new name that needs quoting gets it.
        #expect(ReferenceRewriter.renamingSheet("=Budget!A:1", from: "Budget", to: "Q1 Plan")
                == "='Q1 Plan'!A:1")
        // Named-cell references with the same spelling stay put (no bang).
        #expect(ReferenceRewriter.renamingSheet("='Budget' + Budget!A:1",
                                                from: "Budget", to: "Plan")
                == "='Budget' + Plan!A:1")
        // Other sheets' qualifiers are untouched; nil when nothing matched.
        #expect(ReferenceRewriter.renamingSheet("=Costs!A:1", from: "Budget", to: "Plan") == nil)
        #expect(ReferenceRewriter.renamingSheet("plain label", from: "Budget", to: "Plan") == nil)
    }
}
