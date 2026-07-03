import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Fixed-precision decimals — foundation")
struct FixedDecimalFoundationTests {
    private func outcome(_ s: String) -> Result<EvalOutcome, EngineError> {
        Calculator().evaluate(s)
    }

    @Test func constructAndDisplayPadsToScale() throws {
        #expect(try outcome("Decimal(10.5, 5, 2)").get().description == "Decimal(10.50, 5, 2)")
        #expect(try outcome("Decimal(0.05, 5, 2)").get().description == "Decimal(0.05, 5, 2)")
        #expect(try outcome("Decimal(123, 5, 0)").get().description == "Decimal(123, 5, 0)")
        #expect(try outcome("Decimal(-1.5, 5, 2)").get().description == "Decimal(-1.50, 5, 2)")
    }

    @Test func roundingModes() throws {
        // banker's (default): half goes to the even neighbor → 1.00
        #expect(try outcome("Decimal(1.005, 5, 2)").get().description == "Decimal(1.00, 5, 2)")
        // half-up: half goes away from zero → 1.01, carried in the description
        #expect(try outcome("Decimal(1.005, 5, 2, Rounding.HalfUp)").get().description
                == "Decimal(1.01, 5, 2, Rounding.HalfUp)")
        #expect(try outcome("Decimal(2.675, 5, 2, Rounding.HalfUp)").get().description
                == "Decimal(2.68, 5, 2, Rounding.HalfUp)")
        // the string form (what the constant resolves to) works too
        #expect(outcome("Decimal(1, 5, 2, Rounding.Bankers)").isFailure == false)
    }

    @Test func precisionOverflowAndBadArgsError() {
        #expect(outcome("Decimal(12345, 4, 0)").isFailure)  // 5 digits > precision 4
        #expect(outcome("Decimal(1234.5, 5, 2)").isFailure) // rounds to 1234.50 → 6 digits
        #expect(outcome("Decimal(1, 2, 3)").isFailure)      // scale > precision
        #expect(outcome("Decimal(1, 0, 0)").isFailure)      // precision < 1
    }

    @Test func precisionAndScaleAreCappedAt1000() throws {
        // 1000 is the ceiling; scale rides at most to precision.
        #expect(outcome("Decimal(0.5, 1000, 1000)").isFailure == false)
        #expect(outcome("Decimal(1, 1001, 2)").isFailure)      // precision > 1000
        #expect(outcome("Decimal(0.5, 1001, 1001)").isFailure) // precision (and scale) > 1000
    }

    @Test func shortFormsDefaultToMaxPrecisionAndHideIt() throws {
        // 1-arg: scale from the value, lossless; the canonical form hides the
        // default precision (recalls as the bare Decimal(value)).
        #expect(try outcome("Decimal(0.5)").get().description == "Decimal(0.5)")
        #expect(try outcome("Decimal(3.14159)").get().description == "Decimal(3.14159)") // no rounding
        #expect(try outcome("Decimal(100)").get().description == "Decimal(100)")
        // 2-arg: explicit scale, precision still hidden. The 2-arg spelling
        // survives only when the scale exceeds the value's natural places
        // (Decimal(0.5, 2) → 0.50); when they coincide it shortens to 1-arg
        // (Decimal(3.14159, 2) rounds to 3.14, whose natural scale is already 2).
        #expect(try outcome("Decimal(0.5, 2)").get().description == "Decimal(0.50, 2)")
        #expect(try outcome("Decimal(3.14159, 2)").get().description == "Decimal(3.14)") // explicit round, natural scale 2
        // Round-trips by evaluation.
        let canonical = try outcome("Decimal(0.5, 2)").get().description
        #expect(try outcome(canonical).get().description == canonical)
        // Max-precision headroom: ordinary arithmetic doesn't overflow.
        #expect(try outcome("Decimal(0.5) + Decimal(0.5)").get().description == "Decimal(1.0, 1)")
        // Display stays the clean number.
        #expect(try outcome("Decimal(0.5)").get().displayDescription == "0.5")
    }

    @Test func coercesNumericallyOutsideArithmetic() throws {
        #expect(try outcome("Decimal(10.50, 5, 2) == 10.5").get() == .value(BigDecimal(1)))
        #expect(try outcome("Decimal(2.50, 5, 2) < 3").get() == .value(BigDecimal(1)))
        #expect(try outcome("sum(Decimal(1.5, 5, 2), Decimal(2.5, 5, 2))").get() == .value(BigDecimal(4)))
    }

    @Test func reParsesByEvaluation() throws {
        let once = try outcome("Decimal(3.14159, 6, 2)").get().description
        let twice = try outcome(once).get().description
        #expect(once == "Decimal(3.14, 6, 2)")
        #expect(once == twice)
    }

    @Test func displayIsThePaddedNumberCanonicalStaysTyped() throws {
        // Hosts ECHO the padded number; description (recall/copy/persist) keeps
        // the typed Decimal(…) constructor so precision/scale survive.
        let money = try outcome("Decimal(10.5, 5, 2)").get()
        #expect(money.displayDescription == "10.50")
        #expect(money.description == "Decimal(10.50, 5, 2)")

        let cents = try outcome("Decimal(0.05, 5, 2)").get()
        #expect(cents.displayDescription == "0.05")
        #expect(cents.description == "Decimal(0.05, 5, 2)")
    }
}

@Suite("Fixed-precision decimals — arithmetic")
struct FixedDecimalArithmeticTests {
    private func outcome(_ s: String) -> Result<EvalOutcome, EngineError> {
        Calculator().evaluate(s)
    }

    @Test func keepsScaleAndPrecision() throws {
        #expect(try outcome("Decimal(2.50, 5, 2) + Decimal(1.25, 5, 2)").get().description == "Decimal(3.75, 5, 2)")
        #expect(try outcome("Decimal(10.00, 5, 2) - Decimal(0.01, 5, 2)").get().description == "Decimal(9.99, 5, 2)")
        #expect(try outcome("Decimal(2.50, 5, 2) * Decimal(4, 5, 2)").get().description == "Decimal(10.00, 5, 2)")
        #expect(try outcome("Decimal(10, 5, 2) / Decimal(3, 5, 2)").get().description == "Decimal(3.33, 5, 2)") // rounds to scale
    }

    @Test func widthsPromoteToWidest() throws {
        #expect(try outcome("Decimal(1.5, 5, 2) + Decimal(1.555, 8, 3)").get().description == "Decimal(3.055, 8, 3)")
    }

    @Test func absorbsAndRoundsPlainNumber() throws {
        #expect(try outcome("Decimal(10.00, 5, 2) + 0.005").get().description == "Decimal(10.00, 5, 2)") // banker's
        #expect(try outcome("Decimal(10.00, 5, 2, Rounding.HalfUp) + 0.005").get().description
                == "Decimal(10.01, 5, 2, Rounding.HalfUp)")
    }

    @Test func overflowAndBadMixesError() {
        #expect(outcome("Decimal(999.99, 5, 2) + Decimal(0.01, 5, 2)").isFailure)          // 1000.00 > precision 5
        #expect(outcome("Decimal(1, 5, 2) + Decimal(1, 5, 2, Rounding.HalfUp)").isFailure) // rounding mismatch
        #expect(outcome("Decimal(5, 5, 2) + int(5, 8)").isFailure)                         // cross-family
        #expect(outcome("Decimal(2, 5, 2) ^ 3").isFailure)                                 // power unsupported
    }

    @Test func plainNumericArithmeticIsUnaffected() throws {
        #expect(try outcome("2.50 + 1.25").get() == .value(BigDecimal(string: "3.75")!))
        #expect(try outcome("0.1 + 0.2 == 0.3").get() == .value(BigDecimal(1)))
    }
}
