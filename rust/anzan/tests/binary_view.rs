//! Ports of the Swift `BinaryViewTests.swift` suites — "Binary bit-editor
//! view", "Bit-field formats", and "ans-prefix continuation" — case by case.

use anzan::{
    BinaryEditorBits, BinaryFieldSpec, BinaryView, BinaryViewUnavailable, Calculator, EvalOutcome,
    FormatBuilder, FormatBuilderFieldKind, LanguageMode, Value,
};
use num_bigint::BigInt;

fn value(s: &str) -> Value {
    let mut calc = Calculator::new();
    match calc.evaluate(s).expect("evaluates") {
        EvalOutcome::Value(v) => v,
        other => panic!("not a value: {other:?}"),
    }
}

fn view_width(s: &str, preferred_width: u32) -> BinaryView {
    match BinaryView::make(&value(s), preferred_width) {
        Ok(view) => view,
        Err(reason) => panic!("unexpectedly unavailable: {reason:?}"),
    }
}

fn view(s: &str) -> BinaryView {
    view_width(s, 32)
}

fn unavailable(s: &str) -> BinaryViewUnavailable {
    BinaryView::make(&value(s), 32).expect_err("unexpectedly available")
}

// MARK: - Binary bit-editor view

#[test]
fn fixed_int_uses_its_own_width_and_sign() {
    let v = view("Int8(5)");
    assert_eq!(v.width, 8);
    assert!(v.signed());
    // 0000_0101
    assert_eq!(
        v.bits(),
        [false, false, false, false, false, true, false, true]
    );
}

#[test]
fn unsigned_fixed_int_full_range() {
    let v = view("UInt8(255)");
    assert_eq!(v.width, 8);
    assert!(!v.signed());
    assert!(v.bits().iter().all(|&b| b)); // 1111_1111
}

#[test]
fn negative_fixed_int_is_twos_complement() {
    let v = view("Int8(-1)");
    assert!(v.bits().iter().all(|&b| b)); // 1111_1111
    assert_eq!(v.value().to_string(), "Int8(-1)"); // round-trips to its type
}

#[test]
fn plain_integer_is_unsigned_at_preferred_width_bumped_to_fit() {
    assert_eq!(view_width("255", 8).width, 8);
    assert_eq!(view_width("255", 32).width, 32); // preferred floor honored
    assert_eq!(view_width("256", 8).width, 16); // auto-bumped past 8
    assert_eq!(view("5").width, 32); // default preferred
}

#[test]
fn flipping_a_bit_changes_the_value_preserving_kind() {
    // Int8(5) flip bit 1 (0000_0101 → 0000_0111) = 7, still Int8.
    assert_eq!(
        view("Int8(5)").flipping_bit(1).value().to_string(),
        "Int8(7)"
    );
    // Plain 0 at 8-bit, set the high bit → 128 (unsigned), stays a number.
    assert_eq!(
        view_width("0", 8).flipping_bit(7).value().to_string(),
        "128"
    );
    // UInt8 high bit toggles within the width, never overflows.
    assert_eq!(
        view("UInt8(1)").flipping_bit(7).value().to_string(),
        "UInt8(129)"
    );
}

#[test]
fn flip_is_its_own_inverse() {
    let v = view("UInt8(0b1010)");
    assert_eq!(v.flipping_bit(3).flipping_bit(3).value(), v.value());
}

#[test]
fn non_integers_and_strings_are_unavailable() {
    assert_eq!(unavailable("10.5"), BinaryViewUnavailable::NotAnInteger);
    assert_eq!(unavailable("\"hi\""), BinaryViewUnavailable::NotAnInteger);
    // A decimal type, not bits.
    assert_eq!(
        unavailable("Decimal(10.5, 5, 2)"),
        BinaryViewUnavailable::NotAnInteger
    );
}

#[test]
fn negative_plain_number_suggests_a_typed_int() {
    assert_eq!(unavailable("0 - 5"), BinaryViewUnavailable::Negative);
}

#[test]
fn over_256_bits_is_too_wide() {
    assert_eq!(unavailable("2 ^ 300"), BinaryViewUnavailable::TooWide); // a 301-bit plain integer
}

#[test]
fn minimum_width_tracks_the_value() {
    // The narrowest editable width that holds the value — the UI grays out
    // smaller picker options below this.
    assert_eq!(view("5").minimum_width(), 8); // 3 bits → 8
    assert_eq!(view("255").minimum_width(), 8); // 8 bits → 8
    assert_eq!(view("256").minimum_width(), 16); // 9 bits → 16
    assert_eq!(view("2 ^ 100").minimum_width(), 128); // 101 bits → 128
}

#[test]
fn up_to_256_bits_edits() {
    assert_eq!(view("UInt256(1)").width, 256); // the widest fixed type
    assert_eq!(view("Int256(-1)").width, 256);
    assert_eq!(view("2 ^ 100").width, 128); // 101 bits → 128
    assert_eq!(view("2 ^ 200").width, 256); // 201 bits → 256
}

// MARK: - Bit-field formats

fn perms() -> Vec<BinaryFieldSpec> {
    vec![
        BinaryFieldSpec::new("owner", 3),
        BinaryFieldSpec::new("group", 3),
        BinaryFieldSpec::new("other", 3),
    ]
}

fn names(layout: &[BinaryFieldSpec]) -> Vec<&str> {
    layout.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn layout_parses_a_map() {
    let map = value("{owner: 3, group: 3, other: 3}");
    let layout = BinaryView::layout(&map).expect("a layout");
    // Insertion order preserved.
    assert_eq!(names(&layout), ["owner", "group", "other"]);
    assert_eq!(
        layout.iter().map(|f| f.width).collect::<Vec<_>>(),
        [3, 3, 3]
    );
    // Not a layout: non-map values.
    assert_eq!(BinaryView::layout(&value("5")), None);
}

#[test]
fn decodes_fields_high_to_low_into_the_low_bits() {
    // 493 = 0b1_1110_1101 → owner=111(7) group=101(5) other=101(5).
    let fields = view("493").fields(&perms());
    assert_eq!(
        fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["owner", "group", "other"]
    );
    assert_eq!(
        fields.iter().map(|f| f.value.clone()).collect::<Vec<_>>(),
        [BigInt::from(7), BigInt::from(5), BigInt::from(5)]
    );
    // owner is the highest used range.
    assert_eq!(
        fields.iter().map(|f| f.low_bit).collect::<Vec<_>>(),
        [6, 3, 0]
    );
}

#[test]
fn setting_a_field_rewrites_only_its_range() {
    let v = view("493"); // owner=7 group=5 other=5
    let edited = v.setting_field("group", &BigInt::from(0), &perms());
    assert_eq!(
        edited
            .fields(&perms())
            .iter()
            .map(|f| f.value.clone())
            .collect::<Vec<_>>(),
        [BigInt::from(7), BigInt::from(0), BigInt::from(5)]
    );
    assert_eq!(edited.value().display_description(), "453"); // 0b111000101
}

#[test]
fn setting_clamps_to_the_field_width() {
    // 9 doesn't fit 3 bits; only the low 3 bits (001) land.
    let v = view("0").setting_field("other", &BigInt::from(9), &perms());
    assert_eq!(v.fields(&perms())[2].value, BigInt::from(1));
}

#[test]
fn flag_fields_decode_to_their_meaning() {
    // 493 = 0o755: owner=rwx, group=r-x, other=r-x.
    let rwx: &[&str] = &["r", "w", "x"];
    let layout = vec![
        BinaryFieldSpec::new("owner", 3).with_flags(rwx),
        BinaryFieldSpec::new("group", 3).with_flags(rwx),
        BinaryFieldSpec::new("other", 3).with_flags(rwx),
    ];
    let fields = view("493").fields(&layout);
    assert_eq!(
        fields.iter().map(|f| f.flag_string()).collect::<Vec<_>>(),
        [
            Some("rwx".to_string()),
            Some("r-x".to_string()),
            Some("r-x".to_string())
        ]
    );
    // 001 → --x (only the low/execute bit).
    let one_field = vec![BinaryFieldSpec::new("owner", 3).with_flags(rwx)];
    assert_eq!(
        view("1").fields(&one_field)[0].flag_string(),
        Some("--x".to_string())
    );
}

#[test]
fn multi_char_flags_list_only_the_set_ones() {
    // A 2-flag field, low bit set → lists the set name.
    let layout = vec![BinaryFieldSpec::new("f", 2).with_flags(&["ACK", "SYN"])];
    assert_eq!(
        view("1").fields(&layout)[0].flag_string(),
        Some("SYN".to_string()) // bit0 = SYN (low)
    );
    assert_eq!(
        view("0").fields(&layout)[0].flag_string(),
        Some("—".to_string())
    );
}

#[test]
fn layout_parses_flag_arrays() {
    let map = value("{owner: [\"r\", \"w\", \"x\"]}");
    let layout = BinaryView::layout(&map).expect("a layout");
    assert_eq!(layout[0].width, 3);
    assert_eq!(
        layout[0].flags,
        Some(vec!["r".to_string(), "w".to_string(), "x".to_string()])
    );
}

/// The `Bits` schema the binary editor emits — now shared from the crate
/// itself (`BinaryEditorBits`), so this test also pins the constant.
const BITS_SCHEMA_SOURCE: &str = BinaryEditorBits::SCHEMA_SOURCE;

#[test]
fn layout_parses_a_typed_bit_format_record() {
    // The phase-4 bridge: a Bits::BitFormat record reads structurally into a
    // layout — a flags field, an enum field, and a numeric field, chosen by
    // `kind`.
    let mut calculator = Calculator::new();
    calculator
        .evaluate(BITS_SCHEMA_SOURCE)
        .expect("schema evaluates");
    let outcome = calculator
        .evaluate(concat!(
            "Bits::BitFormat(fields: [",
            "Bits::BitField(name: \"owner\", bits: 3, kind: \"flags\", flags: [\"r\", \"w\", \"x\"], values: [], color: \"blue\", base: 10), ",
            "Bits::BitField(name: \"mode\", bits: 2, kind: \"enum\", flags: [], values: [\"idle\", \"run\", \"halt\", \"max\"], color: \"green\", base: 10), ",
            "Bits::BitField(name: \"rest\", bits: 3, kind: \"numeric\", flags: [], values: [], color: \"\", base: 16)])"
        ))
        .expect("record evaluates");
    let EvalOutcome::Value(record) = outcome else {
        panic!("not a value");
    };
    let layout = BinaryView::layout(&record).expect("a layout");
    assert_eq!(names(&layout), ["owner", "mode", "rest"]);
    assert_eq!(
        layout.iter().map(|f| f.width).collect::<Vec<_>>(),
        [3, 2, 3]
    );
    assert_eq!(
        layout[0].flags,
        Some(vec!["r".to_string(), "w".to_string(), "x".to_string()])
    );
    // Empty color → None (auto).
    assert_eq!(
        layout.iter().map(|f| f.color.clone()).collect::<Vec<_>>(),
        [Some("blue".to_string()), Some("green".to_string()), None]
    );
    assert_eq!(
        layout[1].values,
        Some(vec![
            "idle".to_string(),
            "run".to_string(),
            "halt".to_string(),
            "max".to_string()
        ])
    );
    assert!(layout[2].flags.is_none() && layout[2].values.is_none()); // numeric
    assert_eq!(layout[2].base, Some(16)); // the numeric field reads in hex
}

#[test]
fn numeric_field_renders_and_parses_in_its_base() {
    // The loose {bits, base} map form round-trips a per-field display base.
    let layout = BinaryView::layout(&BinaryView::numeric_format_map(&[
        ("oui", 8, 16),
        ("nic", 8, 10),
    ]))
    .expect("a layout");
    // Base 10 normalizes to None (decimal default).
    assert_eq!(
        layout.iter().map(|f| f.base).collect::<Vec<_>>(),
        [Some(16), None]
    );
    // 0x1B44 across two octets → oui=0x1b (hex text), nic=68 (decimal text).
    let fields = view("0x1B44").fields(&layout);
    assert_eq!(
        fields.iter().map(|f| f.value_text()).collect::<Vec<_>>(),
        ["0x1b", "68"]
    );
    assert_eq!(
        fields.iter().map(|f| f.label()).collect::<Vec<_>>(),
        ["0x1b", "68"]
    );
}

#[test]
fn parse_is_the_inverse_of_value_text() {
    // Parse in the field's base…
    assert_eq!(BinaryView::parse("27", 10), Some(BigInt::from(27)));
    assert_eq!(BinaryView::parse("1b", 16), Some(BigInt::from(27)));
    // …but an explicit prefix always wins over the base.
    assert_eq!(BinaryView::parse("0x1b", 10), Some(BigInt::from(27)));
    assert_eq!(BinaryView::parse("0b101", 10), Some(BigInt::from(5)));
    assert_eq!(BinaryView::parse("0o17", 10), Some(BigInt::from(15)));
    assert_eq!(BinaryView::parse("zz", 16), None);
    // Round-trip: a field's value_text parses back to its value, any base.
    for base in [2, 8, 10, 16] {
        let f = anzan::BinaryField {
            name: "f".to_string(),
            width: 8,
            low_bit: 0,
            value: BigInt::from(171),
            flags: None,
            values: None,
            base: Some(base),
            reserved: false,
            unused: false,
        };
        assert_eq!(
            BinaryView::parse(&f.value_text(), base),
            Some(BigInt::from(171))
        );
    }
}

#[test]
fn enum_field_decodes_its_value_to_a_label() {
    // owner=rwx(7) mode=10(2 = "halt") rest=101(5).  0b111_10_101 = 501.
    let layout = vec![
        BinaryFieldSpec::new("owner", 3).with_flags(&["r", "w", "x"]),
        BinaryFieldSpec::new("mode", 2).with_values(&["idle", "run", "halt", "max"]),
        BinaryFieldSpec::new("rest", 3),
    ];
    let fields = view("501").fields(&layout);
    assert_eq!(
        fields.iter().map(|f| f.label()).collect::<Vec<_>>(),
        ["rwx", "halt", "5"]
    );
    // A value past the label list shows the raw number.
    let two = vec![BinaryFieldSpec::new("x", 3).with_values(&["a", "b"])];
    assert_eq!(
        view("5").fields(&two)[0].enum_string(),
        Some("5".to_string())
    );
}

/// Beyond the Swift suite: `format_value` (the loose-map encoder for the
/// richer presets) round-trips mixed field kinds through `layout`, including
/// duplicate field names — ported from `BinaryFormatTests
/// .formatValueRoundTripsMixedFields` / `.formatValuePreservesDuplicate…`,
/// the two cases there that exercise only the `BinaryView` seam.
#[test]
fn format_value_round_trips_mixed_fields() {
    let layout = vec![
        BinaryFieldSpec::new("QR", 1).with_flags(&["QR"]),
        BinaryFieldSpec::new("Opcode", 4).with_values(&["QUERY", "IQUERY", "STATUS"]),
        BinaryFieldSpec::new("Z", 3).as_reserved(),
        BinaryFieldSpec::new("spare", 2).as_unused(),
        BinaryFieldSpec::new("addr", 8).with_base(16),
    ];
    let decoded = BinaryView::layout(&BinaryView::format_value(&layout)).expect("a layout");
    assert_eq!(decoded, layout);
}

#[test]
fn format_value_preserves_duplicate_field_names_in_order() {
    // Duplicate field names (EFLAGS repeats "reserved"/"flags") must survive
    // encode/decode in order — the parser keys off position, not name.
    let layout = vec![
        BinaryFieldSpec::new("reserved", 2).as_reserved(),
        BinaryFieldSpec::new("flags", 2).with_flags(&["A", "B"]),
        BinaryFieldSpec::new("reserved", 1).as_reserved(),
        BinaryFieldSpec::new("flags", 2).with_flags(&["C", "D"]),
    ];
    let decoded = BinaryView::layout(&BinaryView::format_value(&layout)).expect("a layout");
    assert_eq!(decoded, layout);
    assert_eq!(
        decoded.iter().map(|f| f.reserved).collect::<Vec<_>>(),
        [true, false, true, false]
    );
}

/// `format_map` / `flag_format_map` build the loose shapes `layout` reads —
/// the Swift helpers exist for the app presets; pin their round-trips here.
#[test]
fn format_map_helpers_round_trip() {
    let numeric =
        BinaryView::layout(&BinaryView::format_map(&[("hi", 4), ("lo", 4)])).expect("a layout");
    assert_eq!(names(&numeric), ["hi", "lo"]);
    assert_eq!(numeric.iter().map(|f| f.width).collect::<Vec<_>>(), [4, 4]);

    let flags = BinaryView::layout(&BinaryView::flag_format_map(&[("owner", &["r", "w", "x"])]))
        .expect("a layout");
    assert_eq!(flags[0].flags.as_ref().map(|f| f.len()), Some(3));
    assert_eq!(BinaryView::layout_width(&flags), 3);
}

// MARK: - Format builder

fn builder() -> FormatBuilder {
    FormatBuilder::new(&["blue", "green", "orange"])
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

// MARK: - ans-prefix continuation

#[test]
fn leading_binary_operator_prefixes_ans() {
    let normal = LanguageMode::Normal;
    assert_eq!(
        Calculator::ans_prefixed("+5", normal).as_deref(),
        Some("ans+5")
    );
    assert_eq!(
        Calculator::ans_prefixed("*2", normal).as_deref(),
        Some("ans*2")
    );
    assert_eq!(
        Calculator::ans_prefixed("/4", normal).as_deref(),
        Some("ans/4")
    );
    assert_eq!(
        Calculator::ans_prefixed("^2", normal).as_deref(),
        Some("ans^2")
    );
    assert_eq!(
        Calculator::ans_prefixed("× 3", normal).as_deref(),
        Some("ans× 3")
    );
}

#[test]
fn minus_is_included_speedcrunch_style() {
    assert_eq!(
        Calculator::ans_prefixed("-5", LanguageMode::Normal).as_deref(),
        Some("ans-5")
    );
}

#[test]
fn leading_spaces_are_trimmed_before_prefixing() {
    assert_eq!(
        Calculator::ans_prefixed("  + 5", LanguageMode::Normal).as_deref(),
        Some("ans+ 5")
    );
}

#[test]
fn non_operator_leads_do_not_prefix() {
    assert_eq!(
        Calculator::ans_prefixed("5 + 3", LanguageMode::Normal),
        None
    );
    assert_eq!(
        Calculator::ans_prefixed("sqrt(2)", LanguageMode::Normal),
        None
    );
    assert_eq!(
        Calculator::ans_prefixed("(1+2)", LanguageMode::Normal),
        None
    );
    assert_eq!(Calculator::ans_prefixed("", LanguageMode::Normal), None);
    // ~ is unary prefix.
    assert_eq!(
        Calculator::ans_prefixed("~5", LanguageMode::Programmer),
        None
    );
}

#[test]
fn percent_and_bit_glyphs_are_operators_only_in_programmer() {
    // Normal: % is postfix percent, bit glyphs aren't operators → no prefix.
    assert_eq!(Calculator::ans_prefixed("%5", LanguageMode::Normal), None);
    assert_eq!(Calculator::ans_prefixed("<<2", LanguageMode::Normal), None);
    assert_eq!(Calculator::ans_prefixed("&3", LanguageMode::Normal), None);
    // Programmer: they lead a continuation.
    assert_eq!(
        Calculator::ans_prefixed("%5", LanguageMode::Programmer).as_deref(),
        Some("ans%5")
    );
    assert_eq!(
        Calculator::ans_prefixed("<<2", LanguageMode::Programmer).as_deref(),
        Some("ans<<2")
    );
    assert_eq!(
        Calculator::ans_prefixed("&3", LanguageMode::Programmer).as_deref(),
        Some("ans&3")
    );
}
