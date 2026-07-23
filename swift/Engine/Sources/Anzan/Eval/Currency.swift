/// A currency the language knows — the tag on a `Money` value. A closed,
/// curated set (not "any Unicode currency glyph"), so an amount always names a
/// real currency and the canonical `Money(v, "USD")` form round-trips by code.
/// See docs/MODES.md.
public enum Currency: String, Sendable, CaseIterable {
    case usd, eur, gbp, jpy, cny, inr, krw, rub, chf, btc

    /// The ISO-ish code used in the canonical constructor form and by
    /// `Money(value, "USD")` — uppercase, e.g. "USD".
    public var code: String { rawValue.uppercased() }

    /// The glyph a currency amount displays with (`$10.00`, `CHF 10.00`). Two
    /// currencies (CNY, CHF) have no unambiguous single glyph, so they show a
    /// disambiguated prefix and are reachable only through the constructor.
    public var symbol: String {
        switch self {
        case .usd: return "$"
        case .eur: return "€"
        case .gbp: return "£"
        case .jpy: return "¥"
        case .cny: return "CN¥"
        case .inr: return "₹"
        case .krw: return "₩"
        case .rub: return "₽"
        case .chf: return "CHF "
        case .btc: return "₿"
        }
    }

    /// The currency a leading glyph denotes, or nil if it isn't a supported
    /// currency symbol. Ambiguous glyphs resolve canonically: `$`→USD (not
    /// CAD/AUD), `¥`→JPY (not CNY). Fullwidth ASCII forms (`＄￥￡`) normalize to
    /// their base. CNY/CHF have no glyph — they're constructor-only.
    public static func fromGlyph(_ glyph: Character) -> Currency? {
        switch glyph {
        case "$", "＄": return .usd
        case "€": return .eur
        case "£", "￡": return .gbp
        case "¥", "￥": return .jpy
        case "₹": return .inr
        case "₩": return .krw
        case "₽": return .rub
        case "₿": return .btc
        default: return nil
        }
    }

    /// The currency for an ISO code (case-insensitive) — the `Money(v, "usd")`
    /// constructor path; nil for an unknown code.
    public static func fromCode(_ code: String) -> Currency? {
        Currency(rawValue: code.lowercased())
    }
}
