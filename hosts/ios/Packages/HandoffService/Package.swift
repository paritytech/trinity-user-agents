// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "HandoffService",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "HandoffService",
            targets: ["HandoffService"]
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
            url: "https://github.com/swift-libp2p/swift-cid.git",
            .upToNextMajor(from: "0.0.4")
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(path: "../FoundationExt"),
        .package(path: "../StructuredConcurrency"),
    ],
    targets: [
        .target(
            name: "HandoffService",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "NovaCrypto", package: "crypto-ios"),
                .product(name: "FoundationExt", package: "FoundationExt"),
                .product(name: "CID", package: "swift-cid"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "StructuredConcurrency", package: "StructuredConcurrency")
            ]
        ),
        .testTarget(
            name: "HandoffServiceTests",
            dependencies: [
                "HandoffService",
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "NovaCrypto", package: "crypto-ios"),
                .product(name: "FoundationExt", package: "FoundationExt")
            ]
        )
    ]
)
