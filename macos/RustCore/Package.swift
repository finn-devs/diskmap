// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "RustCore",
    products: [
        .library(name: "RustCore", targets: ["CRustCore"]),
    ],
    targets: [
        .target(
            name: "CRustCore",
            path: "Sources/CRustCore",
            publicHeadersPath: "include",
            linkerSettings: [
                .linkedLibrary("dm_ffi"),
                .unsafeFlags(["-L", "../../target/universal/release"]),
            ]
        ),
    ]
)
