import Anzan
/// Per-cell presentation: text style + number format. Display-only — the
/// underlying value stays exact; formulas, references, and TSV copy always
/// see the raw value. Stored sparsely on `Sheet.formats` (a default format
/// is pruned, never stored) and persisted per sheet in workbooks.

/// Semantic palette colors. Stored by NAME so the app can map them to system
/// colors that adapt to light/dark — per-cell absolute RGB would fight the
/// switchable themes.
public enum PaletteColor: String, Codable, Equatable, Sendable, CaseIterable {
    case red, orange, yellow, green, blue, purple, gray
}

public enum CellAlignment: String, Codable, Equatable, Sendable, CaseIterable {
    /// The grid's automatic rule: text left, numbers right, errors centered.
    case auto
    case left, center, right
}

/// How a numeric cell value renders. All rendering is pure string/BigInt
/// math — no Double, no Foundation NumberFormatter — so formatted display
/// stays as exact as the engine (a 40-digit value groups correctly).
public enum NumberFormat: Equatable, Sendable {
    case general
    /// Fixed decimals with thousands grouping: 1234567.5 → "1,234,567.50".
    case number(decimals: Int)
    /// "$1,234.50" / "-€2.00" — the symbol is stored, so a workbook renders
    /// identically on machines with different locales.
    case currency(symbol: String, decimals: Int)
    /// ×100 with a % sign — an exact exponent shift: 0.0825 → "8.25%".
    case percent(decimals: Int)
    /// Day serials (the engine's date representation) as "2026-06-06".
    case date
    /// Programmer display: integers as "0xC3" / "0b1100_0011". Display-only
    /// like everything here — the value stays an exact decimal, references
    /// see the number. Non-integers fall back to plain rendering.
    case hex
    case binary

    public static let decimalsRange = 0...12

    public func rendered(_ value: BigDecimal) -> String {
        switch self {
        case .general:
            return value.description
        case .number(let decimals):
            return Self.fixed(value, decimals: decimals)
        case .currency(let symbol, let decimals):
            let magnitude = value.isNegative ? -value : value
            return (value.isNegative ? "-" : "") + symbol
                + Self.fixed(magnitude, decimals: decimals)
        case .percent(let decimals):
            let scaled = BigDecimal(significand: value.significand,
                                    exponent: value.exponent + 2)
            return Self.fixed(scaled, decimals: decimals) + "%"
        case .date:
            guard let serial = value.rounded(toPlaces: 0).intValue else {
                return value.description // beyond any calendar — show the number
            }
            let date = CivilDate.civil(fromSerial: serial)
            return Self.padded(date.year, to: 4) + "-"
                + Self.padded(date.month, to: 2) + "-" + Self.padded(date.day, to: 2)
        case .hex:
            return value.hexText ?? value.description
        case .binary:
            return value.binaryText ?? value.description
        }
    }

    /// The format with `delta` more (or fewer) decimals — the menu's
    /// Increase/Decrease Decimals stepper. General steps into Number.
    public func adjustingDecimals(by delta: Int) -> NumberFormat {
        func clamped(_ d: Int) -> Int {
            min(max(d, Self.decimalsRange.lowerBound), Self.decimalsRange.upperBound)
        }
        switch self {
        case .general: return .number(decimals: clamped(2 + delta))
        case .number(let d): return .number(decimals: clamped(d + delta))
        case .currency(let symbol, let d): return .currency(symbol: symbol, decimals: clamped(d + delta))
        case .percent(let d): return .percent(decimals: clamped(d + delta))
        case .date: return .date
        case .hex: return .hex
        case .binary: return .binary
        }
    }

    /// Sign + grouped integer part + fraction padded/rounded to exactly
    /// `decimals` places (banker's, via `rounded(toPlaces:)`).
    private static func fixed(_ value: BigDecimal, decimals: Int) -> String {
        let rounded = value.rounded(toPlaces: decimals)
        let digits = String(rounded.significand.magnitude)
        let sign = rounded.isNegative ? "-" : ""

        var integer: String
        var fraction: String
        if rounded.exponent >= 0 {
            integer = digits + String(repeating: "0", count: rounded.exponent)
            fraction = ""
        } else {
            let pointPosition = digits.count + rounded.exponent
            if pointPosition <= 0 {
                integer = "0"
                fraction = String(repeating: "0", count: -pointPosition) + digits
            } else {
                let index = digits.index(digits.startIndex, offsetBy: pointPosition)
                integer = String(digits[..<index])
                fraction = String(digits[index...])
            }
        }
        if fraction.count < decimals {
            fraction += String(repeating: "0", count: decimals - fraction.count)
        }
        let grouped = Self.grouped(integer)
        return decimals > 0 ? "\(sign)\(grouped).\(fraction)" : "\(sign)\(grouped)"
    }

    /// "1234567" → "1,234,567".
    private static func grouped(_ integer: String) -> String {
        guard integer.count > 3 else { return integer }
        var out: [Character] = []
        for (offset, ch) in integer.reversed().enumerated() {
            if offset > 0, offset.isMultiple(of: 3) { out.append(",") }
            out.append(ch)
        }
        return String(out.reversed())
    }

    private static func padded(_ n: Int, to width: Int) -> String {
        let text = String(abs(n))
        let padded = text.count < width
            ? String(repeating: "0", count: width - text.count) + text : text
        return n < 0 ? "-" + padded : padded
    }
}

public struct CellFormat: Equatable, Sendable {
    public var bold = false
    public var italic = false
    public var underline = false
    public var strikethrough = false
    public var alignment: CellAlignment = .auto
    public var textColor: PaletteColor?
    public var fillColor: PaletteColor?
    public var numberFormat: NumberFormat = .general

    public init() {}

    /// Default formats are pruned from the sparse per-sheet map.
    public var isDefault: Bool { self == CellFormat() }
}

// MARK: - Codable (compact: only non-default fields are written)

extension CellFormat: Codable {
    private enum CodingKeys: String, CodingKey {
        case bold, italic, underline, strikethrough, alignment
        case textColor, fillColor
        case style, decimals, symbol // the flattened NumberFormat
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        bold = try container.decodeIfPresent(Bool.self, forKey: .bold) ?? false
        italic = try container.decodeIfPresent(Bool.self, forKey: .italic) ?? false
        underline = try container.decodeIfPresent(Bool.self, forKey: .underline) ?? false
        strikethrough = try container.decodeIfPresent(Bool.self, forKey: .strikethrough) ?? false
        alignment = try container.decodeIfPresent(CellAlignment.self, forKey: .alignment) ?? .auto
        textColor = try container.decodeIfPresent(PaletteColor.self, forKey: .textColor)
        fillColor = try container.decodeIfPresent(PaletteColor.self, forKey: .fillColor)

        let decimals = try container.decodeIfPresent(Int.self, forKey: .decimals) ?? 2
        switch try container.decodeIfPresent(String.self, forKey: .style) {
        case "number":
            numberFormat = .number(decimals: decimals)
        case "currency":
            numberFormat = .currency(
                symbol: try container.decodeIfPresent(String.self, forKey: .symbol) ?? "$",
                decimals: decimals)
        case "percent":
            numberFormat = .percent(decimals: decimals)
        case "date":
            numberFormat = .date
        case "hex":
            numberFormat = .hex
        case "binary":
            numberFormat = .binary
        default:
            numberFormat = .general // unknown styles from newer versions degrade safely
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        if bold { try container.encode(true, forKey: .bold) }
        if italic { try container.encode(true, forKey: .italic) }
        if underline { try container.encode(true, forKey: .underline) }
        if strikethrough { try container.encode(true, forKey: .strikethrough) }
        if alignment != .auto { try container.encode(alignment, forKey: .alignment) }
        try container.encodeIfPresent(textColor, forKey: .textColor)
        try container.encodeIfPresent(fillColor, forKey: .fillColor)

        switch numberFormat {
        case .general:
            break
        case .number(let decimals):
            try container.encode("number", forKey: .style)
            try container.encode(decimals, forKey: .decimals)
        case .currency(let symbol, let decimals):
            try container.encode("currency", forKey: .style)
            try container.encode(symbol, forKey: .symbol)
            try container.encode(decimals, forKey: .decimals)
        case .percent(let decimals):
            try container.encode("percent", forKey: .style)
            try container.encode(decimals, forKey: .decimals)
        case .date:
            try container.encode("date", forKey: .style)
        case .hex:
            try container.encode("hex", forKey: .style)
        case .binary:
            try container.encode("binary", forKey: .style)
        }
    }
}
