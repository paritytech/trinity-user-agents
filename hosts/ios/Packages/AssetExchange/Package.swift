// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "AssetExchange",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "AssetExchange",
            targets: ["AssetExchange"]
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
            url: "https://github.com/novasamatech/hydra-math-swift",
            exact: "0.4.1"
        ),
        .package(path: "../ChainStore"),
        .package(path: "../CommonService"),
        .package(path: "../SubstrateOperation"),
        .package(path: "../XcmDefinition")
    ],
    targets: [
        .target(
            name: "AssetExchange",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageSubscription", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStateCall", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                .product(name: "HydraMathApi", package: "hydra-math-swift"),
                .product(name: "ChainStore", package: "ChainStore"),
                .product(name: "CommonService", package: "CommonService"),
                .product(name: "SubstrateOperation", package: "SubstrateOperation"),
                .product(name: "XcmDefinition", package: "XcmDefinition")
            ]
        )
    ]
)
