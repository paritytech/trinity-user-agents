// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "StructuredConcurrency",
    defaultLocalization: "en",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "StructuredConcurrency",
            targets: ["StructuredConcurrency"]
        ),
        .library(
            name: "StructuredConcurrencyTestSupport",
            targets: ["StructuredConcurrencyTestSupport"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/paritytech/AsyncExtensions",
            exact: "0.5.6"
        ),
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Operation-iOS",
            from: "3.0.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Foundation-iOS",
            from: "1.3.0"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(path: "../OperationExt"),
    ],
    targets: [
        .target(
            name: "StructuredConcurrency",
            dependencies: [
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageSubscription", package: "substrate-sdk-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "Foundation-iOS", package: "foundation-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
                "OperationExt",
            ],
            path: "Sources"
        ),
        .target(
            name: "StructuredConcurrencyTestSupport",
            path: "TestSupport"
        ),
        .testTarget(
            name: "StructuredConcurrencyTests",
            dependencies: [
                "StructuredConcurrency",
                .product(name: "Operation-iOS", package: "operation-ios"),
            ],
            path: "Tests"
        )
    ]
)
