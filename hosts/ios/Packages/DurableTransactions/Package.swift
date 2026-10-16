// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "DurableTransactions",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "DurableTransactions",
            targets: ["DurableTransactions"]
        ),
        .library(
            name: "DurableTransactionsTestSupport",
            targets: ["DurableTransactionsTestSupport"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/extrinsic-service-ios",
            from: "1.11.0"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(
            url: "https://github.com/novasamatech/Operation-iOS",
            from: "3.0.0"
        ),
        .package(
            url: "https://github.com/apple/swift-async-algorithms",
            from: "1.0.4"
        ),
        .package(
            url: "https://github.com/paritytech/AsyncExtensions",
            exact: "0.5.6"
        ),
        .package(path: "../StructuredConcurrency"),
        .package(path: "../SubstrateOperation"),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../ExtrinsicServiceExt"),
        .package(path: "../BackgroundExecution"),
        .package(path: "../ChainStore")
    ],
    targets: [
        .target(
            name: "DurableTransactions",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "AsyncAlgorithms", package: "swift-async-algorithms"),
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                "StructuredConcurrency",
                "SubstrateOperation",
                "SubstrateSdkExt",
                "ExtrinsicServiceExt",
                "BackgroundExecution",
                "ChainStore"
            ]
        ),
        .target(
            name: "DurableTransactionsTestSupport",
            dependencies: [
                "DurableTransactions",
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                "SubstrateOperation",
                "BackgroundExecution",
                "ChainStore"
            ]
        ),
        .testTarget(
            name: "DurableTransactionsTests",
            dependencies: [
                "DurableTransactions",
                "DurableTransactionsTestSupport",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                "ChainStore"
            ]
        )
    ]
)
