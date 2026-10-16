// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "ExtrinsicServiceExt",
    defaultLocalization: "en",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "ExtrinsicServiceExt",
            targets: ["ExtrinsicServiceExt"]
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
            url: "https://github.com/paritytech/AsyncExtensions",
            exact: "0.5.6"
        ),
        .package(path: "../StructuredConcurrency"),
        .package(path: "../ChainStore"),
        .package(path: "../SubstrateOperation")
    ],
    targets: [
        .target(
            name: "ExtrinsicServiceExt",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                "StructuredConcurrency",
                "ChainStore",
                "SubstrateOperation"
            ]
        ),
        .testTarget(
            name: "ExtrinsicServiceExtTests",
            dependencies: ["ExtrinsicServiceExt", "StructuredConcurrency"],
            path: "Tests"
        )
    ]
)
