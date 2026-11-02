// swift-tools-version: 6.0
import PackageDescription

// Swift wrapper around the Rust core. Run scripts/build-apple.sh first: it produces
// NotFFI.xcframework and Sources/NotCore/Generated.swift.
let package = Package(
    name: "NotCore",
    platforms: [.macOS(.v15)],
    products: [.library(name: "NotCore", targets: ["NotCore"])],
    targets: [
        .binaryTarget(name: "NotFFI", path: "NotFFI.xcframework"),
        .target(
            name: "NotCore",
            dependencies: ["NotFFI"],
            linkerSettings: [
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
    ],
    swiftLanguageModes: [.v5]
)
