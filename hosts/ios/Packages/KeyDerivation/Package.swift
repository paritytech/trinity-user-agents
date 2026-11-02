// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import Foundation
import PackageDescription

let coreIsInTree: Bool = {
    var root = URL(fileURLWithPath: #filePath)
    for _ in 0 ..< 5 {
        root.deleteLastPathComponent()
    }
    return FileManager.default.fileExists(atPath: root.appendingPathComponent("rust/crates/truapi").path)
}()

let coreDependency: Package.Dependency = coreIsInTree
    ? .package(name: "trinity-user-agents", path: "../../../..")
    : .package(url: "https://github.com/paritytech/trinity-user-agents", exact: "0.16.0")

let package = Package(
    name: "KeyDerivation",
    defaultLocalization: "en",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "KeyDerivation",
            targets: ["KeyDerivation"]
        )
    ],
    dependencies: [
        coreDependency,
        .package(
            url: "https://github.com/novasamatech/Keystore-iOS",
            from: "1.0.1"
        ),
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Crypto-iOS",
            from: "0.3.0"
        ),
        .package(
            url: "https://github.com/novasamatech/verifiable-swift",
            from: "0.4.0"
        ),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../FoundationExt")
    ],
    targets: [
        .target(
            name: "KeyDerivation",
            dependencies: [
                .product(name: "TrUAPIHost", package: "trinity-user-agents"),
                .product(name: "Keystore-iOS", package: "keystore-ios"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "NovaCrypto", package: "crypto-ios"),
                .product(name: "BandersnatchApi", package: "verifiable-swift"),
                "SubstrateSdkExt",
                "FoundationExt"
            ],
            path: "Sources"
        ),
        .testTarget(
            name: "KeyDerivationTests",
            dependencies: [
                "KeyDerivation",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "NovaCrypto", package: "crypto-ios")
            ],
            path: "Tests"
        )
    ]
)
