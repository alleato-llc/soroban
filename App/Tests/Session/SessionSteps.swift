import Foundation
import PickleKit
import SorobanEngine

/// One session world per scenario: a Calculator + SheetModel, the exact
/// pair the app builds. autosaveToScratch is switched off FIRST so these
/// tests never touch the real Application Support scratch file.
struct SessionSteps: StepDefinitions {
    nonisolated(unsafe) static var calculator = Calculator()
    nonisolated(unsafe) static var sheet: SheetModel!

    /// init() runs OFF the main actor (reflection discovery), so it only
    /// flags the reset; the first @MainActor step handler builds the world.
    nonisolated(unsafe) static var needsReset = true

    init() {
        Self.needsReset = true
    }

    /// A real log model, wired so `History` reflection resolves in this world.
    nonisolated(unsafe) static var logStore: LogStore!

    @MainActor private static var session: SheetModel {
        if needsReset {
            let calculator = Calculator()
            Self.calculator = calculator
            let sheet = SheetModel(calculator: calculator)
            sheet.autosaveToScratch = false // never touch the real scratch file
            sheet.apply(Workbook(cells: [:], variables: [:]))
            let log = LogStore(persists: false) // never touch the real log.json
            sheet.store.logSource = log // wire `History` to this tape
            Self.logStore = log
            Self.sheet = sheet
            needsReset = false
        }
        return sheet
    }

    /// Mirrors `CalculatorSession.submit`'s outcome → entry mapping, so a logged
    /// line records the same `HistoryEntry` the app would.
    @MainActor private static func logEntry(for line: String) -> HistoryEntry {
        let outcome: HistoryEntry.Outcome
        switch calculator.evaluate(line) {
        case .success(.comment(let text)):
            outcome = .comment(text)
        case .success(.documentation(let doc)):
            outcome = .info(EvalOutcome.documentation(doc).description)
        case .success(let result):
            // Mirror submit: pretty JSON and host-handle results are display-only.
            if let block = result.rawBlock {
                outcome = .info(block)
            } else if case .value(let value) = result, value.containsHost {
                outcome = .info(value.description)
            } else {
                outcome = .value(result.description)
            }
        case .failure(let error):
            outcome = .error(message: "\(error)", position: error.position)
        }
        return HistoryEntry(expression: line, outcome: outcome,
                            note: Calculator.trailingComment(in: line))
    }

    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    @MainActor private static func address(_ key: String) throws -> CellAddress {
        guard let address = CellAddress(key: key.uppercased()) else {
            throw Failure(description: "'\(key)' is not a cell address")
        }
        return address
    }

    @MainActor private static func shown(at key: String) throws -> String {
        switch Self.session.display(at: try address(key)) {
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

    // MARK: Editing

    let cellContains = StepDefinition.given("cell ([A-Za-z]+:[0-9]+) contains \"(.*)\"") { match in
        Self.session.commit(match.captures[1], at: try Self.address(match.captures[0]))
    }

    // A log line, evaluated through the same Calculator the app uses — so
    // workbook-mutation commands (updateCell, addWorksheet, …) run through the
    // session's undoable override. Inner quotes are bare, like the engine suite.
    let runInLog = StepDefinition.when("I run \"(.*)\" in the log") { match in
        _ = Self.session // build the world (installs the mutation override)
        if case .failure(let error) = Self.calculator.evaluate(match.captures[0]) {
            throw Failure(description: "log command failed: \(error)")
        }
    }

    // History reflection: build the tape with these, then query it via the
    // ordinary calculator (`History` resolves on the log path).
    let logHasRun = StepDefinition.given("the log has run \"(.*)\"") { match in
        _ = Self.session // build the world (creates + wires the LogStore)
        Self.logStore.append(Self.logEntry(for: match.captures[0]))
    }

    let evaluatingGives = StepDefinition.then("evaluating \"(.*)\" gives \"(.*)\"") { match in
        _ = Self.session
        switch Self.calculator.evaluate(match.captures[0]) {
        case .success(.value(let value)):
            // Strings compare raw (unquoted); numbers via their canonical form.
            let got: String
            if case .string(let text) = value { got = text } else { got = value.description }
            guard got == match.captures[1] else {
                throw Failure(description:
                    "evaluating \(match.captures[0]) gave \(got), expected \(match.captures[1])")
            }
        case .success(let other):
            throw Failure(description: "expected a value, got \(other)")
        case .failure(let error):
            throw Failure(description: "evaluation failed: \(error)")
        }
    }

    let undo = StepDefinition.when("I undo") { _ in
        Self.session.undo()
    }

    let redo = StepDefinition.when("I redo") { _ in
        Self.session.redo()
    }

    // MARK: Worksheets

    let sheetAdded = StepDefinition.given("a new sheet named \"(.*)\" is added") { match in
        if let message = Self.session.addSheet() {
            throw Failure(description: message)
        }
        if let message = Self.session.renameActiveSheet(to: match.captures[0]) {
            throw Failure(description: message)
        }
    }

    let sheetActivated = StepDefinition.when("sheet \"(.*)\" is activated") { match in
        let names = Self.session.sheetNames
        guard let index = names.firstIndex(where: {
            $0.compare(match.captures[0], options: .caseInsensitive) == .orderedSame
        }) else {
            throw Failure(description: "no sheet named '\(match.captures[0])' in \(names)")
        }
        Self.session.activateSheet(at: index)
    }

    let sheetRenamed = StepDefinition.when("the active sheet is renamed to \"(.*)\"") { match in
        if let message = Self.session.renameActiveSheet(to: match.captures[0]) {
            throw Failure(description: message)
        }
    }

    let activeSheetNamed = StepDefinition.then("the active sheet is named \"(.*)\"") { match in
        let name = Self.session.activeSheetName
        guard name.compare(match.captures[0], options: .caseInsensitive) == .orderedSame else {
            throw Failure(description: "active sheet is '\(name)', expected '\(match.captures[0])'")
        }
    }

    // MARK: Names

    let cellNamed = StepDefinition.given("cell ([A-Za-z]+:[0-9]+) is named \"(.*)\"") { match in
        if let message = Self.session.nameCell(match.captures[1],
                                             at: try Self.address(match.captures[0])) {
            throw Failure(description: message)
        }
    }

    let cellRenamed = StepDefinition.when("cell ([A-Za-z]+:[0-9]+) is renamed to \"(.*)\"") { match in
        if let message = Self.session.nameCell(match.captures[1],
                                             at: try Self.address(match.captures[0])) {
            throw Failure(description: message)
        }
    }

    let nameRemovedInline = StepDefinition.when(
        "the name of cell ([A-Za-z]+:[0-9]+) is removed, replacing references with its address") { match in
        Self.session.removeCellName(at: try Self.address(match.captures[0]), mode: .inlineAddresses)
    }

    // MARK: Selection, fill, paste

    /// Stored by "the cells … are copied" — the in-memory equivalent of the
    /// pasteboard's custom type (the NSPasteboard plumbing itself is thin
    /// and verified manually; these pin the ADJUSTMENT semantics).
    nonisolated(unsafe) static var copied: (tsv: String, anchor: CellAddress)?

    let cellSelected = StepDefinition.when("cell ([A-Za-z]+:[0-9]+) is selected") { match in
        Self.session.select(try Self.address(match.captures[0]))
    }

    let rangeSelected = StepDefinition.given(
        "cells ([A-Za-z]+:[0-9]+) through ([A-Za-z]+:[0-9]+) are selected") { match in
        Self.session.select(try Self.address(match.captures[0]))
        Self.session.selectionExtent = try Self.address(match.captures[1])
    }

    let fillDown = StepDefinition.when("I fill down") { _ in
        Self.session.fillDown()
    }

    let fillRight = StepDefinition.when("I fill right") { _ in
        Self.session.fillRight()
    }

    let selectionCopied = StepDefinition.when("the selection is copied") { _ in
        guard let rect = Self.session.selectionRect,
              let tsv = Self.session.selectionTSV() else {
            throw Failure(description: "nothing selected to copy")
        }
        Self.copied = (tsv, CellAddress(column: rect.columns.lowerBound,
                                        row: rect.rows.lowerBound))
    }

    let copiedPasted = StepDefinition.when("the copied cells are pasted") { _ in
        guard let copied = Self.copied else {
            throw Failure(description: "nothing was copied")
        }
        Self.session.pasteCopiedCells(tsv: copied.tsv, copiedFrom: copied.anchor)
    }

    let externalPasted = StepDefinition.when("the text \"(.*)\" is pasted from outside") { match in
        Self.session.paste(tsv: match.captures[0])
    }

    // MARK: Structural edits

    let rowsInserted = StepDefinition.when("([0-9]+) rows? (?:is|are) inserted above row ([0-9]+)") { match in
        guard let count = Int(match.captures[0]), let row = Int(match.captures[1]) else {
            throw Failure(description: "bad counts")
        }
        if let message = Self.session.insertRows(at: row - 1, count: count) {
            throw Failure(description: message)
        }
    }

    let rowDeleted = StepDefinition.when("row ([0-9]+) is deleted") { match in
        guard let row = Int(match.captures[0]) else { throw Failure(description: "bad row") }
        if let message = Self.session.deleteRows(at: row - 1, count: 1) {
            throw Failure(description: message)
        }
    }

    let columnDeleted = StepDefinition.when("column ([A-Z]) is deleted") { match in
        guard let column = CellAddress.columnIndex(forName: match.captures[0]) else {
            throw Failure(description: "bad column")
        }
        if let message = Self.session.deleteColumns(at: column, count: 1) {
            throw Failure(description: message)
        }
    }

    // MARK: Controls

    let controlCommits = StepDefinition.when(
        "the control in cell ([A-Za-z]+:[0-9]+) commits \"(.*)\"") { match in
        Self.session.commitControl(at: try Self.address(match.captures[0]),
                                 literal: match.captures[1])
    }

    let sliderReleased = StepDefinition.when(
        "the slider in cell ([A-Za-z]+:[0-9]+) is released at the top") { match in
        let address = try Self.address(match.captures[0])
        guard case .slider(let info) = Self.session.display(at: address) else {
            throw Failure(description: "cell \(match.captures[0]) isn't a slider")
        }
        Self.session.commitSlider(at: address, info: info, fraction: 1.0)
    }

    // MARK: Assertions

    let contentsAre = StepDefinition.then("the contents of cell ([A-Za-z]+:[0-9]+) are \"(.*)\"") { match in
        let raw = Self.session.raw(at: try Self.address(match.captures[0]))
        guard raw == match.captures[1] else {
            throw Failure(description: "cell \(match.captures[0]) holds '\(raw)', expected '\(match.captures[1])'")
        }
    }

    let cellShows = StepDefinition.then("cell ([A-Za-z]+:[0-9]+) shows \"(.*)\"") { match in
        let shown = try Self.shown(at: match.captures[0])
        guard shown == match.captures[1] else {
            throw Failure(description: "cell \(match.captures[0]) shows '\(shown)', expected '\(match.captures[1])'")
        }
    }

    let csvContains = StepDefinition.then("the CSV export contains the line \"(.*)\"") { match in
        let csv = Self.session.activeSheetCSV()
        guard csv.components(separatedBy: "\n").contains(match.captures[0]) else {
            throw Failure(description: "CSV export doesn't contain '\(match.captures[0])':\n\(csv)")
        }
    }
}
