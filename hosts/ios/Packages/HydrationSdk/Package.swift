// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "HydrationSdk",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "HydrationSdk",
            targets: ["HydrationSdk"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/attaswift/BigInt",
            from: "5.5.1"
        ),
        .package(path: "../ChainStore"),
        .package(path: "../AssetExchange")
    ],
    targets: [
        .target(
            name: "HydrationSdk",
            dependencies: [
                .product(name: "BigInt", package: "BigInt"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "ChainStore", package: "ChainStore"),
                .product(name: "AssetExchange", package: "AssetExchange")
            ]
        )
    ]
)
