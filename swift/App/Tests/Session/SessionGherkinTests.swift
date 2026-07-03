import Foundation
import Testing
import PickleKit

/// Locates this test bundle (Bundle.module is SPM-only; this is an
/// Xcode-built bundle).
private final class SessionBundleLocator {}

@Suite("Session Gherkin scenarios", .serialized)
struct SessionGherkinTests {
    static let scenarios = GherkinTestScenario.scenarios(
        bundle: Bundle(for: SessionBundleLocator.self), subdirectory: "Features"
    )

    @Test(arguments: SessionGherkinTests.scenarios)
    func scenario(_ test: GherkinTestScenario) async throws {
        let result = try await test.run(stepDefinitions: [SessionSteps.self])
        #expect(result.passed)
    }
}
