import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Workbook journal")
struct WorkbookJournalTests {
    private func baseWorkbook() -> Workbook {
        Workbook(sheets: [
            .init(name: "Sheet 1", cells: ["A:1": "1"]),
            .init(name: "Budget", cells: [:]),
        ], variables: [:])
    }

    @Test func linesRoundTrip() throws {
        let entries = [
            WorkbookJournal.Entry(sheet: "Sheet 1", cell: "A:1", raw: "42"),
            WorkbookJournal.Entry(sheet: "Budget", cell: "B:2", raw: "sum(A:1..A:9) # note"),
            WorkbookJournal.Entry(sheet: "Sheet 1", cell: "A:1", raw: ""),
        ]
        var data = Data()
        for entry in entries {
            data.append(try #require(WorkbookJournal.encodeLine(entry)))
        }
        #expect(WorkbookJournal.decode(data) == entries)
    }

    @Test func replayAppliesInOrderLastWriterWins() {
        var workbook = baseWorkbook()
        WorkbookJournal.apply([
            .init(sheet: "Sheet 1", cell: "A:1", raw: "10"),
            .init(sheet: "Sheet 1", cell: "A:1", raw: "20"),   // supersedes
            .init(sheet: "budget", cell: "B:2", raw: "5"),     // case-insensitive sheet
            .init(sheet: "Sheet 1", cell: "A:2", raw: "kept"),
            .init(sheet: "Sheet 1", cell: "A:2", raw: ""),     // clears
        ], to: &workbook)

        #expect(workbook.sheets[0].cells["A:1"] == "20")
        #expect(workbook.sheets[0].cells["A:2"] == nil)
        #expect(workbook.sheets[1].cells["B:2"] == "5")
    }

    @Test func replayIsIdempotentOverAFreshSnapshot() {
        // The crash-consistency property: a snapshot already containing the
        // journal's effects + a replay of that same journal = same state.
        let entries: [WorkbookJournal.Entry] = [
            .init(sheet: "Sheet 1", cell: "A:1", raw: "10"),
            .init(sheet: "Sheet 1", cell: "A:1", raw: "20"),
        ]
        var snapshot = baseWorkbook()
        WorkbookJournal.apply(entries, to: &snapshot) // "compacted" state
        var replayed = snapshot
        WorkbookJournal.apply(entries, to: &replayed) // crash before truncate
        #expect(replayed == snapshot)
    }

    @Test func junkIsSkippedNotFatal() {
        // Torn final line (crash mid-append) + unknown sheet + bad cell key.
        var data = Data()
        data.append(WorkbookJournal.encodeLine(.init(sheet: "Sheet 1", cell: "A:1", raw: "7"))!)
        data.append(Data("{\"sheet\": \"Sheet 1\", \"cel".utf8)) // torn
        let decoded = WorkbookJournal.decode(data)
        #expect(decoded.count == 1)

        var workbook = baseWorkbook()
        WorkbookJournal.apply([
            .init(sheet: "Nope", cell: "A:1", raw: "9"),
            .init(sheet: "Sheet 1", cell: "ZZ:9", raw: "9"),
            .init(sheet: "Sheet 1", cell: "A:3", raw: "9"),
        ], to: &workbook)
        #expect(workbook.sheets[0].cells["A:3"] == "9")
        #expect(workbook.sheets[0].cells.count == 2) // A:1 original + A:3
    }
}
