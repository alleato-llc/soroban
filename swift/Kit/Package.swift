// swift-tools-version: 6.0
import PackageDescription

// The reusable binary/bit-editor UI, shared between the Soroban calculator and
// the standalone Tama app (a sibling repo). Depends on the engine for the
// host-neutral logic (BinaryView, BinaryView.FormatBuilder); the host-specific
// surface is the BinaryEditorHost seam.
let package = Package(
    name: "BinaryEditorKit",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "BinaryEditorKit", targets: ["BinaryEditorKit"]),
    ],
    dependencies: [
        .package(path: "../Engine"),
        .package(url: "https://github.com/attaswift/BigInt.git", from: "5.3.0"),
    ],
    targets: [
        .target(
            name: "BinaryEditorKit",
            dependencies: [
                .product(name: "SorobanEngine", package: "Engine"),
                .product(name: "BigInt", package: "BigInt"),
            ]
        ),
        .testTarget(
            name: "BinaryEditorKitTests",
            dependencies: ["BinaryEditorKit"]
        ),
    ]
)
