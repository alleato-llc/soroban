import Testing
import BigInt
@testable import Anzan
@testable import SorobanEngine

@Suite("Binary bit-editor view")
struct BinaryViewTests {
    private func view(_ s: String, preferredWidth: Int = 32) -> BinaryView {
        let value = try! Calculator().evaluate(s).get()
        guard case .value(let v) = value else { Issue.record("not a value"); fatalError() }
        switch BinaryView.make(for: v, preferredWidth: preferredWidth) {
        case .success(let view): return view
        case .failure(let reason): Issue.record("unexpectedly unavailable: \(reason)"); fatalError()
        }
    }

    private func unavailable(_ s: String) -> BinaryView.Unavailable? {
        let value = try! Calculator().evaluate(s).get()
        guard case .value(let v) = value else { return nil }
        if case .failure(let reason) = BinaryView.make(for: v) { return reason }
        return nil
    }

    @Test func fixedIntUsesItsOwnWidthAndSign() {
        let v = view("Int8(5)")
        #expect(v.width == 8)
        #expect(v.signed)
        #expect(v.bits == [false, false, false, false, false, true, false, true]) // 0000_0101
    }

    @Test func unsignedFixedIntFullRange() {
        let v = view("UInt8(255)")
        #expect(v.width == 8)
        #expect(!v.signed)
        #expect(v.bits.allSatisfy { $0 })  // 1111_1111
    }

    @Test func negativeFixedIntIsTwosComplement() {
        let v = view("Int8(-1)")
        #expect(v.bits.allSatisfy { $0 })          // 1111_1111
        #expect(v.value.description == "Int8(-1)") // round-trips to its type
    }

    @Test func plainIntegerIsUnsignedAtPreferredWidthBumpedToFit() {
        #expect(view("255", preferredWidth: 8).width == 8)
        #expect(view("255", preferredWidth: 32).width == 32) // preferred floor honored
        #expect(view("256", preferredWidth: 8).width == 16)  // auto-bumped past 8
        #expect(view("5").width == 32)                       // default preferred
    }

    @Test func flippingABitChangesTheValuePreservingKind() {
        // Int8(5) flip bit 1 (0000_0101 → 0000_0111) = 7, still Int8.
        #expect(view("Int8(5)").flippingBit(1).value.description == "Int8(7)")
        // Plain 0 at 8-bit, set the high bit → 128 (unsigned), stays a number.
        #expect(view("0", preferredWidth: 8).flippingBit(7).value.description == "128")
        // UInt8 high bit toggles within the width, never overflows.
        #expect(view("UInt8(1)").flippingBit(7).value.description == "UInt8(129)")
    }

    @Test func flipIsItsOwnInverse() {
        let v = view("UInt8(0b1010)")
        #expect(v.flippingBit(3).flippingBit(3).value == v.value)
    }

    @Test func nonIntegersAndStringsAreUnavailable() {
        #expect(unavailable("10.5") == .notAnInteger)
        #expect(unavailable("\"hi\"") == .notAnInteger)
        #expect(unavailable("Decimal(10.5, 5, 2)") == .notAnInteger) // a decimal type, not bits
    }

    @Test func negativePlainNumberSuggestsATypedInt() {
        #expect(unavailable("0 - 5") == .negative)
    }

    @Test func over256BitsIsTooWide() {
        #expect(unavailable("2 ^ 300") == .tooWide) // a 301-bit plain integer
    }

    @Test func minimumWidthTracksTheValue() {
        // The narrowest editable width that holds the value — the UI grays out
        // smaller picker options below this.
        #expect(view("5").minimumWidth == 8)        // 3 bits → 8
        #expect(view("255").minimumWidth == 8)      // 8 bits → 8
        #expect(view("256").minimumWidth == 16)     // 9 bits → 16
        #expect(view("2 ^ 100").minimumWidth == 128) // 101 bits → 128
    }

    @Test func upTo256BitsEdits() {
        #expect(view("UInt256(1)").width == 256)      // the widest fixed type
        #expect(view("Int256(-1)").width == 256)
        #expect(view("2 ^ 100").width == 128)         // 101 bits → 128
        #expect(view("2 ^ 200").width == 256)         // 201 bits → 256
    }
}

@Suite("Bit-field formats")
struct BitFieldTests {
    private func view(_ s: String, preferredWidth: Int = 32) -> BinaryView {
        let value = try! Calculator().evaluate(s).get()
        guard case .value(let v) = value,
              case .success(let view) = BinaryView.make(for: v, preferredWidth: preferredWidth)
        else { fatalError() }
        return view
    }

    private let perms = [BinaryView.FieldSpec(name: "owner", width: 3),
                         BinaryView.FieldSpec(name: "group", width: 3),
                         BinaryView.FieldSpec(name: "other", width: 3)]

    @Test func layoutParsesAMap() {
        let value = try! Calculator().evaluate("{owner: 3, group: 3, other: 3}").get()
        guard case .value(let map) = value else { Issue.record("not a value"); return }
        let layout = BinaryView.layout(from: map)
        #expect(layout?.map(\.name) == ["owner", "group", "other"]) // insertion order preserved
        #expect(layout?.map(\.width) == [3, 3, 3])
        // Not a layout: non-map, or non-integer / non-positive widths.
        #expect(BinaryView.layout(from: .number(BigDecimal(5))) == nil)
    }

    @Test func decodesFieldsHighToLowIntoTheLowBits() {
        // 493 = 0b1_1110_1101 → owner=111(7) group=101(5) other=101(5).
        let fields = view("493").fields(perms)
        #expect(fields.map(\.name) == ["owner", "group", "other"])
        #expect(fields.map(\.value) == [BigInt(7), BigInt(5), BigInt(5)])
        #expect(fields.map(\.lowBit) == [6, 3, 0]) // owner is the highest used range
    }

    @Test func settingAFieldRewritesOnlyItsRange() {
        let v = view("493")                              // owner=7 group=5 other=5
        let edited = v.setting(field: "group", to: BigInt(0), layout: perms)
        #expect(edited.fields(perms).map(\.value) == [BigInt(7), BigInt(0), BigInt(5)])
        #expect(edited.value.displayDescription == "453") // 0b111000101
    }

    @Test func settingClampsToTheFieldWidth() {
        // 9 doesn't fit 3 bits; only the low 3 bits (001) land.
        let v = view("0").setting(field: "other", to: BigInt(9), layout: perms)
        #expect(v.fields(perms)[2].value == BigInt(1))
    }

    @Test func flagFieldsDecodeToTheirMeaning() {
        // 493 = 0o755: owner=rwx, group=r-x, other=r-x.
        let rwx = ["r", "w", "x"]
        let layout = [BinaryView.FieldSpec(name: "owner", width: 3, flags: rwx),
                      BinaryView.FieldSpec(name: "group", width: 3, flags: rwx),
                      BinaryView.FieldSpec(name: "other", width: 3, flags: rwx)]
        let fields = view("493").fields(layout)
        #expect(fields.map(\.flagString) == ["rwx", "r-x", "r-x"])
        // 001 → --x (only the low/execute bit).
        let oneField = [BinaryView.FieldSpec(name: "owner", width: 3, flags: rwx)]
        #expect(view("1").fields(oneField).first?.flagString == "--x")
    }

    @Test func multiCharFlagsListOnlyTheSetOnes() {
        // A 2-flag field, low bit set → lists the set name.
        let layout = [BinaryView.FieldSpec(name: "f", width: 2, flags: ["ACK", "SYN"])]
        #expect(view("1").fields(layout).first?.flagString == "SYN") // bit0 = SYN (low)
        #expect(view("0").fields(layout).first?.flagString == "—")
    }

    @Test func layoutParsesFlagArrays() {
        let value = try! Calculator().evaluate("{owner: [\"r\", \"w\", \"x\"]}").get()
        guard case .value(let map) = value else { Issue.record("not a value"); return }
        let layout = BinaryView.layout(from: map)
        #expect(layout?.first?.width == 3)
        #expect(layout?.first?.flags == ["r", "w", "x"])
    }

    @Test func layoutParsesATypedBitFormatRecord() {
        // The phase-4 bridge: a Bits::BitFormat record reads structurally into a
        // layout — a flags field, an enum field, and a numeric field, chosen by
        // `kind`.
        let calculator = Calculator()
        _ = calculator.evaluate(CalculatorSessionBitsSchema.source)
        let value = try! calculator.evaluate(
            "Bits::BitFormat(fields: ["
            + "Bits::BitField(name: \"owner\", bits: 3, kind: \"flags\", flags: [\"r\", \"w\", \"x\"], values: [], color: \"blue\", base: 10), "
            + "Bits::BitField(name: \"mode\", bits: 2, kind: \"enum\", flags: [], values: [\"idle\", \"run\", \"halt\", \"max\"], color: \"green\", base: 10), "
            + "Bits::BitField(name: \"rest\", bits: 3, kind: \"numeric\", flags: [], values: [], color: \"\", base: 16)])").get()
        guard case .value(let record) = value else { Issue.record("not a value"); return }
        let layout = BinaryView.layout(from: record)
        #expect(layout?.map(\.name) == ["owner", "mode", "rest"])
        #expect(layout?.map(\.width) == [3, 2, 3])
        #expect(layout?.first?.flags == ["r", "w", "x"])
        #expect(layout?.map(\.color) == ["blue", "green", nil]) // empty color → nil (auto)
        #expect(layout?[1].values == ["idle", "run", "halt", "max"])
        #expect(layout?.last?.flags == nil && layout?.last?.values == nil) // numeric
        #expect(layout?.last?.base == 16) // the numeric field reads in hex
    }

    @Test func numericFieldRendersAndParsesInItsBase() {
        // The loose {bits, base} map form round-trips a per-field display base.
        let layout = BinaryView.layout(from:
            BinaryView.numericFormatMap([("oui", 8, 16), ("nic", 8, 10)]))
        #expect(layout?.map(\.base) == [16, nil]) // base 10 normalizes to nil (decimal default)
        // 0x1B44 across two octets → oui=0x1b (hex text), nic=68 (decimal text).
        let fields = view("0x1B44").fields(layout ?? [])
        #expect(fields.map(\.valueText) == ["0x1b", "68"])
        #expect(fields.map(\.label) == ["0x1b", "68"])
    }

    @Test func parseIsTheInverseOfValueText() {
        // Parse in the field's base…
        #expect(BinaryView.parse("27", base: 10) == BigInt(27))
        #expect(BinaryView.parse("1b", base: 16) == BigInt(27))
        // …but an explicit prefix always wins over the base.
        #expect(BinaryView.parse("0x1b", base: 10) == BigInt(27))
        #expect(BinaryView.parse("0b101", base: 10) == BigInt(5))
        #expect(BinaryView.parse("0o17", base: 10) == BigInt(15))
        #expect(BinaryView.parse("zz", base: 16) == nil)
        // Round-trip: a field's valueText parses back to its value, any base.
        for base in [2, 8, 10, 16] {
            let f = BinaryView.Field(name: "f", width: 8, lowBit: 0, value: BigInt(171),
                                     flags: nil, values: nil, base: base)
            #expect(BinaryView.parse(f.valueText, base: base) == BigInt(171))
        }
    }

    @Test func enumFieldDecodesItsValueToALabel() {
        // owner=rwx(7) mode=10(2 = "halt") rest=101(5).  0b111_10_101 = 501.
        let layout = [BinaryView.FieldSpec(name: "owner", width: 3, flags: ["r", "w", "x"]),
                      BinaryView.FieldSpec(name: "mode", width: 2,
                                           values: ["idle", "run", "halt", "max"]),
                      BinaryView.FieldSpec(name: "rest", width: 3)]
        let fields = view("501").fields(layout)
        #expect(fields.map(\.label) == ["rwx", "halt", "5"])
        // A value past the label list shows the raw number.
        let two = [BinaryView.FieldSpec(name: "x", width: 3, values: ["a", "b"])]
        #expect(view("5").fields(two).first?.enumString == "5")
    }
}

/// The `Bits` schema the app's binary editor emits — duplicated here so the
/// engine test of the record bridge doesn't reach into the app target.
private enum CalculatorSessionBitsSchema {
    static let source =
        "namespace Bits { data BitField { name: String, bits: Number, kind: String, "
        + "flags: [String], values: [String], color: String, base: Number }; "
        + "data BitFormat { fields: [BitField] } }"
}

@Suite("ans-prefix continuation")
struct AnsPrefixTests {
    @Test func leadingBinaryOperatorPrefixesAns() {
        #expect(Calculator.ansPrefixed("+5", mode: .normal) == "ans+5")
        #expect(Calculator.ansPrefixed("*2", mode: .normal) == "ans*2")
        #expect(Calculator.ansPrefixed("/4", mode: .normal) == "ans/4")
        #expect(Calculator.ansPrefixed("^2", mode: .normal) == "ans^2")
        #expect(Calculator.ansPrefixed("× 3", mode: .normal) == "ans× 3")
    }

    @Test func minusIsIncludedSpeedCrunchStyle() {
        #expect(Calculator.ansPrefixed("-5", mode: .normal) == "ans-5")
    }

    @Test func leadingSpacesAreTrimmedBeforePrefixing() {
        #expect(Calculator.ansPrefixed("  + 5", mode: .normal) == "ans+ 5")
    }

    @Test func nonOperatorLeadsDoNotPrefix() {
        #expect(Calculator.ansPrefixed("5 + 3", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("sqrt(2)", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("(1+2)", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("~5", mode: .programmer) == nil) // ~ is unary prefix
    }

    @Test func percentAndBitGlyphsAreOperatorsOnlyInProgrammer() {
        // Normal: % is postfix percent, bit glyphs aren't operators → no prefix.
        #expect(Calculator.ansPrefixed("%5", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("<<2", mode: .normal) == nil)
        #expect(Calculator.ansPrefixed("&3", mode: .normal) == nil)
        // Programmer: they lead a continuation.
        #expect(Calculator.ansPrefixed("%5", mode: .programmer) == "ans%5")
        #expect(Calculator.ansPrefixed("<<2", mode: .programmer) == "ans<<2")
        #expect(Calculator.ansPrefixed("&3", mode: .programmer) == "ans&3")
    }
}

@Suite("Format builder")
struct FormatBuilderTests {
    private func builder() -> BinaryView.FormatBuilder {
        BinaryView.FormatBuilder(palette: ["blue", "green", "orange"])
    }

    @Test func claimToggleClears() {
        var b = builder()
        b.claim(5)
        #expect(b.pendingWidth == 5)
        b.claim(5) // clicking the same far edge clears
        #expect(b.pendingWidth == 0)
        b.claim(3)
        #expect(b.pendingWidth == 3)
    }

    @Test func addFieldBuildsAKindedSpecAndResetsTheDraft() {
        var b = builder()
        // A flags field.
        b.claim(3); b.draftName = "owner"; b.draftKind = .flags; b.draftLabels = "r, w, x"
        b.addField()
        // Draft reset, color advanced to the next palette entry.
        #expect(b.pendingWidth == 0 && b.draftName == "" && b.draftKind == .numeric)
        #expect(b.draftColor == "green")
        // An enum field.
        b.claim(2); b.draftName = "mode"; b.draftKind = .enumeration; b.draftLabels = "idle, run, halt, max"
        b.addField()
        // A hex numeric field.
        b.claim(8); b.draftName = "rest"; b.draftKind = .numeric; b.draftBase = 16
        b.addField()

        let layout = b.layout
        #expect(layout.map(\.name) == ["owner", "mode", "rest"])
        #expect(layout.map(\.width) == [3, 2, 8])
        #expect(layout[0].flags == ["r", "w", "x"])
        #expect(layout[0].color == "blue")
        #expect(layout[1].values == ["idle", "run", "halt", "max"])
        #expect(layout[1].color == "green")
        #expect(layout[2].base == 16 && layout[2].flags == nil && layout[2].values == nil)
    }

    @Test func flagsPadAndTruncateToWidth() {
        var b = builder()
        b.claim(3); b.draftName = "f"; b.draftKind = .flags; b.draftLabels = "a, b" // short
        b.addField()
        #expect(b.layout[0].flags == ["a", "b", "?"])
        b.claim(2); b.draftName = "g"; b.draftKind = .flags; b.draftLabels = "a, b, c, d" // long
        b.addField()
        #expect(b.layout[1].flags == ["a", "b"])
    }

    @Test func canAddFieldGuardsNameAndClaim() {
        var b = builder()
        #expect(!b.canAddField)            // nothing claimed, no name
        b.claim(3)
        #expect(!b.canAddField)            // claimed, still no name
        b.draftName = "  "
        #expect(!b.canAddField)            // blank name
        b.draftName = "x"
        #expect(b.canAddField)
        b.addField()
        #expect(b.layout.count == 1)
        // A no-op add (nothing claimed) leaves the builder unchanged.
        b.addField()
        #expect(b.layout.count == 1)
    }

    @Test func removeAndRecolor() {
        var b = builder()
        b.claim(3); b.draftName = "a"; b.addField()
        b.claim(3); b.draftName = "b"; b.addField()
        let firstID = b.fields[0].id
        b.recolor(firstID, to: "teal")
        #expect(b.fields[0].colorName == "teal")
        b.remove(firstID)
        #expect(b.fields.map(\.name) == ["b"])
    }

    @Test func widthArithmetic() {
        var b = builder()
        b.claim(3); b.draftName = "a"; b.addField()
        b.claim(5); b.draftName = "b"; b.addField()
        #expect(b.committedWidth == 8)
        #expect(b.freeBits(registerWidth: 16) == 8)
        #expect(b.freeBits(registerWidth: 4) == 0) // never negative
    }

    @Test func seedRoundTripsAnExistingLayout() {
        let original = [
            BinaryView.FieldSpec(name: "owner", width: 3, flags: ["r", "w", "x"], color: "blue"),
            BinaryView.FieldSpec(name: "rest", width: 5, color: "green", base: 16),
        ]
        var b = builder()
        b.seed(from: original)
        #expect(b.fields.map(\.name) == ["owner", "rest"])
        #expect(b.fields[0].kind == .flags)
        #expect(b.fields[1].kind == .numeric && b.fields[1].base == 16)
        // The layout reconstructs the same specs.
        #expect(b.layout == original)
    }
}
