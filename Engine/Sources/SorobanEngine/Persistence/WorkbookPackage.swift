import Anzan
import Foundation

/// `.soroban` on disk: a document package (directory Finder shows as one
/// file) holding the diffable JSON manifest and, when the workbook has data
/// sheets, their SQLite store:
///
///     MyModel.soroban/
///     ├── workbook.json   ← the authored model (same format as ever)
///     └── data.sqlite     ← present only when data sheets exist
///
/// Legacy flat `.soroban` JSON files read transparently (a "package with no
/// database"); saves always write the package shape.
public enum WorkbookPackage {
    public static let manifestName = "workbook.json"
    public static let databaseName = "data.sqlite"

    public enum PackageError: Error, CustomStringConvertible {
        case missingManifest
        public var description: String { "the package has no \(manifestName)" }
    }

    public static func read(from url: URL) throws -> Workbook {
        var isDirectory: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
        if exists, isDirectory.boolValue {
            let manifest = url.appendingPathComponent(manifestName)
            guard FileManager.default.fileExists(atPath: manifest.path) else {
                throw PackageError.missingManifest
            }
            return try Workbook.decode(try Data(contentsOf: manifest))
        }
        // Legacy single-file workbook.
        return try Workbook.decode(try Data(contentsOf: url))
    }

    /// The package's database, when it has one.
    public static func databaseURL(in url: URL) -> URL? {
        let candidate = url.appendingPathComponent(databaseName)
        return FileManager.default.fileExists(atPath: candidate.path) ? candidate : nil
    }

    /// Atomic write: builds the package in a sibling temp directory, then
    /// swaps it in — replacing a legacy flat file with a package works too.
    /// `databaseURL` (the live working store) is copied in when given.
    public static func write(_ workbook: Workbook, to url: URL,
                             databaseURL: URL? = nil) throws {
        let fm = FileManager.default
        let temp = url.deletingLastPathComponent()
            .appendingPathComponent(".\(url.lastPathComponent).saving-\(UUID().uuidString)")
        try fm.createDirectory(at: temp, withIntermediateDirectories: true)
        var keepTemp = false
        defer { if !keepTemp { try? fm.removeItem(at: temp) } }

        try workbook.encode().write(to: temp.appendingPathComponent(manifestName))
        if let databaseURL, fm.fileExists(atPath: databaseURL.path) {
            try fm.copyItem(at: databaseURL, to: temp.appendingPathComponent(databaseName))
        }

        if fm.fileExists(atPath: url.path) {
            _ = try fm.replaceItemAt(url, withItemAt: temp)
        } else {
            try fm.moveItem(at: temp, to: url)
            keepTemp = true // moved, nothing to clean
        }
    }
}
