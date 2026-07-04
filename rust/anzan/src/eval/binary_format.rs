//! Shared, host-neutral format data for the binary bit-editor — the band
//! palette, the typed `Bits` module schema + serializer, and the built-in
//! presets. These live beside `BinaryView` (the editor model already lives
//! in Anzan) so the calculator app, the standalone Tama app, and the test
//! suite all draw from ONE source — and so the serializer/presets are
//! unit-testable.

use super::binary_view::{BinaryView, FieldSpec};
use super::value::Value;

/// The bit-field band palette, by NAME — a field's persisted `color` is one
/// of these; a host maps each name to a real (theme-adapting) color.
pub struct BinaryEditorPalette;

impl BinaryEditorPalette {
    pub const NAMES: [&'static str; 6] = ["blue", "green", "orange", "purple", "pink", "teal"];
}

/// The `Bits` module schema and the serializer that renders a layout as a
/// re-parseable `Bits::BitFormat(...)` constructor (the typed form a format
/// saves as). Pure string work; a host evaluates the result through a
/// `Calculator` to persist or restore.
pub struct BinaryEditorBits;

impl BinaryEditorBits {
    /// Defined once per session, before the first saved format.
    pub const SCHEMA_SOURCE: &'static str =
        "namespace Bits { data BitField { name: String, bits: Number, kind: String, \
         flags: [String], values: [String], color: String, base: Number }; \
         data BitFormat { fields: [BitField] } }";

    /// A field with no explicit color gets the palette name for its position.
    pub fn format_source(layout: &[FieldSpec]) -> String {
        fn list(strings: &[String]) -> String {
            strings
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ")
        }
        const EMPTY: &[String] = &[];
        let fields = layout
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let (kind, flags, values): (&str, &[String], &[String]) = if spec.reserved {
                    ("reserved", EMPTY, EMPTY)
                } else if spec.unused {
                    ("unused", EMPTY, EMPTY)
                } else if let Some(f) = spec.flags.as_deref().filter(|f| !f.is_empty()) {
                    ("flags", f, EMPTY)
                } else if let Some(v) = spec.values.as_deref().filter(|v| !v.is_empty()) {
                    ("enum", EMPTY, v)
                } else {
                    ("numeric", EMPTY, EMPTY)
                };
                let color = spec.color.as_deref().unwrap_or(
                    BinaryEditorPalette::NAMES[index % BinaryEditorPalette::NAMES.len()],
                );
                let base = spec.base.unwrap_or(10);
                format!(
                    "Bits::BitField(name: \"{}\", bits: {}, kind: \"{}\", flags: [{}], \
                     values: [{}], color: \"{}\", base: {})",
                    spec.name,
                    spec.width,
                    kind,
                    list(flags),
                    list(values),
                    color,
                    base
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("Bits::BitFormat(fields: [{fields}])")
    }
}

/// Built-in formats shipped with the editor (not language constructs). Flag
/// fields decode each bit to a meaning (`r-x`); the networking formats read
/// in hex (`numeric_format_map`).
pub struct BinaryEditorPresets;

impl BinaryEditorPresets {
    pub fn standard() -> Vec<(&'static str, Value)> {
        let mut presets = Self::core();
        presets.extend(Self::floating_point());
        presets.extend(Self::color());
        presets.extend(Self::networking());
        presets.extend(Self::systems());
        presets
    }

    /// The original six: permissions, TCP, and the network address layouts.
    fn core() -> Vec<(&'static str, Value)> {
        vec![
            (
                "Unix permissions",
                BinaryView::flag_format_map(&[
                    ("owner", &["r", "w", "x"]),
                    ("group", &["r", "w", "x"]),
                    ("other", &["r", "w", "x"]),
                ]),
            ),
            (
                "TCP flags",
                BinaryView::flag_format_map(&[(
                    "flags",
                    &["CWR", "ECE", "URG", "ACK", "PSH", "RST", "SYN", "FIN"],
                )]),
            ),
            (
                "RGB565",
                BinaryView::format_map(&[("r", 5), ("g", 6), ("b", 5)]),
            ),
            (
                "IPv4 address",
                BinaryView::format_map(&[
                    ("octet1", 8),
                    ("octet2", 8),
                    ("octet3", 8),
                    ("octet4", 8),
                ]),
            ),
            (
                "MAC address",
                BinaryView::numeric_format_map(&[
                    ("oui1", 8, 16),
                    ("oui2", 8, 16),
                    ("oui3", 8, 16),
                    ("nic1", 8, 16),
                    ("nic2", 8, 16),
                    ("nic3", 8, 16),
                ]),
            ),
            (
                "IPv6 address",
                BinaryView::numeric_format_map(&[
                    ("h1", 16, 16),
                    ("h2", 16, 16),
                    ("h3", 16, 16),
                    ("h4", 16, 16),
                    ("h5", 16, 16),
                    ("h6", 16, 16),
                    ("h7", 16, 16),
                    ("h8", 16, 16),
                ]),
            ),
        ]
    }

    /// Floating point — sign / exponent / mantissa.
    fn floating_point() -> Vec<(&'static str, Value)> {
        vec![
            (
                "IEEE 754 float",
                BinaryView::format_map(&[("sign", 1), ("exponent", 8), ("mantissa", 23)]),
            ),
            (
                "IEEE 754 double",
                BinaryView::format_map(&[("sign", 1), ("exponent", 11), ("mantissa", 52)]),
            ),
            (
                "IEEE 754 half",
                BinaryView::format_map(&[("sign", 1), ("exponent", 5), ("mantissa", 10)]),
            ),
            (
                "bfloat16",
                BinaryView::format_map(&[("sign", 1), ("exponent", 8), ("mantissa", 7)]),
            ),
        ]
    }

    /// Packed color channels (hex per channel where natural).
    fn color() -> Vec<(&'static str, Value)> {
        vec![
            (
                "RGBA8888",
                BinaryView::numeric_format_map(&[
                    ("r", 8, 16),
                    ("g", 8, 16),
                    ("b", 8, 16),
                    ("a", 8, 16),
                ]),
            ),
            (
                "ARGB1555",
                BinaryView::format_map(&[("a", 1), ("r", 5), ("g", 5), ("b", 5)]),
            ),
            (
                "RGBA4444",
                BinaryView::numeric_format_map(&[
                    ("r", 4, 16),
                    ("g", 4, 16),
                    ("b", 4, 16),
                    ("a", 4, 16),
                ]),
            ),
        ]
    }

    /// Protocol headers with enum/flag/reserved sub-fields.
    fn networking() -> Vec<(&'static str, Value)> {
        vec![
            (
                "DNS header flags",
                BinaryView::format_value(&[
                    FieldSpec::new("QR", 1).with_flags(&["QR"]),
                    FieldSpec::new("Opcode", 4).with_values(&["QUERY", "IQUERY", "STATUS"]),
                    FieldSpec::new("flags", 4).with_flags(&["AA", "TC", "RD", "RA"]),
                    FieldSpec::new("Z", 3).as_reserved(),
                    FieldSpec::new("RCODE", 4).with_values(&[
                        "NOERROR", "FORMERR", "SERVFAIL", "NXDOMAIN", "NOTIMP", "REFUSED",
                    ]),
                ]),
            ),
            (
                "VLAN 802.1Q tag",
                BinaryView::format_map(&[("PCP", 3), ("DEI", 1), ("VID", 12)]),
            ),
            (
                "IPv4 DSCP/ECN",
                BinaryView::format_map(&[("DSCP", 6), ("ECN", 2)]),
            ),
        ]
    }

    /// CPU/OS/filesystem bit layouts (flags + reserved gaps).
    fn systems() -> Vec<(&'static str, Value)> {
        vec![
            (
                "x86 EFLAGS",
                BinaryView::format_value(&[
                    FieldSpec::new("reserved", 10).as_reserved(),
                    FieldSpec::new("flags", 6).with_flags(&["ID", "VIP", "VIF", "AC", "VM", "RF"]),
                    FieldSpec::new("reserved", 1).as_reserved(),
                    FieldSpec::new("NT", 1).with_flags(&["NT"]),
                    FieldSpec::new("IOPL", 2),
                    FieldSpec::new("flags", 6).with_flags(&["OF", "DF", "IF", "TF", "SF", "ZF"]),
                    FieldSpec::new("reserved", 1).as_reserved(),
                    FieldSpec::new("AF", 1).with_flags(&["AF"]),
                    FieldSpec::new("reserved", 1).as_reserved(),
                    FieldSpec::new("PF", 1).with_flags(&["PF"]),
                    FieldSpec::new("reserved", 1).as_reserved(),
                    FieldSpec::new("CF", 1).with_flags(&["CF"]),
                ]),
            ),
            (
                "Unix mode (st_mode)",
                BinaryView::format_value(&[
                    FieldSpec::new("type", 4).with_base(16),
                    FieldSpec::new("special", 3).with_flags(&["setuid", "setgid", "sticky"]),
                    FieldSpec::new("owner", 3).with_flags(&["r", "w", "x"]),
                    FieldSpec::new("group", 3).with_flags(&["r", "w", "x"]),
                    FieldSpec::new("other", 3).with_flags(&["r", "w", "x"]),
                ]),
            ),
            (
                "FAT date",
                BinaryView::format_map(&[("year", 7), ("month", 4), ("day", 5)]),
            ),
            (
                "FAT time",
                BinaryView::format_map(&[("hour", 5), ("minute", 6), ("sec/2", 5)]),
            ),
        ]
    }
}
