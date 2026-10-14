// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "ChainRegistry",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "ChainRegistry",
            targets: ["ChainRegistry"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Operation-iOS",
            from: "3.0.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Foundation-iOS",
            from: "1.5.0"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(
            url: "https://github.com/attaswift/BigInt",
            from: "5.0.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Crypto-iOS",
            from: "0.5.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Starscream.git",
            from: "4.0.13"
        ),
        .package(path: "../ChainStore"),
        .package(path: "../CommonService"),
        .package(path: "../OperationExt"),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../EventCenter")
    ],
    targets: [
        .target(
            name: "ChainRegistry",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStateCall", package: "substrate-sdk-ios"),
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "Foundation-iOS", package: "Foundation-iOS"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "NovaCrypto", package: "Crypto-iOS"),
                "BigInt",
                "Starscream",
                "ChainStore",
                "CommonService",
                "OperationExt",
                "SubstrateSdkExt",
                "EventCenter"
            ]
        ),
        .testTarget(
            name: "ChainRegistryTests",
            dependencies: [
                "ChainRegistry",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "Foundation-iOS", package: "Foundation-iOS")
            ]
        )
    ]
)
