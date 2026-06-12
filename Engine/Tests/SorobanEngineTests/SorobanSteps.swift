import Foundation
import PickleKit
@testable import Anzan
@testable import SorobanEngine

/// One world per scenario: a calculator wired to a fresh SheetStore —
/// exactly the topology the app builds. Statics because PickleKit
/// discovers `StepDefinition` properties by reflection and resets via
/// `init()` (the README pattern); scenarios run serialized.
struct SorobanSteps: StepDefinitions {
    nonisolated(unsafe) static var calculator = Calculator()
    nonisolated(unsafe) static var store = SheetStore(calculator: Calculator())
    nonisolated(unsafe) static var outcome: Result<EvalOutcome, EngineError>?

    init() {
        let calculator = Calculator()
        Self.calculator = calculator
        Self.store = SheetStore(calculator: calculator)
        Self.outcome = nil
    }

    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    private static func address(_ key: String) throws -> CellAddress {
        guard let address = CellAddress(key: key.uppercased()) else {
            throw Failure(description: "'\(key)' is not a cell address")
        }
        return address
    }

    /// What a user "sees" in a cell, as a comparable string.
    private static func shown(at key: String) throws -> String {
        switch Self.store.activeSheet.grid.displayValue(at: try address(key)) {
        case .empty: return ""
        case .text(let text): return text
        case .value(let value): return value.description
        case .error(let message): return "#ERR \(message)"
        case .definition(let glyph): return glyph
        case .note(let comment): return "# \(comment)"
        case .slider(let info), .stepper(let info): return "slider:\(info.value)"
        case .checkbox(let info): return info.isOn ? "checked" : "unchecked"
        case .dropdown(let info): return info.value.displayText
        }
    }

    // MARK: Log

    let calculate = StepDefinition.when("I calculate \"(.*)\"") { match in
        Self.outcome = Self.calculator.evaluate(match.captures[0])
    }

    let resultIs = StepDefinition.then("the result is \"(.*)\"") { match in
        guard case .success(let outcome)? = Self.outcome else {
            throw Failure(description: "expected a result, got \(String(describing: Self.outcome))")
        }
        guard outcome.description == match.captures[0] else {
            throw Failure(description: "expected \(match.captures[0]), got \(outcome.description)")
        }
    }

    let resultNearTarget = StepDefinition.then("the result is within \"(.*)\" of \"(.*)\"") { match in
        guard case .success(let outcome)? = Self.outcome,
              let value = outcome.numericValue,
              let bound = BigDecimal(string: match.captures[0]),
              let target = BigDecimal(string: match.captures[1]) else {
            throw Failure(description: "expected a numeric result, bound, and target")
        }
        let diff = value - target
        let magnitude = diff.isNegative ? -diff : diff
        guard magnitude <= bound else {
            throw Failure(description: "\(value) is not within \(match.captures[0]) of \(match.captures[1])")
        }
    }

    let resultNearZero = StepDefinition.then("the result is within \"(.*)\" of zero") { match in
        guard case .success(let outcome)? = Self.outcome,
              let value = outcome.numericValue,
              let bound = BigDecimal(string: match.captures[0]) else {
            throw Failure(description: "expected a numeric result and bound")
        }
        let magnitude = value.isNegative ? -value : value
        guard magnitude < bound else {
            throw Failure(description: "|\(value)| is not within \(match.captures[0]) of zero")
        }
    }

    let calculationFails = StepDefinition.then("the calculation fails mentioning \"(.*)\"") { match in
        guard case .failure(let error)? = Self.outcome else {
            throw Failure(description: "expected a failure, got \(String(describing: Self.outcome))")
        }
        guard "\(error)".contains(match.captures[0]) else {
            throw Failure(description: "error '\(error)' doesn't mention '\(match.captures[0])'")
        }
    }

    let documentationShown = StepDefinition.then("documentation is shown mentioning \"(.*)\"") { match in
        guard case .success(.documentation(let doc))? = Self.outcome else {
            throw Failure(description: "expected documentation, got \(String(describing: Self.outcome))")
        }
        let text = "\(doc.signature) \(doc.summary) \(doc.examples.joined(separator: " "))"
        guard text.contains(match.captures[0]) else {
            throw Failure(description: "documentation doesn't mention '\(match.captures[0])': \(text)")
        }
    }

    // MARK: Grid

    let cellContains = StepDefinition.given("cell ([A-Za-z]+:[0-9]+) contains \"(.*)\"") { match in
        Self.store.activeSheet.grid.setCell(match.captures[1], at: try Self.address(match.captures[0]))
    }

    let sheetContains = StepDefinition.given("the sheet contains:") { match in
        guard let table = match.dataTable else {
            throw Failure(description: "this step needs a | cell | value | table")
        }
        for row in table.asDictionaries {
            guard let cell = row["cell"], let value = row["value"] else {
                throw Failure(description: "table needs 'cell' and 'value' columns")
            }
            Self.store.activeSheet.grid.setCell(value, at: try Self.address(cell))
        }
    }

    let cellNamed = StepDefinition.given("cell ([A-Za-z]+:[0-9]+) is named \"(.*)\"") { match in
        try Self.store.activeSheet.grid.setCellName(match.captures[1],
                                                    at: try Self.address(match.captures[0]))
    }

    let cellShows = StepDefinition.then("cell ([A-Za-z]+:[0-9]+) shows \"(.*)\"") { match in
        let shown = try Self.shown(at: match.captures[0])
        guard shown == match.captures[1] else {
            throw Failure(description: "cell \(match.captures[0]) shows '\(shown)', expected '\(match.captures[1])'")
        }
    }

    let cellIsSlider = StepDefinition.then("cell ([A-Za-z]+:[0-9]+) is a slider set to \"(.*)\"") { match in
        let shown = try Self.shown(at: match.captures[0])
        guard shown == "slider:\(match.captures[1])" else {
            throw Failure(description: "cell \(match.captures[0]) is '\(shown)', expected a slider at \(match.captures[1])")
        }
    }

    let cellShowsError = StepDefinition.then("cell ([A-Za-z]+:[0-9]+) shows an error mentioning \"(.*)\"") { match in
        let shown = try Self.shown(at: match.captures[0])
        guard shown.hasPrefix("#ERR"), shown.contains(match.captures[1]) else {
            throw Failure(description: "cell \(match.captures[0]) shows '\(shown)', expected an error mentioning '\(match.captures[1])'")
        }
    }

    // MARK: Worksheets

    let sheetNamed = StepDefinition.given("a sheet named \"(.*)\"") { match in
        try Self.store.addSheet()
        try Self.store.rename(at: Self.store.sheets.count - 1, to: match.captures[0])
    }

    let cellOnSheetContains = StepDefinition.given(
        "cell ([A-Za-z]+:[0-9]+) on \"(.*)\" contains \"(.*)\"") { match in
        guard let sheet = Self.store.sheet(named: match.captures[1]) else {
            throw Failure(description: "no sheet named '\(match.captures[1])'")
        }
        sheet.grid.setCell(match.captures[2], at: try Self.address(match.captures[0]))
    }

    // MARK: Formatting (display-only; rendering is engine logic)

    let cellFormatted = StepDefinition.given(
        "cell ([A-Za-z]+:[0-9]+) is formatted as \"(.*)\"") { match in
        var format = Self.store.activeSheet.formats[try Self.address(match.captures[0])]
            ?? CellFormat()
        switch match.captures[1] {
        case "number": format.numberFormat = .number(decimals: 2)
        case "dollars": format.numberFormat = .currency(symbol: "$", decimals: 2)
        case "euros": format.numberFormat = .currency(symbol: "€", decimals: 2)
        case "percent": format.numberFormat = .percent(decimals: 2)
        case "a date": format.numberFormat = .date
        case "hex": format.numberFormat = .hex
        case "binary": format.numberFormat = .binary
        default:
            throw Failure(description: "unknown format '\(match.captures[1])'")
        }
        Self.store.activeSheet.formats[try Self.address(match.captures[0])] = format
    }

    let cellDisplays = StepDefinition.then(
        "cell ([A-Za-z]+:[0-9]+) displays \"(.*)\"") { match in
        let address = try Self.address(match.captures[0])
        guard case .value(let value) = Self.store.activeSheet.grid.displayValue(at: address) else {
            throw Failure(description: "cell \(match.captures[0]) doesn't hold a value")
        }
        let format = Self.store.activeSheet.formats[address] ?? CellFormat()
        let displayed = format.numberFormat.rendered(value)
        guard displayed == match.captures[1] else {
            throw Failure(description: "cell \(match.captures[0]) displays '\(displayed)', expected '\(match.captures[1])'")
        }
    }

    // MARK: Persistence

    let savedAndReopened = StepDefinition.when("the workbook is saved and reopened") { match in
        // Engine-level round trip: raws + names through the codec, into a
        // FRESH store on the same calculator (which rewires the resolvers).
        let payloads = Self.store.sheets.map { sheet in
            Workbook.SheetPayload(
                name: sheet.name,
                cells: Dictionary(uniqueKeysWithValues: sheet.grid.raws.map { ("\($0.key)", $0.value) }),
                names: Dictionary(uniqueKeysWithValues: sheet.grid.cellNames.map { ("\($0.key)", $0.value) }))
        }
        let decoded = try Workbook.decode(
            try Workbook(sheets: payloads, variables: Self.calculator.environment.userVariables,
                         functions: Self.calculator.environment.allUserFunctions,
                         dataTypes: Self.calculator.environment.userDataTypes).encode())

        let store = SheetStore(calculator: Self.calculator)
        Self.store = store
        // Types → functions → variables, exactly like the app on open
        // (record variables are constructor calls; they need their types).
        Self.calculator.restoreSession(from: decoded)
        var sheets: [Sheet] = []
        for payload in decoded.sheets {
            let sheet = store.makeSheet(name: payload.name)
            var contents: [CellAddress: String] = [:]
            var names: [CellAddress: String] = [:]
            for (key, raw) in payload.cells {
                if let address = CellAddress(key: key) { contents[address] = raw }
            }
            for (key, name) in payload.names {
                if let address = CellAddress(key: key) { names[address] = name }
            }
            sheet.grid.load(contents)
            sheet.grid.loadCellNames(names)
            sheets.append(sheet)
        }
        store.replaceSheets(sheets, activeName: decoded.sheets.first?.name)
    }
}
