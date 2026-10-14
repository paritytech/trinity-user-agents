// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "AssetHubSdk",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "AssetHubSdk",
            targets: ["AssetHubSdk"]
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
        .package(path: "../CommonService"),
        .package(path: "../XcmDefinition"),
        .package(path: "../AssetExchange")
    ],
    targets: [
        .target(
            name: "AssetHubSdk",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "ExtrinsicService", package: "extrinsic-service-ios"),
                .product(name: "XcmDefinition", package: "XcmDefinition"),
                .product(name: "CommonService", package: "CommonService"),
                .product(name: "AssetExchange", package: "AssetExchange")
            ]
        )
    ]
)
