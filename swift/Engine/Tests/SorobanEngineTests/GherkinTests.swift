import Foundation
import Testing
import PickleKit

/// Runs every scenario in the shared `spec/anzan/*.feature` suite — the
/// user-perspective specification both ecosystems must honor (steps live in
/// SorobanSteps.swift; see spec/README.md). Serialized: scenarios share the
/// step world via the documented PickleKit static-state pattern.
@Suite("Gherkin scenarios", .serialized)
struct GherkinTests {
    /// Resolved from THIS file's path, not from the test bundle. `Features` is a
    /// symlink to `spec/anzan`, and SwiftPM copies the *link* into the bundle,
    /// where its relative target no longer resolves — so the bundle loader found
    /// zero features and the whole suite passed vacuously. A vacuous pass is
    /// indistinguishable from a real one, which is exactly what makes it
    /// dangerous for a parity oracle; going through the source path means the
    /// scenario count can't silently drop to zero.
    static let scenarios: [GherkinTestScenario] = {
        let specDirectory = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // SorobanEngineTests/
            .deletingLastPathComponent()  // Tests/
            .deletingLastPathComponent()  // Engine/
            .deletingLastPathComponent()  // swift/
            .deletingLastPathComponent()  // the repo root
            .appendingPathComponent("spec/anzan")
        return GherkinTestScenario.scenarios(paths: [specDirectory.path])
    }()

    @Test(arguments: GherkinTests.scenarios)
    func scenario(_ test: GherkinTestScenario) async throws {
        let result = try await test.run(stepDefinitions: [SorobanSteps.self])
        #expect(result.passed)
    }
}
