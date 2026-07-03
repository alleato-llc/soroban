//! Presentational dialects (docs/MODES.md). A mode changes which glyphs you
//! type and read — never what is stored or computed. Canonical (`Normal`) is
//! the only form persisted or transported.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguageMode {
    #[default]
    Normal,
    Programmer,
    Finance,
}

impl LanguageMode {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "programmer" => Some(Self::Programmer),
            "finance" => Some(Self::Finance),
            _ => None,
        }
    }
}
