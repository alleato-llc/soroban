import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Language modes — Programmer-mode parsing")
struct ProgrammerModeParsingTests {
    private func prog() -> Calculator {
        let c = Calculator()
        c.mode = .programmer
        return c
    }

    @Test func bitwiseGlyphsMapToCanonicalOps() throws {
        let c = prog()
        #expect(try c.evaluate("5 ^ 3").get() == .value(BigDecimal(6)))     // XOR  101^011=110
        #expect(try c.evaluate("12 & 10").get() == .value(BigDecimal(8)))   // AND  1100&1010=1000
        #expect(try c.evaluate("12 | 3").get() == .value(BigDecimal(15)))   // OR   1100|0011=1111
        #expect(try c.evaluate("1 << 4").get() == .value(BigDecimal(16)))   // shift left
        #expect(try c.evaluate("8 >> 2").get() == .value(BigDecimal(2)))    // shift right
        #expect(try c.evaluate("17 % 5").get() == .value(BigDecimal(2)))    // modulo
    }

    @Test func powerIsTheFunctionInProgrammerMode() throws {
        let c = prog()
        // `^` is XOR here, so power is written pow(); the builtin already exists.
        #expect(try c.evaluate("pow(2, 3)").get() == .value(BigDecimal(8)))
    }

    @Test func pythonBitwisePrecedence() throws {
        let c = prog()
        #expect(try c.evaluate("2 + 3 & 1").get() == .value(BigDecimal(1)))  // bitwise below arithmetic: (2+3)&1
        #expect(try c.evaluate("1 | 2 & 3").get() == .value(BigDecimal(3)))  // AND tighter than OR: 1|(2&3)
        #expect(try c.evaluate("1 & 1 == 1").get() == .value(BigDecimal(1))) // bitwise above comparison: (1&1)==1 — no C trap
    }

    @Test func outOfModeGlyphsErrorInNormal() {
        let c = Calculator() // .normal
        for line in ["5 & 3", "5 | 3", "1 << 2", "8 >> 1"] {
            guard case .failure(let error) = c.evaluate(line) else {
                Issue.record("expected failure for \(line)"); continue
            }
            #expect("\(error)".contains("Programmer-mode operator"), "\(line): \(error)")
        }
    }

    @Test func normalModeStillMeansPowerAndPercent() throws {
        let c = Calculator() // .normal
        #expect(try c.evaluate("2 ^ 3").get() == .value(BigDecimal(8)))               // power
        #expect(try c.evaluate("3%").get() == .value(BigDecimal(string: "0.03")!))    // percent
        #expect(try c.evaluate("2^3^2").get() == .value(BigDecimal(512)))             // right-assoc power
    }

    @Test func financeModeMatchesNormalGrammar() throws {
        let c = Calculator()
        c.mode = .finance
        // Finance is grammatically identical to Normal today (a placeholder for
        // future finance display defaults) — pin it so a later fork trips here.
        #expect(try c.evaluate("2 ^ 3").get() == .value(BigDecimal(8)))            // power, not XOR
        #expect(try c.evaluate("3%").get() == .value(BigDecimal(string: "0.03")!)) // percent, not modulo
        #expect(c.evaluate("5 & 3").isFailure)                                     // & is Programmer-only
    }
}

@Suite("Language modes — rendering")
struct ModeRenderingTests {
    private func render(_ src: String, parse pmode: LanguageMode, show smode: LanguageMode) throws -> String {
        try Parser.parse(src, mode: pmode).sourceText(mode: smode)
    }

    @Test func canonicalRendersAsGlyphsInProgrammer() throws {
        #expect(try render("bitXor(5, 3)", parse: .normal, show: .programmer) == "(5 ^ 3)")
        #expect(try render("bitAnd(12, 10)", parse: .normal, show: .programmer) == "(12 & 10)")
        #expect(try render("bitOr(12, 3)", parse: .normal, show: .programmer) == "(12 | 3)")
        #expect(try render("mod(17, 5)", parse: .normal, show: .programmer) == "(17 % 5)")
        #expect(try render("bitShift(1, 4)", parse: .normal, show: .programmer) == "(1 << 4)")
        #expect(try render("bitShift(8, -2)", parse: .normal, show: .programmer) == "(8 >> 2)")
        #expect(try render("2 ^ 3", parse: .normal, show: .programmer) == "pow(2, 3)")
        #expect(try render("3%", parse: .normal, show: .programmer) == "(3 * 0.01)")
    }

    @Test func glyphsRenderAsCanonicalInNormal() throws {
        #expect(try render("5 ^ 3", parse: .programmer, show: .normal) == "bitXor(5, 3)")
        #expect(try render("12 & 10", parse: .programmer, show: .normal) == "bitAnd(12, 10)")
        #expect(try render("17 % 5", parse: .programmer, show: .normal) == "mod(17, 5)")
        #expect(try render("8 >> 2", parse: .programmer, show: .normal) == "bitShift(8, (-2))")
    }

    @Test func programmerRoundTripIsStable() throws {
        for src in ["(5 ^ 3)", "(12 & 10)", "(1 << 4)", "(8 >> 2)", "(17 % 5)", "pow(2, 3)", "(1 | (2 & 3))"] {
            let once = try render(src, parse: .programmer, show: .programmer)
            let twice = try Parser.parse(once, mode: .programmer).sourceText(mode: .programmer)
            #expect(once == twice, "\(src) → \(once) → \(twice)")
        }
    }
}
