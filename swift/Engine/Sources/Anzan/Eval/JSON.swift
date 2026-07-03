// Both directions of JSON live here: `Value.jsonText` (the toJson()
// builtin) and `JSONParser` (fromJson()) — exact inverses for everything
// Anzan can represent.

// MARK: - Serializing (toJson)

extension Value {
    /// JSON text: numbers bare (BigDecimal's canonical text is valid JSON),
    /// strings escaped, arrays/maps/records as arrays/objects. Boolean-
    /// declared record fields come out as true/false — the type declaration
    /// is what makes that possible. Functions refuse.
    func jsonText(pretty: Bool, depth: Int = 0) throws -> String {
        switch self {
        case .number(let value):
            return value.description
        case .string(let text):
            return Self.jsonQuoted(text)
        case .array(let items):
            guard !items.isEmpty else { return "[]" }
            let rendered = try items.map { try $0.jsonText(pretty: pretty, depth: depth + 1) }
            return Self.joined(rendered, brackets: ("[", "]"), pretty: pretty, depth: depth)
        case .map(let entries):
            return try Self.jsonObject(entries, booleanFields: [], pretty: pretty, depth: depth)
        case .record(let record):
            return try Self.jsonObject(record.entries, booleanFields: record.booleanFields,
                                       pretty: pretty, depth: depth)
        case .fixedInt(let f):
            // A bounded integer is a JSON number (its exact value).
            return f.value.description
        case .fixedDecimal(let d):
            // A JSON number, kept at the declared scale (e.g. 10.50).
            return d.text
        case .function:
            throw EngineError.domainError(message: "toJson() can't serialize a function")
        case .host(let object):
            throw EngineError.domainError(message: "toJson() can't serialize a \(object.typeName)")
        }
    }

    private static func jsonObject(_ entries: [MapEntry], booleanFields: Set<String>,
                                   pretty: Bool, depth: Int) throws -> String {
        guard !entries.isEmpty else { return "{}" }
        let rendered = try entries.map { entry -> String in
            let value: String
            if booleanFields.contains(entry.key), case .number(let flag) = entry.value {
                value = flag.isZero ? "false" : "true"
            } else {
                value = try entry.value.jsonText(pretty: pretty, depth: depth + 1)
            }
            return jsonQuoted(entry.key) + (pretty ? ": " : ":") + value
        }
        return joined(rendered, brackets: ("{", "}"), pretty: pretty, depth: depth)
    }

    /// Compact packs everything; pretty is the conventional 2-space layout.
    private static func joined(_ parts: [String], brackets: (String, String),
                               pretty: Bool, depth: Int) -> String {
        guard pretty else {
            return brackets.0 + parts.joined(separator: ",") + brackets.1
        }
        let pad = String(repeating: "  ", count: depth + 1)
        return brackets.0 + "\n"
            + parts.map { pad + $0 }.joined(separator: ",\n")
            + "\n" + String(repeating: "  ", count: depth) + brackets.1
    }

    /// JSON string escaping — the JSON set, not the lexer's (adds \r and
    /// \u00XX for remaining control characters).
    private static func jsonQuoted(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\t": out += "\\t"
            case "\r": out += "\\r"
            default:
                if scalar.value < 0x20 {
                    let hex = String(scalar.value, radix: 16, uppercase: true)
                    out += "\\u" + String(repeating: "0", count: 4 - hex.count) + hex
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        return out + "\""
    }
}

// MARK: - Parsing (fromJson)

/// Parses JSON text into a `Value` — `jsonText`'s inverse for everything
/// Anzan can represent: objects → maps, arrays → arrays, strings → strings,
/// numbers → exact decimals, true/false → 1/0.
///
/// Hand-rolled on purpose: Foundation's JSONSerialization round-trips
/// numbers through Double, which is precisely the float drift this engine
/// exists to refuse. Number literals here go straight to
/// `BigDecimal(string:)` at full precision.
///
/// JSON `null` is refused — Anzan deliberately has no null (see
/// docs/ANZAN.md, Influences) and won't invent a coercion for it.
struct JSONParser {
    private let chars: [Character]
    private var index = 0
    private var depth = 0

    /// Nesting cap: honest data never comes close; a pathological input
    /// errors instead of chewing the parser's stack. Sized for the SMALLEST
    /// stack the parser runs on — Swift Testing's ~512 KB cooperative
    /// threads, where each nesting level costs two parser frames (256 was
    /// empirically a SIGBUS there; the depth-cap test now pins this).
    private static let maxDepth = 128

    private init(_ text: String) {
        chars = Array(text)
    }

    static func parse(_ text: String) throws -> Value {
        var parser = JSONParser(text)
        parser.skipWhitespace()
        let value = try parser.value()
        parser.skipWhitespace()
        guard parser.index == parser.chars.count else {
            throw parser.error("unexpected trailing content")
        }
        return value
    }

    // MARK: Scanning

    private var current: Character? {
        index < chars.count ? chars[index] : nil
    }

    private mutating func skipWhitespace() {
        while let c = current, c == " " || c == "\t" || c == "\n" || c == "\r" {
            index += 1
        }
    }

    private func error(_ message: String) -> EngineError {
        .domainError(message: "fromJson: \(message) at character \(index + 1)")
    }

    // MARK: Values

    private mutating func value() throws -> Value {
        depth += 1
        defer { depth -= 1 }
        guard depth <= Self.maxDepth else {
            throw error("nesting deeper than \(Self.maxDepth) levels")
        }

        switch current {
        case "{": return try object()
        case "[": return try array()
        case "\"": return .string(try string())
        case "t", "f", "n": return try keyword()
        case .some(let c) where c == "-" || c.isNumber: return try number()
        case .some: throw error("unexpected character '\(current!)'")
        case nil: throw error("unexpected end of JSON")
        }
    }

    private mutating func object() throws -> Value {
        index += 1 // '{'
        var entries: [Value.MapEntry] = []
        skipWhitespace()
        if current == "}" {
            index += 1
            return .map([])
        }
        while true {
            skipWhitespace()
            guard current == "\"" else {
                throw error("expected a quoted object key")
            }
            let key = try string()
            guard !entries.contains(where: { $0.key == key }) else {
                throw error("duplicate key \"\(key)\"")
            }
            skipWhitespace()
            guard current == ":" else {
                throw error("expected ':' after key \"\(key)\"")
            }
            index += 1
            skipWhitespace()
            entries.append(Value.MapEntry(key: key, value: try value()))
            skipWhitespace()
            switch current {
            case ",":
                index += 1
            case "}":
                index += 1
                return .map(entries)
            default:
                throw error("expected ',' or '}'")
            }
        }
    }

    private mutating func array() throws -> Value {
        index += 1 // '['
        var items: [Value] = []
        skipWhitespace()
        if current == "]" {
            index += 1
            return .array([])
        }
        while true {
            skipWhitespace()
            items.append(try value())
            skipWhitespace()
            switch current {
            case ",":
                index += 1
            case "]":
                index += 1
                return .array(items)
            default:
                throw error("expected ',' or ']'")
            }
        }
    }

    private mutating func keyword() throws -> Value {
        for (word, result) in [("true", Value.number(.one)), ("false", .number(.zero))]
        where chars[index...].starts(with: word) {
            index += word.count
            return result
        }
        if chars[index...].starts(with: "null") {
            throw error("JSON null has no Anzan value — remove it or default it before parsing")
        }
        throw error("unexpected character '\(current!)'")
    }

    /// JSON's number grammar, handed to BigDecimal at full precision.
    /// The leading sign is split off (the engine's number parser, like its
    /// lexer, treats signs as separate).
    private mutating func number() throws -> Value {
        let start = index
        var negative = false
        if current == "-" {
            negative = true
            index += 1
        }
        let digitsStart = index
        while let c = current, c.isNumber || c == "." || c == "e" || c == "E"
            || ((c == "+" || c == "-") && "eE".contains(chars[index - 1])) {
            index += 1
        }
        let text = String(chars[digitsStart..<index])
        guard !text.isEmpty, let magnitude = BigDecimal(string: text) else {
            index = start
            throw error("malformed number")
        }
        return .number(negative ? -magnitude : magnitude)
    }

    /// `"…"` with the full JSON escape set, including \uXXXX and surrogate
    /// pairs (which is why this scans rather than reusing the lexer).
    private mutating func string() throws -> String {
        index += 1 // opening quote
        var text = ""
        while let c = current {
            switch c {
            case "\"":
                index += 1
                return text
            case "\\":
                index += 1
                switch current {
                case "\"": text.append("\""); index += 1
                case "\\": text.append("\\"); index += 1
                case "/": text.append("/"); index += 1
                case "n": text.append("\n"); index += 1
                case "t": text.append("\t"); index += 1
                case "r": text.append("\r"); index += 1
                case "b": text.append("\u{8}"); index += 1
                case "f": text.append("\u{C}"); index += 1
                case "u":
                    index += 1
                    text.append(try unicodeEscape())
                case .some(let escaped):
                    throw error("unknown escape '\\\(escaped)'")
                case nil:
                    throw error("unterminated string")
                }
            default:
                text.append(c)
                index += 1
            }
        }
        throw error("unterminated string")
    }

    /// The 4 hex digits after `\u` — possibly the high half of a surrogate
    /// pair, in which case the matching `\uDC00–\uDFFF` must follow.
    private mutating func unicodeEscape() throws -> Character {
        let high = try hex4()
        if (0xD800...0xDBFF).contains(high) {
            guard current == "\\", index + 1 < chars.count, chars[index + 1] == "u" else {
                throw error("missing low surrogate after \\u escape")
            }
            index += 2
            let low = try hex4()
            guard (0xDC00...0xDFFF).contains(low) else {
                throw error("invalid low surrogate in \\u escape")
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
            return Character(UnicodeScalar(combined)!)
        }
        guard let scalar = UnicodeScalar(high) else {
            throw error("invalid \\u escape") // a lone low surrogate
        }
        return Character(scalar)
    }

    private mutating func hex4() throws -> Int {
        guard index + 4 <= chars.count,
              let code = Int(String(chars[index..<index + 4]), radix: 16) else {
            throw error("\\u needs 4 hex digits")
        }
        index += 4
        return code
    }
}
