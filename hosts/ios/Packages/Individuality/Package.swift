// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Individuality",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "Individuality",
            targets: ["Individuality"]
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
        .package(path: "../StructuredConcurrency"),
        .package(
            url: "https://github.com/pointfreeco/swift-clocks",
            from: "1.0.6"
        ),
        .package(path: "../KeyDerivation"),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../ChainStore"),
        .package(path: "../SubstrateOperation"),
        .package(path: "../BulletinChain"),
        .package(path: "../BackgroundExecution"),
        .package(path: "../DurableTransactions"),
        .package(path: "../IssueMonitoring")
    ],
    targets: [
        .target(
            name: "Individuality",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageSubscription", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                "StructuredConcurrency",
                "KeyDerivation",
                "SubstrateSdkExt",
                "ChainStore",
                "SubstrateOperation",
                "BulletinChain",
                "BackgroundExecution",
                "DurableTransactions",
                "IssueMonitoring"
            ],
            path: "Sources"
        ),
        .testTarget(
            name: "IndividualityTests",
            dependencies: [
                "Individuality",
                "BackgroundExecution",
                .product(name: "DurableTransactionsTestSupport", package: "DurableTransactions"),
                .product(name: "Clocks", package: "swift-clocks")
            ],
            path: "Tests"
        )
    ]
)
