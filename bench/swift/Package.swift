// swift-tools-version: 6.0
//
// Cross-engine benchmark runner (Swift engine). A standalone SwiftPM executable
// DELIBERATELY OUTSIDE the Engine package (its own .build/) so it never disturbs the
// engine's build or CI. It depends on the SorobanEngine library, which re-exports the
// Anzan language (`@_exported import Anzan`), so `Calculator.evaluate` — the fair
// symmetric op the Rust runner also measures — is in scope.
import PackageDescription

let package = Package(
    name: "soroban-bench-swift",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(path: "../../swift/Engine"),
    ],
    targets: [
        .executableTarget(
            name: "runner",
            dependencies: [
                .product(name: "SorobanEngine", package: "Engine"),
            ]
        ),
    ]
)
