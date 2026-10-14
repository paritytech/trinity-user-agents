// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Products",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "Products",
            targets: ["Products"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Crypto-iOS",
            from: "0.3.0"
        ),
        .package(
            url: "https://github.com/attaswift/BigInt",
            from: "5.5.1"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(
            url: "https://github.com/novasamatech/Keystore-iOS",
            from: "1.0.1"
        ),
        .package(path: "../CarParser"),
        .package(path: "../FoundationExt"),
        .package(path: "../KeyDerivation"),
        .package(path: "../StructuredConcurrency"),
        .package(
            url: "https://github.com/swift-libp2p/swift-cid.git",
            .upToNextMajor(from: "0.0.4")
        ),
        .package(
            url: "https://github.com/paritytech/AsyncExtensions",
            exact: "0.5.6"
        ),
        .package(path: "../StatementStore"),
        .package(path: "../Individuality"),
        .package(path: "../UIKitExt"),
        .package(path: "../AssetsManagement"),
        .package(path: "../ChainStore"),
        .package(path: "../SubstrateOperation"),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../Revive")
    ],
    targets: [
        .target(
            name: "Products",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "NovaCrypto", package: "crypto-ios"),
                .product(name: "BigInt", package: "BigInt"),
                .product(name: "SDKLogger", package: "logger-ios"),
                "CarParser",
                "FoundationExt",
                .product(name: "Keystore-iOS", package: "keystore-ios"),
                .product(name: "KeyDerivation", package: "KeyDerivation"),
                .product(name: "CID", package: "swift-cid"),
                .product(name: "StructuredConcurrency", package: "StructuredConcurrency"),
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "StatementStore", package: "StatementStore"),
                "Individuality",
                "UIKitExt",
                .product(name: "AssetsManagement", package: "AssetsManagement"),
                .product(name: "ChainStore", package: "ChainStore"),
                "SubstrateOperation",
                "SubstrateSdkExt",
                "Revive"
            ],
            resources: [.process("Resources")]
        ),
        .testTarget(
            name: "ProductsTests",
            dependencies: [
                "Products",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "KeyDerivation", package: "KeyDerivation"),
                "Revive",
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
            ]
        )
    ]
)
