// swift-tools-version: 5.11

import PackageDescription

let package = Package(
    name: "AssetsManagement",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "AssetsManagement",
            targets: ["AssetsManagement"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(path: "../ChainStore"),
        .package(path: "../HydrationSdk"),
        .package(path: "../StructuredConcurrency"),
        .package(path: "../SubstrateSdkExt")
    ],
    targets: [
        .target(
            name: "AssetsManagement",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "ChainStore", package: "ChainStore"),
                .product(name: "HydrationSdk", package: "HydrationSdk"),
                "StructuredConcurrency",
                "SubstrateSdkExt"
            ]
        )
    ]
)
