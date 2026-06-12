import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Sliders (control expressions)")
struct SliderTests {
    private func makeStore() -> (Calculator, SheetStore) {
        let calc = Calculator()
        return (calc, SheetStore(calculator: calc))
    }

    private func addr(_ column: Int, _ row: Int) -> CellAddress {
        CellAddress(column: column, row: row)
    }

    @Test func builtinEvaluatesAndClamps() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("slider(5, 0, 10)").get() == .value(BigDecimal(5)))
        #expect(try calc.evaluate("slider(15, 0, 10)").get() == .value(BigDecimal(10)))
        #expect(try calc.evaluate("slider(-1, 0, 10)").get() == .value(BigDecimal(0)))
        #expect(calc.evaluate("slider(1, 10, 0)").isFailure)      // min < max
        #expect(calc.evaluate("slider(1, 0, 10, 0)").isFailure)   // step > 0
        #expect(calc.evaluate("slider(1)").isFailure)             // arity
    }

    @Test func infoExtraction() throws {
        let info = try #require(SliderInfo.extract(
            from: try Parser.parse("slider(0.08, 0, 0.2)"), name: "rate"))
        #expect(info.name == "rate")
        #expect(info.value == BigDecimal(string: "0.08")!)
        #expect(info.step == BigDecimal(string: "0.002")!) // (max−min)/100

        // Explicit step + negative literals + clamping.
        let stepped = try #require(SliderInfo.extract(
            from: try Parser.parse("slider(-5, -10, 10, 0.5)"), name: nil))
        #expect(stepped.step == BigDecimal(string: "0.5")!)
        #expect(stepped.value == BigDecimal(-5))
        let clamped = try #require(SliderInfo.extract(
            from: try Parser.parse("slider(99, 0, 10)"), name: nil))
        #expect(clamped.value == BigDecimal(10))

        // Non-literal arguments aren't controls (the value IS the storage).
        #expect(SliderInfo.extract(from: try Parser.parse("slider(A:1, 0, 10)"), name: nil) == nil)
        #expect(SliderInfo.extract(from: try Parser.parse("slider(1 + 1, 0, 10)"), name: nil) == nil)
        // Invalid ranges fall through to evaluation (which errors).
        #expect(SliderInfo.extract(from: try Parser.parse("slider(1, 10, 0)"), name: nil) == nil)
    }

    @Test func dragGeometry() throws {
        let info = try #require(SliderInfo.extract(
            from: try Parser.parse("slider(5, 0, 10, 1)"), name: nil))
        #expect(info.fraction == 0.5)
        #expect(info.value(atFraction: 0) == BigDecimal(0))
        #expect(info.value(atFraction: 1) == BigDecimal(10))
        #expect(info.value(atFraction: 0.349) == BigDecimal(3)) // quantized to step
        #expect(info.value(atFraction: -2) == BigDecimal(0))    // clamped
    }

    @Test func cellsRenderSliders() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = slider(0.08, 0, 0.2)", at: addr(0, 0)) // named
        grid.setCell("=slider(5, 0, 10)", at: addr(0, 1))           // anonymous, marked
        grid.setCell("slider(3, 0, 6)", at: addr(0, 2))             // anonymous, plain

        guard case .slider(let named) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("definition slider should display as a control"); return
        }
        #expect(named.name == "rate")

        guard case .slider = grid.displayValue(at: addr(0, 1)),
              case .slider = grid.displayValue(at: addr(0, 2)) else {
            Issue.record("anonymous sliders should display as controls"); return
        }

        // Values flow: by name for the definition, by address for anonymous.
        #expect(try calc.evaluate("rate * 100").get() == .value(BigDecimal(8)))
        #expect(try calc.evaluate("A:2 + A:3").get() == .value(BigDecimal(8)))
        #expect(try calc.evaluate("sum(A:1..A:3)").get()
                == .value(BigDecimal(string: "8.08")!)) // ranges include sliders
    }

    @Test func overridesPreviewWithoutRewriting() throws {
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = slider(0.08, 0, 0.2)", at: addr(0, 0))
        grid.setCell("=rate * 100", at: addr(0, 1))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(8)))

        // Mid-drag: the override feeds resolution; the raw is untouched.
        grid.sliderOverrides[addr(0, 0)] = BigDecimal(string: "0.15")!
        grid.recalculate()
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(15)))
        guard case .slider(let info) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("still a slider mid-drag"); return
        }
        #expect(info.value == BigDecimal(string: "0.15")!)
        #expect(grid.raw(at: addr(0, 0)) == "rate = slider(0.08, 0, 0.2)")

        // Release: override cleared, raw rewritten.
        grid.sliderOverrides.removeAll()
        let rewritten = try #require(Slider.rewriting(grid.raw(at: addr(0, 0)),
                                                      to: BigDecimal(string: "0.15")!))
        grid.setCell(rewritten, at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(15)))
        _ = calc // keep resolvers alive
    }

    @Test func previewInvalidatesOnlyThroughRecordedEdges() throws {
        // The big-workbook responsiveness contract: a drag tick must NOT need
        // store.recalculate() — the definition-read edge recorded when A:2
        // evaluated carries the targeted invalidation to the reader.
        let (calc, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("rate = slider(0.08, 0, 0.2)", at: addr(0, 0))
        grid.setCell("=rate * 100", at: addr(0, 1))
        grid.setCell("=2 + 2", at: addr(0, 2)) // unrelated — keeps its memo
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(8)))
        #expect(grid.displayValue(at: addr(0, 2)) == .value(BigDecimal(4)))

        grid.setSliderOverride(BigDecimal(string: "0.15")!, at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(15)))
        guard case .slider(let mid) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("still a slider mid-drag"); return
        }
        #expect(mid.value == BigDecimal(string: "0.15")!)

        // Cancelled drag: clearing restores the stored literal's value.
        grid.clearSliderOverride(at: addr(0, 0))
        #expect(grid.displayValue(at: addr(0, 1)) == .value(BigDecimal(8)))

        // Anonymous sliders ride ordinary cell edges the same way.
        grid.setCell("slider(3, 0, 6)", at: addr(1, 0))
        grid.setCell("=B:1 * 10", at: addr(1, 1))
        #expect(grid.displayValue(at: addr(1, 1)) == .value(BigDecimal(30)))
        grid.setSliderOverride(BigDecimal(5), at: addr(1, 0))
        #expect(grid.displayValue(at: addr(1, 1)) == .value(BigDecimal(50)))
        _ = calc // keep resolvers alive
    }

    @Test func rewritingIsTokenPrecise() {
        #expect(Slider.rewriting("rate = slider(0.08, 0, 0.2)  # base case",
                                 to: BigDecimal(string: "0.11")!)
                == "rate = slider(0.11, 0, 0.2)  # base case")
        #expect(Slider.rewriting("=slider( -5 , -10, 10)", to: BigDecimal(7))
                == "=slider( 7 , -10, 10)")
        #expect(Slider.rewriting("slider(3,0,6)", to: BigDecimal(string: "4.5")!)
                == "slider(4.5,0,6)")
        #expect(Slider.rewriting("no control here", to: BigDecimal(1)) == nil)
        #expect(Slider.rewriting("slider(A:1, 0, 1)", to: BigDecimal(1)) == nil)
    }

    @Test func checkboxBasics() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("checkbox(true)").get() == .value(BigDecimal(1)))
        #expect(try calc.evaluate("checkbox(false)").get() == .value(BigDecimal(0)))
        #expect(try calc.evaluate("checkbox(7)").get() == .value(BigDecimal(1))) // truthy

        let (_, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("flag = checkbox(true)", at: addr(0, 0))
        guard case .checkbox(let info) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("checkbox cell should display as a control"); return
        }
        #expect(info.isOn)
        #expect(info.name == "flag")
        #expect(try grid.numericValue(column: "A", row: 1) == BigDecimal(1))

        // Toggle = rewrite the literal.
        let toggled = try #require(Control.rewriting("flag = checkbox(true)  # gate",
                                                     toLiteral: "false"))
        #expect(toggled == "flag = checkbox(false)  # gate")
    }

    @Test func stepperBasics() throws {
        let calc = Calculator()
        #expect(try calc.evaluate("stepper(5, 1, 20)").get() == .value(BigDecimal(5)))
        #expect(try calc.evaluate("stepper(50, 1, 20)").get() == .value(BigDecimal(20))) // clamp
        #expect(calc.evaluate("stepper(1, 5, 2)").isFailure)

        let info = try #require(SliderInfo.extract(
            from: try Parser.parse("stepper(5, 1, 20)"), name: "n", function: "stepper"))
        #expect(info.step == BigDecimal(1)) // stepper default step is 1, not range/100

        let (_, store) = makeStore()
        store.activeSheet.grid.setCell("n = stepper(5, 1, 20)", at: addr(0, 0))
        guard case .stepper = store.activeSheet.grid.displayValue(at: addr(0, 0)) else {
            Issue.record("stepper cell should display as a control"); return
        }
        #expect(try store.activeSheet.grid.numericValue(column: "A", row: 1) == BigDecimal(5))
    }

    @Test func dropdownBasics() throws {
        let calc = Calculator()
        // The value of the cell IS the selected option.
        #expect(try calc.evaluate("dropdown(\"EU\", [\"EU\", \"US\"])").get().description
                == "\"EU\"")
        #expect(try calc.evaluate("dropdown(5, [1, 5, 10])").get() == .value(BigDecimal(5)))
        #expect(calc.evaluate("dropdown(1, 2)").isFailure) // options must be an array

        let (calc2, store) = makeStore()
        let grid = store.activeSheet.grid
        grid.setCell("region = dropdown(\"EU\", [\"EU\", \"US\", \"APAC\"])", at: addr(0, 0))
        guard case .dropdown(let info) = grid.displayValue(at: addr(0, 0)) else {
            Issue.record("dropdown cell should display as a control"); return
        }
        #expect(info.name == "region")
        #expect(info.value == .string("EU"))
        #expect(info.options.count == 3)

        // Strings flow by NAME (== comparisons); numeric use errors like text.
        #expect(try calc2.evaluate("if(region == \"EU\", 10, 20)").get()
                == .value(BigDecimal(10)))
        #expect(calc2.evaluate("A:1 + 1").isFailure)

        // Choosing an option = rewriting the string literal.
        let chosen = try #require(Control.rewriting(grid.raw(at: addr(0, 0)),
                                                    toLiteral: "\"US\""))
        grid.setCell(chosen, at: addr(0, 0))
        #expect(try calc2.evaluate("if(region == \"US\", 10, 20)").get()
                == .value(BigDecimal(10)))

        // Numeric dropdowns participate in ranges; string ones skip.
        grid.setCell("dropdown(5, [1, 5, 10])", at: addr(0, 1))
        #expect(try calc2.evaluate("sum(A:1..A:2)").get() == .value(BigDecimal(5)))
    }

    @Test func sliderDefinitionsStayImmutableFromTheLog() throws {
        let (calc, store) = makeStore()
        store.activeSheet.grid.setCell("rate = slider(0.08, 0, 0.2)", at: addr(0, 0))
        guard case .failure(let error) = calc.evaluate("rate = 0.5") else {
            Issue.record("slider definitions are cell-owned"); return
        }
        #expect("\(error)".contains("A:1"))
    }
}
