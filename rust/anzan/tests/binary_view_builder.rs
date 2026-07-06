//! Port of the Swift `BinaryViewTests.swift` "Format builder" suite — the
//! visual format builder's claim/draft/add/remove/seed behavior.

use anzan::{BinaryFieldSpec, FormatBuilder, FormatBuilderFieldKind};

fn builder() -> FormatBuilder {
    FormatBuilder::new(&["blue", "green", "orange"])
}

fn names(layout: &[BinaryFieldSpec]) -> Vec<&str> {
    layout.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn claim_toggle_clears() {
    let mut b = builder();
    b.claim(5);
    assert_eq!(b.pending_width(), 5);
    b.claim(5); // clicking the same far edge clears
    assert_eq!(b.pending_width(), 0);
    b.claim(3);
    assert_eq!(b.pending_width(), 3);
}

#[test]
fn add_field_builds_a_kinded_spec_and_resets_the_draft() {
    let mut b = builder();
    // A flags field.
    b.claim(3);
    b.draft_name = "owner".to_string();
    b.draft_kind = FormatBuilderFieldKind::Flags;
    b.draft_labels = "r, w, x".to_string();
    b.add_field();
    // Draft reset, color advanced to the next palette entry.
    assert_eq!(b.pending_width(), 0);
    assert_eq!(b.draft_name, "");
    assert_eq!(b.draft_kind, FormatBuilderFieldKind::Numeric);
    assert_eq!(b.draft_color, "green");
    // An enum field.
    b.claim(2);
    b.draft_name = "mode".to_string();
    b.draft_kind = FormatBuilderFieldKind::Enumeration;
    b.draft_labels = "idle, run, halt, max".to_string();
    b.add_field();
    // A hex numeric field.
    b.claim(8);
    b.draft_name = "rest".to_string();
    b.draft_kind = FormatBuilderFieldKind::Numeric;
    b.draft_base = 16;
    b.add_field();

    let layout = b.layout();
    assert_eq!(names(&layout), ["owner", "mode", "rest"]);
    assert_eq!(
        layout.iter().map(|f| f.width).collect::<Vec<_>>(),
        [3, 2, 8]
    );
    assert_eq!(
        layout[0].flags,
        Some(vec!["r".to_string(), "w".to_string(), "x".to_string()])
    );
    assert_eq!(layout[0].color.as_deref(), Some("blue"));
    assert_eq!(
        layout[1].values,
        Some(vec![
            "idle".to_string(),
            "run".to_string(),
            "halt".to_string(),
            "max".to_string()
        ])
    );
    assert_eq!(layout[1].color.as_deref(), Some("green"));
    assert_eq!(layout[2].base, Some(16));
    assert!(layout[2].flags.is_none() && layout[2].values.is_none());
}

#[test]
fn flags_pad_and_truncate_to_width() {
    let mut b = builder();
    b.claim(3);
    b.draft_name = "f".to_string();
    b.draft_kind = FormatBuilderFieldKind::Flags;
    b.draft_labels = "a, b".to_string(); // short
    b.add_field();
    assert_eq!(
        b.layout()[0].flags,
        Some(vec!["a".to_string(), "b".to_string(), "?".to_string()])
    );
    b.claim(2);
    b.draft_name = "g".to_string();
    b.draft_kind = FormatBuilderFieldKind::Flags;
    b.draft_labels = "a, b, c, d".to_string(); // long
    b.add_field();
    assert_eq!(
        b.layout()[1].flags,
        Some(vec!["a".to_string(), "b".to_string()])
    );
}

#[test]
fn can_add_field_guards_name_and_claim() {
    let mut b = builder();
    assert!(!b.can_add_field()); // nothing claimed, no name
    b.claim(3);
    assert!(!b.can_add_field()); // claimed, still no name
    b.draft_name = "  ".to_string();
    assert!(!b.can_add_field()); // blank name
    b.draft_name = "x".to_string();
    assert!(b.can_add_field());
    b.add_field();
    assert_eq!(b.layout().len(), 1);
    // A no-op add (nothing claimed) leaves the builder unchanged.
    b.add_field();
    assert_eq!(b.layout().len(), 1);
}

#[test]
fn remove_and_recolor() {
    let mut b = builder();
    b.claim(3);
    b.draft_name = "a".to_string();
    b.add_field();
    b.claim(3);
    b.draft_name = "b".to_string();
    b.add_field();
    let first_id = b.fields()[0].id;
    b.recolor(first_id, "teal");
    assert_eq!(b.fields()[0].color_name, "teal");
    b.remove(first_id);
    assert_eq!(
        b.fields()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
}

#[test]
fn width_arithmetic() {
    let mut b = builder();
    b.claim(3);
    b.draft_name = "a".to_string();
    b.add_field();
    b.claim(5);
    b.draft_name = "b".to_string();
    b.add_field();
    assert_eq!(b.committed_width(), 8);
    assert_eq!(b.free_bits(16), 8);
    assert_eq!(b.free_bits(4), 0); // never negative
}

#[test]
fn seed_round_trips_an_existing_layout() {
    let original = vec![
        BinaryFieldSpec::new("owner", 3)
            .with_flags(&["r", "w", "x"])
            .with_color("blue"),
        BinaryFieldSpec::new("rest", 5)
            .with_color("green")
            .with_base(16),
    ];
    let mut b = builder();
    b.seed(&original);
    assert_eq!(
        b.fields()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["owner", "rest"]
    );
    assert_eq!(b.fields()[0].kind, FormatBuilderFieldKind::Flags);
    assert_eq!(b.fields()[1].kind, FormatBuilderFieldKind::Numeric);
    assert_eq!(b.fields()[1].base, 16);
    // The layout reconstructs the same specs.
    assert_eq!(b.layout(), original);
}

#[test]
fn gap_fields_need_no_name_and_carry_their_kind() {
    let mut b = builder();
    // Reserved: no name required; defaults the name to the kind.
    b.claim(8);
    b.draft_kind = FormatBuilderFieldKind::Reserved;
    assert!(b.can_add_field());
    b.add_field();
    assert!(b.layout()[0].reserved);
    assert_eq!(b.layout()[0].name, "reserved");
    // Unused: an editable gap.
    b.claim(4);
    b.draft_kind = FormatBuilderFieldKind::Unused;
    b.add_field();
    assert!(b.layout()[1].unused);
    assert_eq!(b.layout()[1].name, "unused");
    // seed round-trips both gap kinds.
    let mut c = builder();
    c.seed(&b.layout());
    assert_eq!(
        c.fields().iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            FormatBuilderFieldKind::Reserved,
            FormatBuilderFieldKind::Unused
        ]
    );
    assert_eq!(c.layout(), b.layout());
}
