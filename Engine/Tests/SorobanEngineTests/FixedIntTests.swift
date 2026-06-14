import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Fixed-width integers — foundation")
struct FixedIntFoundationTests {
    private func outcome(_ s: String) -> Result<EvalOutcome, EngineError> {
        Calculator().evaluate(s)
    }

    @Test func bothFormsConstructAndShowPerWidth() throws {
        // Per-width and parameterized forms are equivalent; the canonical
        // (description) is always the per-width spelling.
        #expect(try outcome("Int32(27374)").get().description == "Int32(27374)")
        #expect(try outcome("Int(27374, 32)").get().description == "Int32(27374)")
        #expect(try outcome("UInt8(255)").get().description == "UInt8(255)")
        #expect(try outcome("Int8(-1)").get().description == "Int8(-1)")
        #expect(try outcome("Int(5, 8) == Int8(5)").get() == .value(BigDecimal(1)))
    }

    @Test func rangeAndWidthAreChecked() {
        #expect(outcome("Int8(200)").isFailure)    // > 127
        #expect(outcome("UInt8(-1)").isFailure)     // < 0
        #expect(outcome("UInt8(256)").isFailure)    // > 255
        #expect(outcome("Int8(128)").isFailure)     // signed max is 127
        #expect(outcome("Int(5, 7)").isFailure)     // 7 isn't an allowed width
        #expect(outcome("Int8(1.5)").isFailure)     // not an integer — no silent truncation
    }

    @Test func boundariesAreInclusive() throws {
        #expect(try outcome("Int8(127)").get().description == "Int8(127)")
        #expect(try outcome("Int8(-128)").get().description == "Int8(-128)")
        #expect(try outcome("UInt256(100)").get().description == "UInt256(100)")
    }

    @Test func coercesNumericallyOutsideTypedArithmetic() throws {
        #expect(try outcome("Int8(5) == 5").get() == .value(BigDecimal(1)))
        #expect(try outcome("Int8(5) < 10").get() == .value(BigDecimal(1)))
        #expect(try outcome("sum(Int8(2), Int8(3))").get() == .value(BigDecimal(5)))
        #expect(try outcome("if(Int8(1), 10, 20)").get() == .value(BigDecimal(10)))
    }

    @Test func descriptionReParsesByEvaluation() throws {
        let once = try outcome("Int16(42)").get().description
        let twice = try outcome(once).get().description
        #expect(once == "Int16(42)")
        #expect(once == twice)
    }

    @Test func displayIsThePlainNumberCanonicalStaysTyped() throws {
        // Hosts ECHO the clean number; description (recall/copy/persist) keeps
        // the typed constructor so the type survives a round trip.
        let big = try outcome("Int32(343353)").get()
        #expect(big.displayDescription == "343353")
        #expect(big.description == "Int32(343353)")

        let negative = try outcome("Int8(-1)").get()
        #expect(negative.displayDescription == "-1")
        #expect(negative.description == "Int8(-1)")
    }
}

@Suite("Fixed-width integers — arithmetic")
struct FixedIntArithmeticTests {
    private func outcome(_ s: String) -> Result<EvalOutcome, EngineError> {
        Calculator().evaluate(s)
    }

    @Test func checkedArithmeticKeepsTheType() throws {
        #expect(try outcome("Int8(5) + Int8(3)").get().description == "Int8(8)")
        #expect(try outcome("Int8(10) - Int8(4)").get().description == "Int8(6)")
        #expect(try outcome("Int8(5) * Int8(4)").get().description == "Int8(20)")
        #expect(try outcome("Int8(7) / Int8(2)").get().description == "Int8(3)") // truncating
    }

    @Test func literalsAdoptTheType() throws {
        #expect(try outcome("Int8(5) + 3").get().description == "Int8(8)")
        #expect(try outcome("100 + Int16(5)").get().description == "Int16(105)")
    }

    @Test func widthPromotesToTheLargest() throws {
        #expect(try outcome("Int(100, 8) + Int(100, 16)").get().description == "Int16(200)")
    }

    @Test func powerFollowsTheBaseType() throws {
        #expect(try outcome("Int8(2) ^ 3").get().description == "Int8(8)")
    }

    @Test func overflowErrorsNeverWraps() {
        #expect(outcome("Int8(100) + Int8(100)").isFailure)  // 200 > 127, no wrap
        #expect(outcome("Int(2, 32) ^ 40").isFailure)            // 2^40 > Int32 max
        #expect(outcome("UInt8(0) - 1").isFailure)               // underflow, no wrap to 255
        #expect(outcome("Int8(0) * 1000").isFailure)             // literal 1000 can't adopt Int8
    }

    @Test func signNeverPromotes() {
        #expect(outcome("Int8(5) + UInt8(5)").isFailure)
    }

    @Test func decimalsAndFractionsDontMix() {
        #expect(outcome("Int8(5) + 1.5").isFailure) // fractional, no truncation
    }

    @Test func plainNumericArithmeticIsUnaffected() throws {
        #expect(try outcome("5 + 3").get() == .value(BigDecimal(8)))
        #expect(try outcome("0.1 + 0.2 == 0.3").get() == .value(BigDecimal(1)))
    }
}

@Suite("Fixed-width integers — bitwise (two's-complement)")
struct FixedIntBitwiseTests {
    private func prog(_ s: String) -> Result<EvalOutcome, EngineError> {
        let c = Calculator(); c.mode = .programmer
        return c.evaluate(s)
    }

    @Test func bitwiseKeepsTheTypeUnsigned() throws {
        #expect(try prog("UInt8(12) & UInt8(10)").get().description == "UInt8(8)")
        #expect(try prog("UInt8(12) | UInt8(3)").get().description == "UInt8(15)")
        #expect(try prog("UInt8(12) ^ UInt8(10)").get().description == "UInt8(6)")
    }

    @Test func twosComplementForSigned() throws {
        #expect(try prog("~UInt8(0)").get().description == "UInt8(255)")
        #expect(try prog("~Int8(0)").get().description == "Int8(-1)")     // ~0 = -1
        #expect(try prog("~Int8(5)").get().description == "Int8(-6)")     // ~x = -x-1
        #expect(try prog("Int8(-1) & Int8(3)").get().description == "Int8(3)") // 0xFF & 0x03
    }

    @Test func shiftsAreCheckedForFixedWidth() throws {
        #expect(try prog("UInt8(1) << 4").get().description == "UInt8(16)")
        #expect(try prog("UInt8(128) >> 2").get().description == "UInt8(32)")
        #expect(prog("UInt8(1) << 8").isFailure)   // 256 leaves UInt8 → overflow
        #expect(prog("Int8(64) << 1").isFailure)   // 128 > Int8 max
    }

    @Test func plainBitwisePathUnchanged() throws {
        #expect(try prog("12 & 10").get() == .value(BigDecimal(8)))  // no fixedInt → arbitrary width
        #expect(prog("bitNot(5)").isFailure)                         // a plain number has no width
    }
}

@Suite("Fixed-width integers — edges")
struct FixedIntEdgeTests {
    private func outcome(_ s: String) -> Result<EvalOutcome, EngineError> {
        Calculator().evaluate(s)
    }

    @Test func powerWithNumericBaseAndFixedExponent() throws {
        // Numeric base, fixed-width exponent: the exponent is a count → ordinary
        // numeric power (a plain number, not a fixedInt).
        #expect(try outcome("2 ^ Int8(3)").get() == .value(BigDecimal(8)))
    }

    @Test func fixedWidthPowerRejectsBadExponent() {
        #expect(outcome("Int8(2) ^ (-1)").isFailure)
    }

    @Test func nonNumberOperandErrors() {
        #expect(outcome("Int8(5) * \"x\"").isFailure)
    }

    @Test func divideByZeroErrors() {
        #expect(outcome("Int8(5) / Int8(0)").isFailure)
    }

    @Test func toJsonRendersTheNumber() throws {
        #expect(try outcome("toJson(Int32(255))").get().description.contains("255"))
    }
}
