//! Port of the Swift `BinaryFormatTests.swift` suite ("Binary editor format
//! library") — the `BinaryEditorBits` serializer is the wire format every
//! saved format persists as, so it earns a round-trip; the presets ship to
//! users, so they earn a well-formedness check. The two cases that exercise
//! only the `BinaryView` seam (`formatValueRoundTripsMixedFields` /
//! `formatValuePreservesDuplicateFieldNamesInOrder`) live in
//! `tests/binary_view.rs`; the workbook save/reopen case needs the engine
//! crate and lives in `rust/engine/tests/binary_format.rs`.

use anzan::{
    BinaryEditorBits, BinaryEditorPalette, BinaryEditorPresets, BinaryField, BinaryFieldSpec,
    BinaryView, BinaryViewKind, Calculator,
};
use num_bigint::BigInt;

/// Evaluate a layout's serialized `Bits::BitFormat(...)` and decode it back —
/// the exact path a save/restore takes.
fn round_trip(layout: &[BinaryFieldSpec]) -> Option<Vec<BinaryFieldSpec>> {
    let mut calc = Calculator::new();
    calc.evaluate(BinaryEditorBits::SCHEMA_SOURCE)
        .expect("schema evaluates");
    let value = calc
        .evaluate_formula(&BinaryEditorBits::format_source(layout))
        .ok()?;
    BinaryView::layout(&value)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn numeric_flags_and_enum_fields_round_trip() {
    let layout = vec![
        BinaryFieldSpec::new("owner", 3)
            .with_flags(&["r", "w", "x"])
            .with_color("blue"),
        BinaryFieldSpec::new("mode", 2)
            .with_values(&["idle", "run", "halt", "max"])
            .with_color("green"),
        BinaryFieldSpec::new("port", 8)
            .with_color("orange")
            .with_base(16),
    ];
    assert_eq!(round_trip(&layout), Some(layout.clone()));
}

#[test]
fn reserved_and_unused_gaps_round_trip() {
    let layout = vec![
        BinaryFieldSpec::new("hi", 4).with_color("blue"),
        BinaryFieldSpec::new("rsvd", 2)
            .with_color("green")
            .as_reserved(),
        BinaryFieldSpec::new("spare", 2)
            .with_color("orange")
            .as_unused(),
    ];
    let decoded = round_trip(&layout).expect("a layout");
    assert_eq!(decoded, layout); // FieldSpec equality covers reserved/unused
    assert_eq!(
        decoded.iter().map(|f| f.reserved).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        decoded.iter().map(|f| f.unused).collect::<Vec<_>>(),
        [false, false, true]
    );
}

#[test]
fn a_base_persists_through_the_serializer() {
    for base in [2, 8, 16] {
        let layout = vec![BinaryFieldSpec::new("v", 8)
            .with_color("blue")
            .with_base(base)];
        assert_eq!(round_trip(&layout).expect("a layout")[0].base, Some(base));
    }
    // 10 is the decimal default — it normalizes back to None, not 10.
    let decimal = vec![BinaryFieldSpec::new("v", 8)
        .with_color("blue")
        .with_base(10)];
    assert_eq!(round_trip(&decimal).expect("a layout")[0].base, None);
}

#[test]
fn a_colorless_layout_is_canonicalized_to_the_palette_by_position() {
    let bare = vec![
        BinaryFieldSpec::new("a", 4),
        BinaryFieldSpec::new("b", 4),
        BinaryFieldSpec::new("c", 4),
    ];
    let decoded = round_trip(&bare).expect("a layout");
    assert_eq!(
        decoded.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(
        decoded.iter().map(|f| f.width).collect::<Vec<_>>(),
        [4, 4, 4]
    );
    assert_eq!(
        decoded
            .iter()
            .filter_map(|f| f.color.as_deref())
            .collect::<Vec<_>>(),
        BinaryEditorPalette::NAMES[..3]
    );
}

/// Decode real, known-good values through the new presets — the proof the
/// layouts are not just the right *width* but the right *bits*, decoded by
/// the same `fields()` path the editor uses.
#[test]
fn rich_presets_decode_real_world_values() {
    fn decode(name: &str, pattern: BigInt, width: u32) -> Vec<BinaryField> {
        let format = BinaryEditorPresets::standard()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("preset {name} exists"))
            .1;
        let layout = BinaryView::layout(&format).expect("a layout");
        BinaryView {
            kind: BinaryViewKind::Plain,
            width,
            pattern,
        }
        .fields(&layout)
    }
    fn field<'a>(fields: &'a [BinaryField], name: &str) -> &'a BinaryField {
        fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("field {name} exists"))
    }

    // IEEE 754 float 1.0 = 0x3F800000 → sign 0, exponent 127 (biased), mantissa 0.
    let f = decode("IEEE 754 float", BigInt::from(0x3F80_0000), 32);
    assert_eq!(field(&f, "sign").value, BigInt::from(0));
    assert_eq!(field(&f, "exponent").value, BigInt::from(127));
    assert_eq!(field(&f, "mantissa").value, BigInt::from(0));
    // IEEE 754 double -2.0 = 0xC000000000000000 → sign 1, exponent 1024, mantissa 0.
    let d = decode(
        "IEEE 754 double",
        (BigInt::from(1) << 63) | (BigInt::from(1) << 62),
        64,
    );
    assert_eq!(field(&d, "sign").value, BigInt::from(1));
    assert_eq!(field(&d, "exponent").value, BigInt::from(1024));

    // Unix st_mode 0o100644 = regular file (type 0x8), rw-r--r--.
    let m = decode("Unix mode (st_mode)", BigInt::from(0o100644), 16);
    assert_eq!(field(&m, "type").value, BigInt::from(8)); // S_IFREG high nibble
    assert_eq!(field(&m, "owner").flag_string().as_deref(), Some("rw-"));
    assert_eq!(field(&m, "group").flag_string().as_deref(), Some("r--"));
    assert_eq!(field(&m, "other").flag_string().as_deref(), Some("r--"));

    // x86 EFLAGS 0x45 → CF (bit 0), PF (bit 2), ZF (bit 6) set; nothing else.
    let e = decode("x86 EFLAGS", BigInt::from(0x45), 32);
    assert_eq!(field(&e, "CF").flag_string().as_deref(), Some("CF"));
    assert_eq!(field(&e, "PF").flag_string().as_deref(), Some("PF"));
    assert_eq!(
        e.iter()
            .find(|f| f
                .flags
                .as_ref()
                .is_some_and(|flags| flags.iter().any(|n| n == "ZF")))
            .and_then(|f| f.flag_string())
            .as_deref(),
        Some("ZF")
    );
    // Sanity: no cross-format leakage.
    assert!(!e.iter().any(|f| f
        .flags
        .as_ref()
        .is_some_and(|flags| flags.iter().any(|n| n == "CWR"))));

    // RGBA8888 0xAABBCCDD → r=0xAA, g=0xBB, b=0xCC, a=0xDD (hex readout).
    let c = decode("RGBA8888", BigInt::from(0xAABB_CCDDu32), 32);
    assert_eq!(field(&c, "r").value_text(), "0xaa");
    assert_eq!(field(&c, "a").value_text(), "0xdd");
}

#[test]
fn every_preset_decodes_and_is_well_formed() {
    for (name, format) in BinaryEditorPresets::standard() {
        let layout = BinaryView::layout(&format);
        assert!(layout.is_some(), "preset {name} should decode to a layout");
        let layout = layout.unwrap();
        assert!(
            layout.iter().all(|f| f.width >= 1),
            "preset {name} has a zero-width field"
        );
        assert!(!layout.is_empty(), "preset {name} is empty");
    }
}

#[test]
fn preset_total_widths_match_their_protocols() {
    fn bits(name: &str) -> u32 {
        let format = BinaryEditorPresets::standard()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("preset {name} exists"))
            .1;
        BinaryView::layout_width(&BinaryView::layout(&format).expect("a layout"))
    }
    assert_eq!(bits("Unix permissions"), 9); // rwx × 3
    assert_eq!(bits("TCP flags"), 8);
    assert_eq!(bits("RGB565"), 16);
    assert_eq!(bits("IPv4 address"), 32);
    assert_eq!(bits("MAC address"), 48);
    assert_eq!(bits("IPv6 address"), 128);
    // Floating point.
    assert_eq!(bits("IEEE 754 float"), 32);
    assert_eq!(bits("IEEE 754 double"), 64);
    assert_eq!(bits("IEEE 754 half"), 16);
    assert_eq!(bits("bfloat16"), 16);
    // Color.
    assert_eq!(bits("RGBA8888"), 32);
    assert_eq!(bits("ARGB1555"), 16);
    assert_eq!(bits("RGBA4444"), 16);
    // Networking.
    assert_eq!(bits("DNS header flags"), 16);
    assert_eq!(bits("VLAN 802.1Q tag"), 16);
    assert_eq!(bits("IPv4 DSCP/ECN"), 8);
    // Systems.
    assert_eq!(bits("x86 EFLAGS"), 32);
    assert_eq!(bits("Unix mode (st_mode)"), 16);
    assert_eq!(bits("FAT date"), 16);
    assert_eq!(bits("FAT time"), 16);
}

/// The richer presets carry enum / reserved fields — spot-check that the
/// decode preserves those kinds (not just total width).
#[test]
fn rich_presets_carry_enum_and_reserved_fields() {
    fn layout(name: &str) -> Vec<BinaryFieldSpec> {
        let format = BinaryEditorPresets::standard()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("preset {name} exists"))
            .1;
        BinaryView::layout(&format).expect("a layout")
    }
    let dns = layout("DNS header flags");
    assert!(dns.iter().any(|f| f.name == "Opcode" && f.values.is_some()));
    assert!(dns.iter().any(|f| f.name == "Z" && f.reserved));

    let eflags = layout("x86 EFLAGS");
    assert_eq!(eflags.iter().filter(|f| f.reserved).count(), 5);
    assert!(eflags.iter().any(|f| f
        .flags
        .as_ref()
        .is_some_and(|flags| flags.iter().any(|n| n == "CF"))));

    let mode = layout("Unix mode (st_mode)");
    assert!(mode.iter().any(|f| f.name == "type" && f.base == Some(16)));
    assert!(mode
        .iter()
        .any(|f| f.name == "owner" && f.flags == Some(strings(&["r", "w", "x"]))));
}
