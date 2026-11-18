// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Coinage",
    defaultLocalization: "en",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "Coinage",
            targets: ["Coinage"]
        )
    ],
    dependencies: [
        .package(name: "trinity-user-agents", path: "../../../.."),
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/attaswift/BigInt",
            from: "5.5.1"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(
            url: "https://github.com/apple/swift-async-algorithms",
            from: "1.0.4"
        ),
        .package(
            url: "https://github.com/paritytech/AsyncExtensions",
            exact: "0.5.6"
        ),
        .package(path: "../KeyDerivation")
    ],
    targets: [
        .target(
            name: "Coinage",
            dependencies: [
                .product(name: "TrUAPIHost", package: "trinity-user-agents"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "BigInt", package: "BigInt"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "AsyncAlgorithms", package: "swift-async-algorithms"),
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                "KeyDerivation"
            ]
        )
    ]
)
