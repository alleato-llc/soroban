//! The Soroban theme catalog — the ten named palettes the AppKit app ships,
//! ported to rime's nine-token [`Palette`]. Six of them (Dracula, GitHub,
//! Gruvbox Dark, Nord, Solarized Dark/Light) rime already ships tuned, so we
//! reuse those; the remaining four (One Light, Soroban Dark/Light, Terminal
//! Green) are defined here from `swift/App/Resources/Themes/*.json`.
//!
//! The Swift themes carry seven colors; rime wants nine. The map is
//! `windowBackground→bg`, `inputBackground→surface`, `resultText→ink`,
//! `secondaryText→muted`, `accent→accent`, `errorText→danger`; `hairline`,
//! `success`, and `warn` have no Swift source, so they're chosen to fit.

use iced::Color;
use rime::theme::{self, Palette};

/// One Light — the Atom One Light editor theme.
const ONE_LIGHT: Palette = Palette {
    bg: Color::from_rgb8(0xfa, 0xfa, 0xfa),
    surface: Color::from_rgb8(0xea, 0xea, 0xeb),
    ink: Color::from_rgb8(0x38, 0x3a, 0x42),
    muted: Color::from_rgb8(0xa0, 0xa1, 0xa7),
    hairline: Color::from_rgb8(0xd3, 0xd3, 0xd6),
    accent: Color::from_rgb8(0x40, 0x78, 0xf2),
    success: Color::from_rgb8(0x50, 0xa1, 0x4f),
    warn: Color::from_rgb8(0xc1, 0x84, 0x01),
    danger: Color::from_rgb8(0xe4, 0x56, 0x49),
};

/// Soroban Dark — the app's own dark theme (a Tokyo-Night-leaning blue).
const SOROBAN_DARK: Palette = Palette {
    bg: Color::from_rgb8(0x1e, 0x1e, 0x28),
    surface: Color::from_rgb8(0x2a, 0x2a, 0x38),
    ink: Color::from_rgb8(0xe6, 0xe6, 0xf0),
    muted: Color::from_rgb8(0x6c, 0x70, 0x86),
    hairline: Color::from_rgb8(0x3a, 0x3a, 0x48),
    accent: Color::from_rgb8(0x7a, 0xa2, 0xf7),
    success: Color::from_rgb8(0x9e, 0xce, 0x6a),
    warn: Color::from_rgb8(0xe0, 0xaf, 0x68),
    danger: Color::from_rgb8(0xff, 0x6b, 0x6b),
};

/// Soroban Light — the app's own light theme.
const SOROBAN_LIGHT: Palette = Palette {
    bg: Color::from_rgb8(0xfa, 0xfa, 0xf7),
    surface: Color::from_rgb8(0xff, 0xff, 0xff),
    ink: Color::from_rgb8(0x1c, 0x1e, 0x21),
    muted: Color::from_rgb8(0x9a, 0x9f, 0xa6),
    hairline: Color::from_rgb8(0xe2, 0xe2, 0xde),
    accent: Color::from_rgb8(0x3b, 0x6e, 0xa8),
    success: Color::from_rgb8(0x2e, 0x7d, 0x32),
    warn: Color::from_rgb8(0xb7, 0x79, 0x1f),
    danger: Color::from_rgb8(0xc4, 0x1e, 0x3a),
};

/// Terminal Green — a monochrome green CRT look.
const TERMINAL_GREEN: Palette = Palette {
    bg: Color::from_rgb8(0x0c, 0x0c, 0x0c),
    surface: Color::from_rgb8(0x16, 0x16, 0x16),
    ink: Color::from_rgb8(0x55, 0xff, 0x55),
    muted: Color::from_rgb8(0x1f, 0x7a, 0x1f),
    hairline: Color::from_rgb8(0x1a, 0x3a, 0x1a),
    accent: Color::from_rgb8(0x33, 0xaa, 0x33),
    success: Color::from_rgb8(0x55, 0xff, 0x55),
    warn: Color::from_rgb8(0xaa, 0xaa, 0x33),
    danger: Color::from_rgb8(0xff, 0x55, 0x55),
};

/// The catalog, in the AppKit app's order: `(name, palette, is_dark)`.
pub fn catalog() -> &'static [(&'static str, Palette, bool)] {
    &[
        ("Dracula", theme::DRACULA, true),
        ("GitHub Light", theme::GITHUB, false),
        ("Gruvbox Dark", theme::GRUVBOX_DARK, true),
        ("Nord", theme::NORD, true),
        ("One Light", ONE_LIGHT, false),
        ("Solarized Dark", theme::SOLARIZED_DARK, true),
        ("Solarized Light", theme::SOLARIZED_LIGHT, false),
        ("Soroban Dark", SOROBAN_DARK, true),
        ("Soroban Light", SOROBAN_LIGHT, false),
        ("Terminal Green", TERMINAL_GREEN, true),
    ]
}

/// The default theme's name — the first in the catalog.
pub fn default_name() -> &'static str {
    catalog()[0].0
}

/// Look up a palette by name; falls back to the default theme for an unknown
/// name (e.g. a persisted name that no longer exists).
pub fn palette(name: &str) -> Palette {
    catalog()
        .iter()
        .find(|(candidate, _, _)| *candidate == name)
        .map(|(_, palette, _)| *palette)
        .unwrap_or(catalog()[0].1)
}

/// The catalog names, for a picker.
pub fn names() -> Vec<String> {
    catalog()
        .iter()
        .map(|(name, _, _)| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests;
