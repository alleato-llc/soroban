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
