import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Sheet-scoped definitions (λ / 𝑖 cells)")
struct SheetDefinitionTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func functionCellsRenderLambdaAndResolve() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("tax(x) = x * 1.0825  # TX sales tax", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 0)) == .definition("λ tax(x)"))

        // Callable from a formula on the same sheet…
        grid.setCell("=tax(100)", at: addr(0, 1))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(string: "108.25")!))
        // …and from the log while the sheet is active.
        #expect(try calc.evaluate("tax(200)").get() == .value(BigDecimal(string: "216.5")!))
    }

    @Test func variableCellsRenderItalicIAndResolve() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = 0.1", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 0)) == .definition("𝑖 rate"))

        grid.setCell("=100 * rate", at: addr(0, 1))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(10)))
        #expect(try calc.evaluate("rate * 2").get() == .value(BigDecimal(string: "0.2")!))
    }

    @Test func definitionsMayReadCellsAndTrackDependencies() throws {
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("100", at: addr(0, 0))                  // A:1
        grid.setCell("rate = A:1 / 1000", at: addr(0, 1))    // 𝑖 rate = 0.1
        grid.setCell("=50 * rate", at: addr(0, 2))           // reads rate → A:1
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(5)))

        // Changing A:1 must reach the formula THROUGH the definition.
        grid.setCell("200", at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(10)))

        // Function bodies read cells too (the user's headline case).
        grid.setCell("tax(x) = x * A:1", at: addr(1, 0))
        grid.setCell("=tax(2)", at: addr(1, 1))
        #expect(grid.displayValue(at: addr(1, 1)) == .value(BigDecimal(400)))
    }

    @Test func sameNameRedefinitionReachesReadersWithoutTheHammer() throws {
        // setCell's targeted carve-out: a 𝑖 cell redefining the SAME variable
        // (control commits land here) invalidates via the definition-read
        // edges, not invalidateEverything — readers must still update.
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("growth = 0.1", at: addr(0, 0))
        grid.setCell("margin = growth * 2", at: addr(1, 0))   // chained 𝑖
        grid.setCell("=margin * 100", at: addr(0, 1))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(20)))

        grid.setCell("growth = 0.3", at: addr(0, 0)) // same name → targeted path
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(60)))

        // RENAMING is not value-preserving: the reader's name goes unresolved
        // (the hammer path must still fire — no edge points at 'fee').
        grid.setCell("fee = 0.3", at: addr(0, 0))
        guard case .error(let message) = grid.displayValue(at: addr(0, 1)) else {
            Issue.record("reader of a renamed definition should error"); return
        }
        #expect(message.contains("growth"))
    }

    @Test func definitionsAreSheetScoped() throws {
        let (calc, store) = makeStore()
        store.activeSheet.grid.setCell("rate = 0.1", at: addr(0, 0))
        let second = try store.addSheet()
        second.grid.setCell("rate = 0.5", at: addr(0, 0))    // same name, own sheet
        second.grid.setCell("=10 * rate", at: addr(0, 1))
        #expect(second.grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(5)))

        store.sheets[0].grid.setCell("=10 * rate", at: addr(0, 1))
        #expect(store.sheets[0].grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(1)))

        // The log follows the active tab.
        store.activeIndex = 1
        #expect(try calc.evaluate("rate").get() == .value(BigDecimal(string: "0.5")!))
        store.activeIndex = 0
        #expect(try calc.evaluate("rate").get() == .value(BigDecimal(string: "0.1")!))
    }

    @Test func cellDefinedNamesAreImmutableFromTheLog() throws {
        let (calc, store) = makeStore()
        store.activeSheet.grid.setCell("rate = 0.1", at: addr(0, 2)) // A:3
        store.activeSheet.grid.setCell("tax(x) = x * 2", at: addr(1, 0))

        guard case .failure(let assignError) = calc.evaluate("rate = 0.2") else {
            Issue.record("assigning a cell-owned name should fail"); return
        }
        #expect("\(assignError)".contains("A:3"))

        guard case .failure(let defineError) = calc.evaluate("tax(x) = x") else {
            Issue.record("redefining a cell-owned function should fail"); return
        }
        #expect("\(defineError)".contains("edit that cell"))
    }

    @Test func shadowingAndPrecedence() throws {
        let (calc, store) = makeStore()
        // Sheet definitions shadow log globals…
        _ = try calc.evaluate("x = 1").get()
        store.activeSheet.grid.setCell("x = 2", at: addr(0, 0))
        #expect(try calc.evaluate("x").get() == .value(BigDecimal(2)))
        // …but parameters shadow sheet definitions.
        _ = try calc.evaluate("probe(x) = x + 10").get()
        #expect(try calc.evaluate("probe(5)").get() == .value(BigDecimal(15)))
        // Removing the cell un-shadows the log variable.
        store.activeSheet.grid.setCell("", at: addr(0, 0))
        #expect(try calc.evaluate("x").get() == .value(BigDecimal(1)))
    }

    @Test func duplicateAndBuiltinDefinitionsError() throws {
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = 0.1", at: addr(0, 0))           // A:1 — canonical
        grid.setCell("rate = 0.9", at: addr(0, 5))           // A:6 — duplicate
        #expect(grid.displayValue(at: addr(0, 0)) == .definition("𝑖 rate"))
        guard case .error(let message) = grid.displayValue(at: addr(0, 5)) else {
            Issue.record("duplicate definition should error"); return
        }
        #expect(message.contains("A:1"))

        grid.setCell("sum(x) = x", at: addr(1, 0))           // built-in collision
        guard case .error(let builtinMessage) = grid.displayValue(at: addr(1, 0)) else {
            Issue.record("built-in redefinition should error"); return
        }
        #expect(builtinMessage.contains("built-in"))
    }

    @Test func circularDefinitionsAreCaught() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("loop = loop + 1", at: addr(0, 0))      // self-reference
        guard case .failure(let error) = calc.evaluate("loop") else {
            Issue.record("self-referential definition should fail"); return
        }
        #expect("\(error)".contains("circular"))

        // Cell ↔ definition cycle: B:1 uses rate, rate reads B:1.
        grid.setCell("rate = B:1 / 10", at: addr(0, 1))
        grid.setCell("=rate * 2", at: addr(1, 0))            // B:1
        guard case .error(let cellMessage) = grid.displayValue(at: addr(1, 0)) else {
            Issue.record("cell↔definition cycle should error"); return
        }
        #expect(cellMessage.contains("circular"))
    }

    @Test func referencesToDefinitionCellsActLikeText() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = 0.1", at: addr(0, 0))           // A:1
        grid.setCell("5", at: addr(0, 1))                    // A:2
        // Direct numeric use errors; ranges skip the definition cell.
        #expect(calc.evaluate("A:1 + 1").isFailure)
        #expect(try calc.evaluate("sum(A:1..A:2)").get() == .value(BigDecimal(5)))
    }

    @Test func definitionsPersistThroughWorkbooks() throws {
        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("tax(x) = x * 1.1  # with margin", at: addr(0, 0))
        grid.setCell("rate = 0.07", at: addr(0, 1))
        grid.setCell("=tax(100) + rate", at: addr(0, 2))

        // Definitions are just cell raws — round-trip through the codec.
        let workbook = Workbook(cells: ["A:1": grid.raw(at: addr(0, 0)),
                                        "A:2": grid.raw(at: addr(0, 1)),
                                        "A:3": grid.raw(at: addr(0, 2))],
                                variables: [:])
        let decoded = try Workbook.decode(try workbook.encode())

        let (_, fresh) = makeStore()
        var contents: [CellAddress: String] = [:]
        for (key, raw) in decoded.cells {
            contents[CellAddress(key: key)!] = raw
        }
        fresh.activeSheet.grid.load(contents)
        #expect(fresh.activeSheet.grid.displayValue(at: addr(0, 2))
                == .value(BigDecimal(string: "110.07")!))
        #expect(fresh.activeSheet.grid.displayValue(at: addr(0, 0))
                == .definition("λ tax(x)"))
    }

    @Test func cellDefinedFunctionsAreValues() throws {
        let (calc, store) = makeStore()
        store.activeSheet.grid.setCell("double(x) = x * 2", at: addr(0, 0))
        // Bare-name reference to a λ cell works in higher-order functions.
        #expect(try calc.evaluate("map(double, [1, 2, 3])").get().description == "[2, 4, 6]")
    }

    @Test func manFindsCellDefinedFunctions() throws {
        let (calc, store) = makeStore()
        store.activeSheet.grid.setCell("tax(x) = x * 1.0825  # TX sales tax", at: addr(0, 0))
        let doc = try #require(calc.documentation(for: "tax"))
        #expect(doc.summary == "TX sales tax")
        guard case .success(.documentation) = calc.evaluate("man tax") else {
            Issue.record("man should find λ cells"); return
        }
    }
}
