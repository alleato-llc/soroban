import Anzan

/// The read-only `History` reflection API — the calculation log as an iterable
/// array of entry handles, so a LOG-LINE expression can inspect what came
/// before (`History[-1].value`, `sum(map(e -> e.value, History))`).
///
/// `History` is LOG-ONLY: the resolver hands back the array only on the log
/// path (`inLog`); in a CELL the name is simply unknown, so it degrades to a
/// text label (Anzan's unknownVariable → text rule) rather than erroring — a
/// cell may legitimately hold a header literally named "History". The reason
/// for the gate is reproducibility: the log is GLOBAL session state, not the
/// workbook, so a cell reading it wouldn't be reproducible or portable.
///
/// The host feeds the log through `LogSource` (host-neutral `LogRecord`s); the
/// App conforms `CalculatorSession`, and engine tests use a stub. Each entry's
/// `kind`/`referencesCells` are DERIVED here by parsing the stored input (a
/// function definition is logged as a value of its signature, so the outcome
/// alone can't classify it — the input parse can).

/// One log line, host-neutral. `value` is the typed result (number/string) when
/// the line produced one, nil otherwise (errors, comments, definitions).
public struct LogRecord: Sendable {
    public let input: String   // verbatim expression — intent + replay source
    public let text: String    // displayed outcome string — always present
    public let value: Value?   // typed value, only for value-producing lines
    public let isError: Bool
    public let isComment: Bool
    public let isInfo: Bool     // display-only output (man()/JSON/host dumps) — no value
    public let note: String

    public init(input: String, text: String, value: Value?,
                isError: Bool, isComment: Bool, isInfo: Bool = false, note: String) {
        self.input = input
        self.text = text
        self.value = value
        self.isError = isError
        self.isComment = isComment
        self.isInfo = isInfo
        self.note = note
    }
}

/// The host's read interface to the log — the "clean host API underneath".
public protocol LogSource: AnyObject {
    var logCount: Int { get }
    func logRecord(at index: Int) -> LogRecord?
}

enum HistoryReflection {
    /// Builds `History` — an array of entry handles, oldest → newest. Called by
    /// the resolver only on the log path; the gate lives there, not here.
    static func value(from source: LogSource) -> Value {
        var entries: [Value] = []
        entries.reserveCapacity(source.logCount)
        for index in 0..<source.logCount {
            if let record = source.logRecord(at: index) {
                entries.append(.host(HistoryEntryObject(record: record)))
            }
        }
        return .array(entries)
    }
}

/// One entry handle: `.input` / `.value` / `.text` / `.kind` / `.isError` /
/// `.referencesCells` / `.note`.
final class HistoryEntryObject: HostObject, @unchecked Sendable {
    let record: LogRecord

    init(record: LogRecord) { self.record = record }

    var typeName: String { "LogEntry" }
    var description: String { "LogEntry(\(record.input))" }

    func isEqual(to other: any HostObject) -> Bool {
        guard let other = other as? HistoryEntryObject else { return false }
        return record.input == other.record.input && record.text == other.record.text
    }

    func member(_ name: String) -> Value? {
        switch name {
        case "input": return .string(record.input)
        case "text": return .string(record.text)
        case "value": return record.value // nil for non-value lines → guard with .kind
        case "kind": return .string(kind)
        case "isError": return .bool(record.isError)
        case "referencesCells": return .bool(referencesCells)
        case "note": return .string(record.note)
        default: return nil
        }
    }

    /// The input parsed as an expression (nil if it doesn't parse — an error
    /// line, say). A leading `=` is tolerated like the log itself.
    private var parsed: Expression? {
        var line = record.input.trimmingCharacters(in: .whitespaces)
        if line.hasPrefix("=") { line = String(line.dropFirst()) }
        return try? Parser.parse(line)
    }

    /// "value" | "error" | "comment" | "info" | "function" | "datatype".
    /// Errors/comments/info come from the outcome flags; function/datatype need
    /// the input parse (a definition is logged as a value of its signature).
    /// "info" is display-only output (man()/JSON/host dumps) — `.value` is nil.
    var kind: String {
        if record.isError { return "error" }
        if record.isComment { return "comment" }
        if record.isInfo { return "info" } // man()/JSON/host dumps — display-only
        switch parsed {
        case .functionDefinition: return "function"
        case .dataDefinition: return "datatype"
        default: return "value"
        }
    }

    /// Provenance: did this line read a cell / named cell? (the "source" flag).
    var referencesCells: Bool {
        parsed?.containsCellReference ?? false
    }
}
