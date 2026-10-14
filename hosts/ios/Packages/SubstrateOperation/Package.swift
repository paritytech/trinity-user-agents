// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "SubstrateOperation",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "SubstrateOperation",
            targets: ["SubstrateOperation"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.11.0"
        ),
        .package(path: "../XcmDefinition"),
        .package(path: "../ChainStore"),
        .package(path: "../StructuredConcurrency"),
        .package(path: "../SubstrateSdkExt"),
        .package(path: "../FoundationExt")
    ],
    targets: [
        .target(
            name: "SubstrateOperation",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStateCall", package: "substrate-sdk-ios"),
                .product(name: "SubstrateStorageQuery", package: "substrate-sdk-ios"),
                "XcmDefinition",
                "ChainStore",
                "StructuredConcurrency",
                "SubstrateSdkExt",
                "FoundationExt"
            ]
        ),
        .testTarget(
            name: "SubstrateOperationTests",
            dependencies: [
                "SubstrateOperation",
                "FoundationExt",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios")
            ]
        )
    ]
)
