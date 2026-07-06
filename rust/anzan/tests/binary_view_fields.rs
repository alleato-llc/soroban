//! Port of the Swift `BinaryViewTests.swift` "Bit-field formats" suite —
//! layout parsing (loose maps and typed `Bits::BitFormat` records), field
//! decoding, editing, and the format-map encoders.

use anzan::{BinaryEditorBits, BinaryFieldSpec, BinaryView, Calculator, EvalOutcome, Value};
use num_bigint::BigInt;

fn value(s: &str) -> Value {
    let mut calc = Calculator::new();
    match calc.evaluate(s).expect("evaluates") {
        EvalOutcome::Value(v) => v,
        other => panic!("not a value: {other:?}"),
    }
}

fn view(s: &str) -> BinaryView {
    BinaryView::make(&value(s), 32).expect("unexpectedly unavailable")
}

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
