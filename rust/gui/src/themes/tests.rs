//! Tests for the theme catalog: its size and name/palette resolution.

use super::*;

#[test]
fn catalog_holds_the_ten_soroban_themes() {
    assert_eq!(catalog().len(), 10);
    assert_eq!(names().len(), 10);
    assert_eq!(default_name(), "Dracula");
}

#[test]
fn palette_resolves_by_name_and_falls_back() {
    // A named theme resolves to its own palette…
    let one_light = palette("One Light");
    assert_eq!(one_light.accent, Color::from_rgb8(0x40, 0x78, 0xf2));
    // …and an unknown name falls back to the default (Dracula).
    assert_eq!(palette("No Such Theme").bg, palette("Dracula").bg);
}
