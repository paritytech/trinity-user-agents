// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "StatementStore",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "StatementStore",
            targets: ["StatementStore"]
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
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(path: "../StructuredConcurrency"),
        .package(path: "../SubstrateSdkExt")
    ],
    targets: [
        .target(
            name: "StatementStore",
            dependencies: [
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "StructuredConcurrency", package: "StructuredConcurrency")
            ]
        ),
        .testTarget(
            name: "StatementStoreTests",
            dependencies: [
                "StatementStore",
                .product(name: "SubstrateSdkExt", package: "SubstrateSdkExt")
            ],
            path: "Tests"
        )
    ]
)
