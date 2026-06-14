import Testing
import SorobanEngine

/// Coverage for two recent session-layer features the model layer owns:
/// font-proportional grid geometry (SheetModel+Layout) and the dialect-switch
/// log marker (`HistoryEntry.Outcome.mode` → `LogRecord`). The view-model glue
/// around them (CalculatorSession's append-on-switch, the pinch/⌘± gestures) is
/// build-verified only — it isn't compiled into this target.
@MainActor
@Suite("Recent session-layer features")
struct RecentSessionFeatureTests {
    private func makeSheet() -> SheetModel {
        let sheet = SheetModel(calculator: Calculator())
        sheet.autosaveToScratch = false // never touch the real scratch file
        return sheet
    }

    // MARK: Font-proportional grid

    @Test func gridDefaultsScaleWithFont() {
        let sheet = makeSheet()
        sheet.gridFontSize = 14 // the base the defaults are tuned for
        #expect(sheet.defaultColumnWidthScaled == 92)
        #expect(sheet.defaultRowHeightScaled == 24)

        sheet.gridFontSize = 28 // double the font → double the cell geometry
        #expect(sheet.defaultColumnWidthScaled == 184)
        #expect(sheet.defaultRowHeightScaled == 48)
        #expect(sheet.width(ofColumn: 0) == 184) // an un-resized column follows the font
        #expect(sheet.height(ofRow: 0) == 48)
    }

    @Test func customSizesIgnoreFont() {
        let sheet = makeSheet()
        sheet.previewColumnResize(120, forColumn: 0)
        sheet.endColumnResize() // a deliberate, explicit width
        sheet.gridFontSize = 48
        #expect(sheet.width(ofColumn: 0) == 120) // explicit width is left alone
        #expect(sheet.width(ofColumn: 1) > 120)  // a default column still scales up
    }

    @Test func scaledDefaultsClampToTheResizeRange() {
        let sheet = makeSheet()
        sheet.gridFontSize = 100 // 92*100/14 ≈ 657 and 24*100/14 ≈ 171 — both clamp
        #expect(sheet.defaultColumnWidthScaled == SheetModel.columnWidthRange.upperBound)
        #expect(sheet.defaultRowHeightScaled == SheetModel.rowHeightRange.upperBound)
    }

    // MARK: Dialect-switch log marker

    @Test func modeMarkerIsDisplayOnly() {
        let log = LogStore(persists: false)
        log.append(HistoryEntry(expression: "", outcome: .mode("Programmer mode")))
        let record = log.logRecord(at: 0)
        #expect(record?.text == "Programmer mode")
        #expect(record?.isInfo == true) // display-only, like man output
        #expect(record?.value == nil)   // never recallable as a value
        #expect(record?.isError == false)
    }

    // MARK: Clean number display (type-preserving recall)

    @Test func recallOverrideKeepsTheTypedForm() {
        // A fixed-width int shows its plain number but recalls the typed
        // constructor — the canonical form rides along as the override.
        let typed = HistoryEntry(expression: "Int32(343353)",
                                 outcome: .value("343353"),
                                 recallOverride: "Int32(343353)")
        #expect(typed.recallValue == "Int32(343353)")

        // A plain number has no override — recall is the displayed text itself.
        let plain = HistoryEntry(expression: "2 + 2", outcome: .value("4"))
        #expect(plain.recallValue == "4")
    }
}
