import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

/// The read-only `History` reflection API — the calculation log as an iterable
/// array of entry handles. Log-only: live on the log path, a text label in a
/// cell. Entry `kind`/`referencesCells` are derived from the stored input.
@Suite("History reflection")
struct HistoryReflectionTests {
    /// A stand-in log so the engine suite can exercise History without the app.
    final class StubLog: LogSource {
        var records: [LogRecord]
        init(_ records: [LogRecord]) { self.records = records }
        var logCount: Int { records.count }
        func logRecord(at index: Int) -> LogRecord? {
            records.indices.contains(index) ? records[index] : nil
        }
    }

    /// Keeps the store (resolver captures it weakly) and log alive for a test.
    final class Harness {
        let calc = Calculator()
        let store: SheetStore
        let log: StubLog
        init(_ records: [LogRecord]) {
            log = StubLog(records)
            store = SheetStore(calculator: calc)
            store.logSource = log
        }
    }

    private func num(_ value: Int) -> Value { .number(BigDecimal(value)) }

    private func record(_ input: String, value: Value? = nil, text: String? = nil,
                        isError: Bool = false, isComment: Bool = false, isInfo: Bool = false,
                        note: String = "") -> LogRecord {
        LogRecord(input: input, text: text ?? input, value: value,
                  isError: isError, isComment: isComment, isInfo: isInfo, note: note)
    }

    // MARK: The array shape

    @Test func historyIsAnIterableIndexableArray() throws {
        let h = Harness([
            record("1", value: num(1)),
            record("ans + 1", value: num(2)),
            record("ans * 43", value: num(86)),
        ])
        #expect(try h.calc.evaluate("len(History)").get() == .value(BigDecimal(3)))
        #expect(try h.calc.evaluate("History[0].value").get() == .value(BigDecimal(1)))
        #expect(try h.calc.evaluate("History[2].value").get() == .value(BigDecimal(86)))
        // Last entry — "ans, but as an entry". Plain arrays are 0-based with no
        // negative indexing, so use the existing last()/first() builtins.
        #expect(try h.calc.evaluate("last(History).input").get() == .value(.string("ans * 43")))
        #expect(try h.calc.evaluate("first(History).value").get() == .value(BigDecimal(1)))
        // Iterable: map/filter/reduce work because it's a real array. (`e` is
        // reserved — Euler's constant — so the lambda parameter is `entry`.)
        #expect(try h.calc.evaluate("sum(map(entry -> entry.value, History))").get()
            == .value(BigDecimal(89)))
    }

    // MARK: Entry fields

    @Test func entryExposesInputValueTextAndNote() throws {
        // 8% logs the value 0.08; compare it inside Anzan to dodge Swift-side
        // BigDecimal-literal construction.
        let h = Harness([
            record("8%", value: .number(BigDecimal(significand: 8, exponent: -2)),
                   text: "0.08", note: "tax"),
        ])
        #expect(try h.calc.evaluate("History[0].input").get() == .value(.string("8%")))
        #expect(try h.calc.evaluate("History[0].value == 0.08").get() == .value(BigDecimal(1)))
        #expect(try h.calc.evaluate("History[0].text").get() == .value(.string("0.08")))
        #expect(try h.calc.evaluate("History[0].note").get() == .value(.string("tax")))
    }

    @Test func stringResultsCarryTheirType() throws {
        let h = Harness([record(#"="Q" + 1"#, value: .string("Q1"))])
        #expect(try h.calc.evaluate("History[0].value").get() == .value(.string("Q1")))
    }

    // MARK: kind derivation (input-parse + outcome flags)

    @Test func kindClassifiesEachLine() throws {
        let h = Harness([
            record("100 + 8", value: num(108)),                       // value
            record("1 / 0", text: "#ERR", isError: true),             // error
            record("# a note", text: "a note", isComment: true),      // comment
            record("f(x) = x^2", value: .string("f(x)")),             // function (logged as a value!)
            record("data Point { x: Number, y: Number }"),            // datatype
            record("History", text: "[LogEntry(1)]", isInfo: true),  // display-only (host dump)
        ])
        #expect(try h.calc.evaluate("History[0].kind").get() == .value(.string("value")))
        #expect(try h.calc.evaluate("History[1].kind").get() == .value(.string("error")))
        #expect(try h.calc.evaluate("History[2].kind").get() == .value(.string("comment")))
        #expect(try h.calc.evaluate("History[3].kind").get() == .value(.string("function")))
        #expect(try h.calc.evaluate("History[4].kind").get() == .value(.string("datatype")))
        #expect(try h.calc.evaluate("History[5].kind").get() == .value(.string("info")))
        // isError is sugar for kind == "error".
        #expect(try h.calc.evaluate("History[1].isError").get() == .value(BigDecimal(1)))
        #expect(try h.calc.evaluate("History[0].isError").get() == .value(BigDecimal(0)))
    }

    // MARK: referencesCells provenance

    @Test func referencesCellsFlagsWorkbookDependence() throws {
        let h = Harness([
            record("A:1 + 10", value: num(15)),
            record("2 + 2", value: num(4)),
            record("'Projected Rate' * 2", value: num(8)),
        ])
        #expect(try h.calc.evaluate("History[0].referencesCells").get() == .value(BigDecimal(1)))
        #expect(try h.calc.evaluate("History[1].referencesCells").get() == .value(BigDecimal(0)))
        // Named-cell references count too.
        #expect(try h.calc.evaluate("History[2].referencesCells").get() == .value(BigDecimal(1)))
    }

    // MARK: containsHost (why a host result is logged display-only)

    @Test func containsHostDetectsReflectionHandles() {
        let handle = Value.host(HistoryEntryObject(
            record: record("x", value: num(1))))
        #expect(num(1).containsHost == false)
        #expect(Value.array([num(1), num(2)]).containsHost == false)
        #expect(Value.string("[LogEntry(x)]").containsHost == false) // a plain string isn't a host
        #expect(handle.containsHost == true)
        // A handle nested anywhere → true (this is the `History` dump case).
        #expect(Value.array([num(1), handle]).containsHost == true)
    }

    // MARK: The log-only gate

    @Test func historyIsLogOnly() throws {
        let h = Harness([record("1", value: num(1))])
        // On the log path the array resolves; in a cell it's nil (→ text label),
        // while Workbook reflection stays available in both.
        #expect(h.calc.hostValueResolver?("History", true) != nil)
        #expect(h.calc.hostValueResolver?("History", false) == nil)
        #expect(h.calc.hostValueResolver?("Workbook", false) != nil)
    }

    @Test func historyUnknownWithoutALogSource() throws {
        // No logSource wired → History is simply unknown, even on the log path.
        let calc = Calculator()
        let store = SheetStore(calculator: calc)
        #expect(calc.hostValueResolver?("History", true) == nil)
        withExtendedLifetime(store) {}
    }
}
