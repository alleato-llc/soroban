import Testing
import PickleKit

/// Runs every scenario in Tests/SorobanEngineTests/Features/*.feature —
/// the user-perspective specification of the calculator (see the .feature
/// files; steps live in SorobanSteps.swift). Serialized: scenarios share
/// the step world via the documented PickleKit static-state pattern.
@Suite("Gherkin scenarios", .serialized)
struct GherkinTests {
    static let scenarios = GherkinTestScenario.scenarios(
        bundle: .module, subdirectory: "Features"
    )

    @Test(arguments: GherkinTests.scenarios)
    func scenario(_ test: GherkinTestScenario) async throws {
        let result = try await test.run(stepDefinitions: [SorobanSteps.self])
        #expect(result.passed)
    }
}
