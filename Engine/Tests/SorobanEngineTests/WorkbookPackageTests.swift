import Foundation
import Testing
@testable import Anzan
@testable import SorobanEngine

@Suite("Workbook package")
struct WorkbookPackageTests {
    private func tempURL(_ name: String) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("soroban-tests-\(UUID().uuidString)")
            .appendingPathComponent(name)
    }

    private var sample: Workbook {
        Workbook(cells: ["A:1": "42"], variables: ["rate": .number(BigDecimal(string: "0.1")!)])
    }

    @Test func packageRoundTrip() throws {
        let url = tempURL("model.soroban")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }

        try WorkbookPackage.write(sample, to: url)

        var isDirectory: ObjCBool = false
        #expect(FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory))
        #expect(isDirectory.boolValue) // it's a package, not a flat file
        #expect(WorkbookPackage.databaseURL(in: url) == nil) // no data sheets

        let read = try WorkbookPackage.read(from: url)
        #expect(read.sheets[0].cells["A:1"] == "42")

        // Overwriting an existing package is atomic-replace, not append.
        var updated = sample
        updated.sheets[0].cells["A:2"] = "7"
        try WorkbookPackage.write(updated, to: url)
        #expect(try WorkbookPackage.read(from: url).sheets[0].cells["A:2"] == "7")
    }

    @Test func legacyFlatFilesStillRead() throws {
        let url = tempURL("legacy.soroban")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }

        try sample.encode().write(to: url) // old-style single JSON file
        let read = try WorkbookPackage.read(from: url)
        #expect(read.sheets[0].cells["A:1"] == "42")

        // Saving over a flat file upgrades it to a package in place.
        try WorkbookPackage.write(read, to: url)
        var isDirectory: ObjCBool = false
        _ = FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
        #expect(isDirectory.boolValue)
        #expect(try WorkbookPackage.read(from: url).sheets[0].cells["A:1"] == "42")
    }

    @Test func packageCarriesTheDatabase() throws {
        let dir = tempURL("x").deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let fakeDB = dir.appendingPathComponent("working.sqlite")
        try Data("not really sqlite, just bytes".utf8).write(to: fakeDB)
        let url = dir.appendingPathComponent("with-data.soroban")

        try WorkbookPackage.write(sample, to: url, databaseURL: fakeDB)
        let inside = try #require(WorkbookPackage.databaseURL(in: url))
        #expect(try Data(contentsOf: inside) == Data(contentsOf: fakeDB))
    }

    @Test func emptyDirectoryIsNotAWorkbook() throws {
        let url = tempURL("hollow.soroban")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        #expect(throws: WorkbookPackage.PackageError.self) {
            try WorkbookPackage.read(from: url)
        }
    }
}
