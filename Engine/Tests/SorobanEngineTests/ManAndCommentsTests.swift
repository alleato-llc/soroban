import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Comments and man()")
struct ManAndCommentsTests {
    @Test func commentsAreIgnoredInExpressions() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("1 + 2  # adding things").get() == .value(BigDecimal(3)))
        #expect(try calc.evaluate("100 * percent(8.25) # tax").get()
            == .value(BigDecimal(string: "8.25")!))
        // '#' anywhere ends the line.
        #expect(try calc.evaluate("6 * 7 # 1/0 never runs").get() == .value(BigDecimal(42)))
        // A comment-only line is now a first-class NOTE (not a parse error),
        // and it never touches `ans`.
        _ = calc.evaluate("99")
        #expect(try calc.evaluate("# just a note").get() == .comment("just a note"))
        #expect(calc.environment.ans == .number(BigDecimal(99)))
        // The trailing-comment / standalone-comment helpers split correctly,
        // respecting strings.
        #expect(Calculator.standaloneComment(in: "# a note") == "a note")
        #expect(Calculator.standaloneComment(in: "5 + 3 # adds") == nil)
        #expect(Calculator.trailingComment(in: "5 + 3 # adds") == "adds")
        #expect(Calculator.trailingComment(in: #"len("a # b")"#) == nil)
    }

    @Test func docCommentsDocumentUserFunctions() throws {
        let calc = Calculator()
        _ = calc.evaluate("tax(x) = x * 1.0825  # TX sales tax on a subtotal")

        let doc = try #require(calc.documentation(for: "tax"))
        #expect(doc.summary == "TX sales tax on a subtotal")
        #expect(doc.signature == "tax(x)")
        #expect(doc.examples == ["tax(x) = x * 1.0825  # TX sales tax on a subtotal"])

        // Redefining updates the documentation.
        _ = calc.evaluate("tax(x) = x * 1.05  # reduced rate")
        #expect(calc.documentation(for: "tax")?.summary == "reduced rate")

        // Undocumented functions get the gentle nudge.
        _ = calc.evaluate("f(x) = x")
        #expect(calc.documentation(for: "f")?.summary.contains("trailing comment") == true)
    }

    @Test func docCommentsSurviveWorkbookRoundTrip() throws {
        let calc = Calculator()
        _ = calc.evaluate("tax(x) = x * 1.0825 # TX sales tax")
        let workbook = Workbook(cells: [:], variables: [:],
                                functions: calc.environment.allUserFunctions)
        let decoded = try Workbook.decode(try workbook.encode())

        let fresh = Calculator()
        for source in decoded.functions.sorted() {
            _ = fresh.evaluate(source)
        }
        #expect(fresh.documentation(for: "tax")?.summary == "TX sales tax")
        #expect(try fresh.evaluate("tax(100)").get() == .value(BigDecimal(string: "108.25")!))
    }

    @Test func manPrintsDocumentation() throws {
        let calc = Calculator()
        guard case .success(.documentation(let doc)) = calc.evaluate("man(pmt)") else {
            Issue.record("expected documentation outcome")
            return
        }
        #expect(doc.name == "pmt")
        #expect(doc.signature.contains("pmt("))

        // help() is the same; lookups are case-insensitive; special forms work.
        #expect(calc.evaluate("help(SUM)").isFailure == false)
        guard case .success(.documentation(let ifDoc)) = calc.evaluate("man(if)") else {
            Issue.record("expected if docs")
            return
        }
        #expect(ifDoc.summary.contains("taken branch"))

        // The printed form is multi-line: signature, summary, examples.
        let text = try calc.evaluate("man(abs)").get().description
        #expect(text.contains("abs(x)"))
        #expect(text.contains("e.g."))
    }

    @Test func manCoversUserFunctionsAndFailsHelpfully() {
        let calc = Calculator()
        _ = calc.evaluate("tax(x) = x * 1.0825 # TX sales tax")
        guard case .success(.documentation(let doc)) = calc.evaluate("man(tax)") else {
            Issue.record("expected user function docs")
            return
        }
        #expect(doc.summary == "TX sales tax")

        guard case .failure(let error) = calc.evaluate("man(nope)") else {
            Issue.record("expected failure")
            return
        }
        #expect(error.description.contains("no documentation for 'nope'"))
    }

    @Test func manIsLogOnlyAndReserved() {
        let calc = Calculator()
        let sheet = Spreadsheet(calculator: calc)
        sheet.setCell("man(pmt)", at: CellAddress(column: 0, row: 0))
        guard case .error(let message) = sheet.displayValue(at: CellAddress(column: 0, row: 0)) else {
            Issue.record("expected cell error")
            return
        }
        #expect(message.contains("calculation log"))

        #expect(calc.evaluate("man = 5").isFailure)
        #expect(calc.evaluate("help(x) = x").isFailure)
        #expect(calc.evaluate("man(1 + 2)").isFailure) // names only
        // man() never touches ans.
        _ = calc.evaluate("42")
        _ = calc.evaluate("man(abs)")
        #expect(calc.environment.ans == .number(BigDecimal(42)))
    }
}
