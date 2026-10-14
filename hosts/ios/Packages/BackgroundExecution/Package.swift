// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "BackgroundExecution",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "BackgroundExecution",
            targets: ["BackgroundExecution"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
        .package(path: "../ChainRegistry"),
    ],
    targets: [
        .target(
            name: "BackgroundExecution",
            dependencies: [
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "ChainRegistry", package: "ChainRegistry"),
            ]
        ),
        .testTarget(
            name: "BackgroundExecutionTests",
            dependencies: [
                "BackgroundExecution",
                .product(name: "ChainRegistry", package: "ChainRegistry"),
            ]
        )
    ]
)
