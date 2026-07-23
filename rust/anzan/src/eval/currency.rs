//! A currency the language knows — the tag on a `Money` value. A closed,
//! curated set (not "any Unicode currency glyph"), so an amount always names a
//! real currency and the canonical `Money(v, "USD")` form round-trips by code.
//! See docs/MODES.md.

/// The closed set of currencies the language understands. A peer of the
/// `Currency` Swift enum — the same ten, in the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Usd,
    Eur,
    Gbp,
    Jpy,
    Cny,
    Inr,
    Krw,
    Rub,
    Chf,
    Btc,
}

impl Currency {
    /// Every currency, in declaration order — for the "use one of …" hint.
    pub const ALL: [Currency; 10] = [
        Self::Usd,
        Self::Eur,
        Self::Gbp,
        Self::Jpy,
        Self::Cny,
        Self::Inr,
        Self::Krw,
        Self::Rub,
        Self::Chf,
        Self::Btc,
    ];

    /// The ISO-ish code used in the canonical constructor form and by
    /// `Money(value, "USD")` — uppercase, e.g. "USD".
    pub fn code(self) -> &'static str {
        match self {
            Self::Usd => "USD",
            Self::Eur => "EUR",
            Self::Gbp => "GBP",
            Self::Jpy => "JPY",
            Self::Cny => "CNY",
            Self::Inr => "INR",
            Self::Krw => "KRW",
            Self::Rub => "RUB",
            Self::Chf => "CHF",
            Self::Btc => "BTC",
        }
    }

    /// The glyph a currency amount displays with (`$10.00`, `CHF 10.00`). Two
    /// currencies (CNY, CHF) have no unambiguous single glyph, so they show a
    /// disambiguated prefix and are reachable only through the constructor.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Usd => "$",
            Self::Eur => "€",
            Self::Gbp => "£",
            Self::Jpy => "¥",
            Self::Cny => "CN¥",
            Self::Inr => "₹",
            Self::Krw => "₩",
            Self::Rub => "₽",
            Self::Chf => "CHF ",
            Self::Btc => "₿",
        }
    }

    /// The currency a leading glyph denotes, or `None` if it isn't a supported
    /// currency symbol. Ambiguous glyphs resolve canonically: `$`→USD (not
    /// CAD/AUD), `¥`→JPY (not CNY). Fullwidth ASCII forms (`＄￥￡`) normalize to
    /// their base. CNY/CHF have no glyph — they're constructor-only.
    pub fn from_glyph(glyph: char) -> Option<Self> {
        match glyph {
            '$' | '＄' => Some(Self::Usd),
            '€' => Some(Self::Eur),
            '£' | '￡' => Some(Self::Gbp),
            '¥' | '￥' => Some(Self::Jpy),
            '₹' => Some(Self::Inr),
            '₩' => Some(Self::Krw),
            '₽' => Some(Self::Rub),
            '₿' => Some(Self::Btc),
            _ => None,
        }
    }

    /// The currency for an ISO code (case-insensitive) — the `Money(v, "usd")`
    /// constructor path; `None` for an unknown code.
    pub fn from_code(code: &str) -> Option<Self> {
        match code.to_lowercase().as_str() {
            "usd" => Some(Self::Usd),
            "eur" => Some(Self::Eur),
            "gbp" => Some(Self::Gbp),
            "jpy" => Some(Self::Jpy),
            "cny" => Some(Self::Cny),
            "inr" => Some(Self::Inr),
            "krw" => Some(Self::Krw),
            "rub" => Some(Self::Rub),
            "chf" => Some(Self::Chf),
            "btc" => Some(Self::Btc),
            _ => None,
        }
    }
}
