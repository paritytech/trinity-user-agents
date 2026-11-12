// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "CommonService",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "CommonService",
            targets: ["CommonService"]
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
        .package(path: "../ChainStore")
    ],
    targets: [
        .target(
            name: "CommonService",
            dependencies: [
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageSubscription", package: "substrate-sdk-ios"),
                .product(name: "ChainStore", package: "ChainStore")
            ]
        ),
        .testTarget(
            name: "CommonServiceTests",
            dependencies: ["CommonService"],
            path: "Tests"
        )
    ]
)
