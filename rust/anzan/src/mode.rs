//! Presentational dialects (docs/MODES.md). A mode changes which glyphs you
//! type and read — never what is stored or computed. Canonical (`Normal`) is
//! the only form persisted or transported.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguageMode {
    #[default]
    Normal,
    Programmer,
    /// Grammatically identical to `Normal`; changes only how a plain NUMERIC
    /// result ECHOES — scientific notation (`2.46912e5`), or the engineering
    /// variant (exponent snapped to a multiple of 3) via `ScientificStyle`.
    /// Value-carried display (Money, grouping) still wins.
    Scientific,
}

impl LanguageMode {
    /// The mode's spelled name — ":mode" echoes and persistence.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Programmer => "programmer",
            Self::Scientific => "scientific",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "programmer" => Some(Self::Programmer),
            "scientific" => Some(Self::Scientific),
            _ => None,
        }
    }
}

/// The scientific-mode echo variant: plain SCI (`2.46912e5`) or ENG
/// (`246.912e3` — the exponent snapped to a multiple of 3). A display style,
/// not a mode: one `Scientific` dialect, two notations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScientificStyle {
    #[default]
    Sci,
    Eng,
}

impl ScientificStyle {
    /// The style's spelled name — `:mode scientific eng` and persistence.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Sci => "sci",
            Self::Eng => "eng",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "sci" => Some(Self::Sci),
            "eng" => Some(Self::Eng),
            _ => None,
        }
    }
}
