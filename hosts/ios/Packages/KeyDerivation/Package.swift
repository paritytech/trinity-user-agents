// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

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
