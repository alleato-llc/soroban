import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Cell formats")
struct CellFormatTests {
    private func big(_ s: String) -> BigDecimal { BigDecimal(string: s)! }

    @Test func numberGroupsAndFixesDecimals() {
        let format = NumberFormat.number(decimals: 2)
        #expect(format.rendered(big("1234567.5")) == "1,234,567.50")
        #expect(format.rendered(big("0.125")) == "0.12")      // banker's: half to even
        #expect(format.rendered(big("0.135")) == "0.14")
        #expect(format.rendered(big("-42")) == "-42.00")
        #expect(format.rendered(.zero) == "0.00")
        #expect(NumberFormat.number(decimals: 0).rendered(big("1234.6")) == "1,235")
    }

    @Test func bigValuesStayExact() {
        // Far beyond Double's 15 digits — grouping must not lose a digit.
        let format = NumberFormat.number(decimals: 0)
        #expect(format.rendered(big("12345678901234567890123456789"))
                == "12,345,678,901,234,567,890,123,456,789")
    }

    @Test func currencyPutsSignBeforeSymbol() {
        let usd = NumberFormat.currency(symbol: "$", decimals: 2)
        #expect(usd.rendered(big("1234.5")) == "$1,234.50")
        #expect(usd.rendered(big("-1234.5")) == "-$1,234.50")
        let eur = NumberFormat.currency(symbol: "€", decimals: 2)
        #expect(eur.rendered(big("-2")) == "-€2.00")
    }

    @Test func percentIsAnExactShift() {
        let format = NumberFormat.percent(decimals: 2)
        #expect(format.rendered(big("0.0825")) == "8.25%")
        #expect(format.rendered(big("1")) == "100.00%")
        #expect(format.rendered(big("-0.005")) == "-0.50%")
        // The classic Double trap: 0.1 + 0.2; here it's exact.
        #expect(NumberFormat.percent(decimals: 0).rendered(big("0.3")) == "30%")
    }

    @Test func dateRendersSerials() throws {
        let serial = CivilDate.serial(year: 2026, month: 6, day: 6)
        #expect(NumberFormat.date.rendered(BigDecimal(serial)) == "2026-06-06")
        #expect(NumberFormat.date.rendered(BigDecimal(0)) == "1970-01-01")
        #expect(NumberFormat.date.rendered(BigDecimal(-1)) == "1969-12-31")
        // Non-integer serials round first.
        #expect(NumberFormat.date.rendered(big("0.4")) == "1970-01-01")
    }

    @Test func decimalStepping() {
        #expect(NumberFormat.general.adjustingDecimals(by: 1) == .number(decimals: 3))
        #expect(NumberFormat.number(decimals: 0).adjustingDecimals(by: -1) == .number(decimals: 0))
        #expect(NumberFormat.percent(decimals: 12).adjustingDecimals(by: 1) == .percent(decimals: 12))
        #expect(NumberFormat.currency(symbol: "£", decimals: 2).adjustingDecimals(by: 1)
                == .currency(symbol: "£", decimals: 3))
        #expect(NumberFormat.date.adjustingDecimals(by: 1) == .date)
    }

    @Test func defaultDetectionAndPruning() {
        var format = CellFormat()
        #expect(format.isDefault)
        format.bold = true
        #expect(!format.isDefault)
        format.bold = false
        #expect(format.isDefault)
    }

    @Test func codableIsCompactAndRoundTrips() throws {
        var format = CellFormat()
        format.bold = true
        format.numberFormat = .currency(symbol: "€", decimals: 3)
        format.fillColor = .yellow
        format.alignment = .center

        let data = try JSONEncoder().encode(format)
        let decoded = try JSONDecoder().decode(CellFormat.self, from: data)
        #expect(decoded == format)

        // Compactness: untouched fields don't appear.
        let json = String(data: data, encoding: .utf8)!
        #expect(!json.contains("italic"))
        #expect(!json.contains("underline"))
        #expect(!json.contains("textColor"))

        // A style-only format writes no number keys at all.
        var plain = CellFormat()
        plain.italic = true
        let plainJSON = String(data: try JSONEncoder().encode(plain), encoding: .utf8)!
        #expect(!plainJSON.contains("style"))
        #expect(!plainJSON.contains("decimals"))

        // Unknown future styles degrade to general, not an error.
        let future = try JSONDecoder().decode(
            CellFormat.self, from: Data(#"{"style":"fraction","bold":true}"#.utf8))
        #expect(future.numberFormat == .general)
        #expect(future.bold)
    }

    @Test func workbookCarriesFormats() throws {
        var format = CellFormat()
        format.underline = true
        format.numberFormat = .percent(decimals: 1)

        let workbook = Workbook(
            sheets: [Workbook.SheetPayload(name: "Sheet 1",
                                           cells: ["A:1": "0.0825"],
                                           formats: ["A:1": format])],
            variables: [:])
        let decoded = try Workbook.decode(try workbook.encode())
        #expect(decoded.sheets[0].formats["A:1"] == format)

        // Files without formats decode to empty (older-file policy).
        let legacy = try Workbook.decode(Data("""
        {"format": "soroban-workbook", "version": 1,
         "sheets": [{"name": "Sheet 1", "cells": {"A:1": "1"}}],
         "variables": {}}
        """.utf8))
        #expect(legacy.sheets[0].formats.isEmpty)
    }
}
