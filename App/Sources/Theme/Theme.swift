import SwiftUI

/// A complete visual style, decodable from a JSON file so users can drop
/// custom themes into Application Support.
struct Theme: Codable, Identifiable, Hashable {
    var name: String
    var windowBackground: HexColor
    var inputBackground: HexColor
    var expressionText: HexColor
    var resultText: HexColor
    var errorText: HexColor
    var secondaryText: HexColor
    var accent: HexColor
    var fontName: String? // nil → system monospaced
    var fontSize: Double

    var id: String { name }

    func font(scale: Double = 1) -> Font {
        let size = fontSize * scale
        if let fontName {
            return .custom(fontName, size: size)
        }
        return .system(size: size, design: .monospaced)
    }
}

/// A Codable sRGB color in "#RRGGBB" or "#RRGGBBAA" form.
struct HexColor: Codable, Hashable {
    var red: Double
    var green: Double
    var blue: Double
    var alpha: Double

    var color: Color {
        Color(.sRGB, red: red, green: green, blue: blue, opacity: alpha)
    }

    init?(hex: String) {
        var text = hex.trimmingCharacters(in: .whitespaces)
        if text.hasPrefix("#") { text.removeFirst() }
        guard text.count == 6 || text.count == 8,
              let value = UInt64(text, radix: 16) else { return nil }
        let hasAlpha = text.count == 8
        let shift: (Int) -> Double = { Double((value >> $0) & 0xFF) / 255 }
        if hasAlpha {
            (red, green, blue, alpha) = (shift(24), shift(16), shift(8), shift(0))
        } else {
            (red, green, blue, alpha) = (shift(16), shift(8), shift(0), 1)
        }
    }

    init(from decoder: Decoder) throws {
        let text = try decoder.singleValueContainer().decode(String.self)
        guard let parsed = HexColor(hex: text) else {
            throw DecodingError.dataCorrupted(.init(
                codingPath: decoder.codingPath,
                debugDescription: "expected #RRGGBB or #RRGGBBAA, got '\(text)'"))
        }
        self = parsed
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        let component: (Double) -> String = { String(format: "%02X", Int(($0 * 255).rounded())) }
        var text = "#" + component(red) + component(green) + component(blue)
        if alpha < 1 { text += component(alpha) }
        try container.encode(text)
    }
}
