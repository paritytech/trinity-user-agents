// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Revive",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "Revive",
            targets: ["Revive"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.11.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Operation-iOS",
            from: "3.0.0"
        ),
        .package(
            url: "https://github.com/attaswift/BigInt",
            from: "5.5.1"
        ),
        .package(
            url: "https://github.com/novasamatech/web3swift.git",
            from: "3.3.2"
        ),
        .package(path: "../ChainStore"),
        .package(path: "../SubstrateOperation"),
        .package(path: "../StructuredConcurrency")
    ],
    targets: [
        .target(
            name: "Revive",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStateCall", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "BigInt", package: "BigInt"),
                .product(name: "web3swift", package: "web3swift"),
                "ChainStore",
                "SubstrateOperation",
                "StructuredConcurrency"
            ]
        ),
        .testTarget(
            name: "ReviveTests",
            dependencies: [
                "Revive",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "BigInt", package: "BigInt"),
                "ChainStore"
            ]
        )
    ]
)
