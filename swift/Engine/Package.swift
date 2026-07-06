// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SorobanEngine",
    platforms: [
        // macOS 15 / iOS 18: the exact-decimal significand (Number/Integer.swift)
        // uses the stdlib UInt128/Int128 for its base-2⁶⁴ limb arithmetic, which
        // are available from these versions.
        .macOS(.v15),
        // The library (Anzan + SorobanEngine) is platform-agnostic and links
        // into the iPad app; the SorobanCLI/LineNoise executable stays macOS
        // (nothing on the iOS target depends on it, so it never builds there).
        .iOS(.v18),
    ],
    products: [
        .library(name: "SorobanEngine", targets: ["SorobanEngine"]),
        // The command-line calculator: `swift build -c release --product soroban`.
        .executable(name: "soroban", targets: ["SorobanCLI"]),
    ],
    dependencies: [
        .package(url: "https://github.com/attaswift/BigInt.git", from: "5.3.0"),
        // CLI-only (REPL line editing: history, arrows, tab completion).
        // The engine library target must stay BigInt-only. Pinned to a
        // revision — the repo's newest tag (0.0.3) predates Swift 5.
        .package(url: "https://github.com/andybest/linenoise-swift.git",
                 revision: "cbf0a35c6e159e4fe6a03f76c8a17ef08e907b0e"),
        // BDD/Gherkin scenarios (test-only). Pinned to a REVISION — upstream
        // has no tags yet, and tracking main let any pickle-kit push break
        // this repo's CI. Move to a version pin once pickle-kit tags one.
        .package(url: "https://github.com/alleato-llc/pickle-kit.git",
                 revision: "da664a59b56691b3a29d0e702b2e9df2ed2c5db9"),
    ],
    targets: [
        // The LANGUAGE — lexer, parser, evaluator, exact numbers, the
        // function library, and the Calculator facade. Knows nothing about
        // grids or files: hosts wire cells in through resolver closures.
        .target(
            name: "Anzan",
            dependencies: [
                .product(name: "BigInt", package: "BigInt")
            ]
        ),
        // The HOSTING layer — the spreadsheet model (Sheet/) and the
        // workbook codec (Persistence/). Re-exports Anzan, so depending on
        // SorobanEngine is enough for the app.
        .target(
            name: "SorobanEngine",
            dependencies: ["Anzan"]
        ),
        // The CLI is an Anzan REPL — it deliberately has no sheet layer.
        .executableTarget(
            name: "SorobanCLI",
            dependencies: [
                "Anzan",
                .product(name: "LineNoise", package: "linenoise-swift"),
            ]
        ),
        .testTarget(
            name: "SorobanEngineTests",
            dependencies: [
                "Anzan",
                "SorobanEngine",
                .product(name: "PickleKit", package: "pickle-kit"),
            ],
            resources: [.copy("Features")]
        ),
    ]
)
