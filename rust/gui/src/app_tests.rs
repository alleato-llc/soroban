//! Unit tests for the crate root (`main.rs`): selection movement, the font
//! picker, and font-size zoom clamping. Kept in a sibling file rather than an
//! inline `#[cfg(test)] mod tests` block (house style — tests out of source).

use super::*;
use crate::render::next_selection;
use soroban_gui::session::{GRID_COLS, GRID_ROWS};

#[test]
fn move_relocates_single_cell() {
    let next = next_selection(GridSelection::cell(4, 2), 1, 0, false);
    assert_eq!(next.anchor, (5, 2));
    assert_eq!(next.extent, (5, 2));
}

#[test]
fn move_clamps_at_the_top_left_corner() {
    let next = next_selection(GridSelection::cell(0, 0), -1, -1, false);
    assert_eq!(next.anchor, (0, 0));
}

#[test]
fn move_clamps_at_the_bottom_right_corner() {
    let start = GridSelection::cell(GRID_ROWS - 1, GRID_COLS - 1);
    let next = next_selection(start, 1, 1, false);
    assert_eq!(next.anchor, (GRID_ROWS - 1, GRID_COLS - 1));
}

#[test]
fn extend_holds_the_anchor_and_moves_the_extent() {
    let start = GridSelection::cell(4, 2);
    let next = next_selection(start, 2, 1, true);
    assert_eq!(next.anchor, (4, 2));
    assert_eq!(next.extent, (6, 3));
}

#[test]
fn font_choices_lead_with_system_then_bundled_then_platform() {
    let names: Vec<&str> = font_choices().iter().map(|(n, _)| *n).collect();
    assert_eq!(names[0], "System");
    for family in BUNDLED_FAMILIES {
        assert!(
            names.contains(&family),
            "bundled {family} missing from picker"
        );
    }
    for &family in SYSTEM_FAMILIES {
        assert!(
            names.contains(&family),
            "system {family} missing from picker"
        );
    }
    // System + 5 bundled + the curated per-OS system list.
    assert_eq!(
        names.len(),
        1 + BUNDLED_FAMILIES.len() + SYSTEM_FAMILIES.len()
    );
}

#[test]
fn font_for_resolves_bundled_and_falls_back() {
    // A bundled family resolves to a named font, an unknown name to MONO.
    assert_eq!(font_for("Fira Mono"), Font::with_name("Fira Mono"));
    assert_eq!(font_for("No Such Font 12345"), MONO);
}

#[test]
fn examples_menu_is_last_with_one_submenu_per_category() {
    use rime::widgets::MenuItem;
    let app = App::default();
    let menus = app.menus();
    let examples = menus.last().expect("a menu bar");
    assert_eq!(examples.title, "Examples");
    assert_eq!(examples.items.len(), crate::examples::CATEGORIES.len());
    for (item, (name, expressions)) in examples.items.iter().zip(crate::examples::CATEGORIES) {
        match item {
            MenuItem::Submenu { label, items, .. } => {
                assert_eq!(label, name);
                assert_eq!(items.len(), expressions.len());
            }
            _ => panic!("category {name} is not a submenu"),
        }
    }
}

#[test]
fn use_example_shows_the_log_and_fills_the_input_without_evaluating() {
    let mut app = App {
        mode: ViewMode::Grid,
        ..App::default()
    };
    let entries_before = app.session.entries().len();
    let _ = app.update(Message::UseExample("sqrt(3^2 + 4^2)"));
    assert!(app.mode == ViewMode::Log);
    assert_eq!(app.session.input(), "sqrt(3^2 + 4^2)");
    // Not evaluated — the user presses Enter.
    assert_eq!(app.session.entries().len(), entries_before);
}

#[test]
fn hovering_an_examples_category_expands_it_without_closing_the_menu() {
    let mut app = App::default();
    let _ = app.update(Message::ToggleMenu(Some(4))); // open Examples
    let _ = app.update(Message::HoverExampleCategory(Some(2)));
    assert_eq!(app.menu_open, Some(4));
    assert_eq!(app.examples_submenu, Some(2));
    // Closing (or switching) the menu retracts the flyout.
    let _ = app.update(Message::ToggleMenu(None));
    assert_eq!(app.examples_submenu, None);
}

#[test]
fn zoom_font_is_clamped_to_the_slider_range() {
    let mut app = App {
        font_size: 27.0,
        ..App::default()
    };
    let _ = app.update(Message::ZoomFont(1.0)); // 28
    assert_eq!(app.font_size, 28.0);
    let _ = app.update(Message::ZoomFont(1.0)); // clamps at 28
    assert_eq!(app.font_size, 28.0);
    app.font_size = 9.0;
    let _ = app.update(Message::ZoomFont(-1.0)); // clamps at 9
    assert_eq!(app.font_size, 9.0);
    let _ = app.update(Message::ResetFontSize);
    assert_eq!(app.font_size, 14.0);
}
