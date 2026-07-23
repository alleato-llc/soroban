//! Unit tests for the Examples menu data and label truncation. Kept in a
//! sibling file rather than an inline `#[cfg(test)] mod tests` block (house
//! style — tests out of source).

use super::*;

#[test]
fn showcase_leads_and_every_category_has_examples() {
    // The Showcase group (the Cash namespace one-liner) leads, mirroring
    // CalculatorSession.welcomeCategories.
    assert_eq!(CATEGORIES.first().map(|(name, _)| *name), Some("Showcase"));
    for (name, examples) in CATEGORIES {
        assert!(!examples.is_empty(), "category {name} has no examples");
    }
}

#[test]
fn short_labels_pass_through_unchanged() {
    assert_eq!(menu_label("sqrt(3^2 + 4^2)"), "sqrt(3^2 + 4^2)");
    // Exactly at the limit — still untruncated.
    let exact: String = "x".repeat(LABEL_MAX);
    assert_eq!(menu_label(&exact), exact);
}

#[test]
fn long_labels_truncate_with_an_ellipsis() {
    let (_, showcase) = CATEGORIES[0];
    let label = menu_label(showcase[0]);
    assert!(label.ends_with('…'));
    assert!(label.chars().count() <= LABEL_MAX);
    assert!(showcase[0].starts_with(label.trim_end_matches('…').trim_end()));
}

#[test]
fn truncation_counts_characters_not_bytes() {
    // Multi-byte glyphs (the reduction operators) must not split mid-char.
    let wide = "∑".repeat(LABEL_MAX + 10);
    let label = menu_label(&wide);
    assert_eq!(label.chars().count(), LABEL_MAX);
    assert!(label.ends_with('…'));
}
