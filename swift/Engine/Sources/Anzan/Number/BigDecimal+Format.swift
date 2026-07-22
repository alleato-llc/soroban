/// Thousands-grouped rendering. All of it is pure string/BigInt math — no
/// Double, no Foundation NumberFormatter — so a 40-digit value groups exactly.
///
/// This lives in `Anzan` (not the hosting layer) because finance-mode literals
/// echo their own grouping: `138,561 * 9%` answers `12,470.49`. The sheet's
/// `NumberFormat` renders through the same helpers, so a formatted cell and a
/// finance-mode result can never drift apart.
extension BigDecimal {
    /// "1234567" → "1,234,567". Takes the bare digits of an integer part.
    public static func grouping(_ integer: String) -> String {
        guard integer.count > 3 else { return integer }
        var out: [Character] = []
        for (offset, ch) in integer.reversed().enumerated() {
            if offset > 0, offset.isMultiple(of: 3) { out.append(",") }
            out.append(ch)
        }
        return String(out.reversed())
    }

    /// Sign + grouped integer part + fraction padded/rounded to exactly
    /// `decimals` places (banker's, via `rounded(toPlaces:)`).
    public func groupedText(decimals: Int) -> String {
        let rounded = self.rounded(toPlaces: decimals)
        let (sign, integer, rawFraction) = rounded.parts
        var fraction = rawFraction
        if fraction.count < decimals {
            fraction += String(repeating: "0", count: decimals - fraction.count)
        }
        let grouped = Self.grouping(integer)
        return decimals > 0 ? "\(sign)\(grouped).\(fraction)" : "\(sign)\(grouped)"
    }

    /// Grouped at the value's OWN number of decimals — no padding, no rounding.
    /// `138561` → "138,561"; `12470.49` → "12,470.49". Scientific-notation
    /// values (past `description`'s threshold) pass through ungrouped, since
    /// there is no integer run to group.
    public var groupedText: String {
        let plain = description
        guard !plain.contains("e"), !plain.contains("E") else { return plain }
        let (sign, integer, fraction) = parts
        let grouped = Self.grouping(integer)
        return fraction.isEmpty ? "\(sign)\(grouped)" : "\(sign)\(grouped).\(fraction)"
    }

    /// Splits into sign, bare integer digits, and bare fraction digits.
    private var parts: (sign: String, integer: String, fraction: String) {
        let digits = significand.magnitude.description
        let sign = isNegative ? "-" : ""
        if exponent >= 0 {
            return (sign, digits + String(repeating: "0", count: exponent), "")
        }
        let pointPosition = digits.count + exponent
        if pointPosition <= 0 {
            return (sign, "0", String(repeating: "0", count: -pointPosition) + digits)
        }
        let index = digits.index(digits.startIndex, offsetBy: pointPosition)
        return (sign, String(digits[..<index]), String(digits[index...]))
    }
}
