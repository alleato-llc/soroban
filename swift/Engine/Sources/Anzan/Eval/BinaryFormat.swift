/// Shared, host-neutral format data for the binary bit-editor — the band
/// palette, the typed `Bits` module schema + serializer, and the built-in
/// presets. These live beside `BinaryView` (the editor model already lives in
/// Anzan) so the calculator app, the standalone Tama app, and the test suite
/// all draw from ONE source — and so the serializer/presets are unit-testable
/// (a SwiftUI package has no test target; the engine does).

/// The bit-field band palette, by NAME — a field's persisted `color` is one of
/// these; a host maps each name to a real (theme-adapting) color.
public enum BinaryEditorPalette {
    public static let names = ["blue", "green", "orange", "purple", "pink", "teal"]
}

/// The `Bits` module schema and the serializer that renders a layout as a
/// re-parseable `Bits::BitFormat(...)` constructor (the typed form a format
/// saves as). Pure string work; a host evaluates the result through a
/// `Calculator` to persist or restore.
public enum BinaryEditorBits {
    /// Defined once per session, before the first saved format.
    public static let schemaSource =
        "namespace Bits { data BitField { name: String, bits: Number, kind: String, "
        + "flags: [String], values: [String], color: String, base: Number }; "
        + "data BitFormat { fields: [BitField] } }"

    /// A field with no explicit color gets the palette name for its position.
    public static func formatSource(_ layout: [BinaryView.FieldSpec]) -> String {
        func list(_ strings: [String]) -> String {
            strings.map { "\"\($0)\"" }.joined(separator: ", ")
        }
        let fields = layout.enumerated().map { index, spec -> String in
            let kind: String, flags: [String], values: [String]
            if spec.reserved {
                kind = "reserved"; flags = []; values = []
            } else if spec.unused {
                kind = "unused"; flags = []; values = []
            } else if let f = spec.flags, !f.isEmpty {
                kind = "flags"; flags = f; values = []
            } else if let v = spec.values, !v.isEmpty {
                kind = "enum"; flags = []; values = v
            } else {
                kind = "numeric"; flags = []; values = []
            }
            let color = spec.color ?? BinaryEditorPalette.names[index % BinaryEditorPalette.names.count]
            let base = spec.base ?? 10
            return "Bits::BitField(name: \"\(spec.name)\", bits: \(spec.width), "
                + "kind: \"\(kind)\", flags: [\(list(flags))], values: [\(list(values))], "
                + "color: \"\(color)\", base: \(base))"
        }.joined(separator: ", ")
        return "Bits::BitFormat(fields: [\(fields)])"
    }
}

/// Built-in formats shipped with the editor (not language constructs). Flag
/// fields decode each bit to a meaning (`r-x`); the networking formats read in
/// hex (`numericFormatMap`).
public enum BinaryEditorPresets {
    public static let standard: [(name: String, format: Value)] =
        core + floatingPoint + color + networking + systems

    /// The original six: permissions, TCP, and the network address layouts.
    private static let core: [(name: String, format: Value)] = [
        ("Unix permissions", BinaryView.flagFormatMap([
            ("owner", ["r", "w", "x"]), ("group", ["r", "w", "x"]), ("other", ["r", "w", "x"])])),
        ("TCP flags", BinaryView.flagFormatMap([
            ("flags", ["CWR", "ECE", "URG", "ACK", "PSH", "RST", "SYN", "FIN"])])),
        ("RGB565", BinaryView.formatMap([("r", 5), ("g", 6), ("b", 5)])),
        ("IPv4 address", BinaryView.formatMap([
            ("octet1", 8), ("octet2", 8), ("octet3", 8), ("octet4", 8)])),
        ("MAC address", BinaryView.numericFormatMap([
            ("oui1", 8, 16), ("oui2", 8, 16), ("oui3", 8, 16),
            ("nic1", 8, 16), ("nic2", 8, 16), ("nic3", 8, 16)])),
        ("IPv6 address", BinaryView.numericFormatMap([
            ("h1", 16, 16), ("h2", 16, 16), ("h3", 16, 16), ("h4", 16, 16),
            ("h5", 16, 16), ("h6", 16, 16), ("h7", 16, 16), ("h8", 16, 16)])),
    ]

    /// Floating point — sign / exponent / mantissa.
    private static let floatingPoint: [(name: String, format: Value)] = [
        ("IEEE 754 float", BinaryView.formatMap([("sign", 1), ("exponent", 8), ("mantissa", 23)])),
        ("IEEE 754 double", BinaryView.formatMap([("sign", 1), ("exponent", 11), ("mantissa", 52)])),
        ("IEEE 754 half", BinaryView.formatMap([("sign", 1), ("exponent", 5), ("mantissa", 10)])),
        ("bfloat16", BinaryView.formatMap([("sign", 1), ("exponent", 8), ("mantissa", 7)])),
    ]

    /// Packed color channels (hex per channel where natural).
    private static let color: [(name: String, format: Value)] = [
        ("RGBA8888", BinaryView.numericFormatMap([("r", 8, 16), ("g", 8, 16), ("b", 8, 16), ("a", 8, 16)])),
        ("ARGB1555", BinaryView.formatMap([("a", 1), ("r", 5), ("g", 5), ("b", 5)])),
        ("RGBA4444", BinaryView.numericFormatMap([("r", 4, 16), ("g", 4, 16), ("b", 4, 16), ("a", 4, 16)])),
    ]

    /// Protocol headers with enum/flag/reserved sub-fields.
    private static let networking: [(name: String, format: Value)] = [
        ("DNS header flags", BinaryView.formatValue([
            .init(name: "QR", width: 1, flags: ["QR"]),
            .init(name: "Opcode", width: 4, values: ["QUERY", "IQUERY", "STATUS"]),
            .init(name: "flags", width: 4, flags: ["AA", "TC", "RD", "RA"]),
            .init(name: "Z", width: 3, reserved: true),
            .init(name: "RCODE", width: 4,
                  values: ["NOERROR", "FORMERR", "SERVFAIL", "NXDOMAIN", "NOTIMP", "REFUSED"]),
        ])),
        ("VLAN 802.1Q tag", BinaryView.formatMap([("PCP", 3), ("DEI", 1), ("VID", 12)])),
        ("IPv4 DSCP/ECN", BinaryView.formatMap([("DSCP", 6), ("ECN", 2)])),
    ]

    /// CPU/OS/filesystem bit layouts (flags + reserved gaps).
    private static let systems: [(name: String, format: Value)] = [
        ("x86 EFLAGS", BinaryView.formatValue([
            .init(name: "reserved", width: 10, reserved: true),
            .init(name: "flags", width: 6, flags: ["ID", "VIP", "VIF", "AC", "VM", "RF"]),
            .init(name: "reserved", width: 1, reserved: true),
            .init(name: "NT", width: 1, flags: ["NT"]),
            .init(name: "IOPL", width: 2),
            .init(name: "flags", width: 6, flags: ["OF", "DF", "IF", "TF", "SF", "ZF"]),
            .init(name: "reserved", width: 1, reserved: true),
            .init(name: "AF", width: 1, flags: ["AF"]),
            .init(name: "reserved", width: 1, reserved: true),
            .init(name: "PF", width: 1, flags: ["PF"]),
            .init(name: "reserved", width: 1, reserved: true),
            .init(name: "CF", width: 1, flags: ["CF"]),
        ])),
        ("Unix mode (st_mode)", BinaryView.formatValue([
            .init(name: "type", width: 4, base: 16),
            .init(name: "special", width: 3, flags: ["setuid", "setgid", "sticky"]),
            .init(name: "owner", width: 3, flags: ["r", "w", "x"]),
            .init(name: "group", width: 3, flags: ["r", "w", "x"]),
            .init(name: "other", width: 3, flags: ["r", "w", "x"]),
        ])),
        ("FAT date", BinaryView.formatMap([("year", 7), ("month", 4), ("day", 5)])),
        ("FAT time", BinaryView.formatMap([("hour", 5), ("minute", 6), ("sec/2", 5)])),
    ]
}
