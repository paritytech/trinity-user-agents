// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "ChainStore",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "ChainStore",
            targets: ["ChainStore"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        )
    ],
    targets: [
        .target(
            name: "ChainStore",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios")
            ]
        )
    ]
)
