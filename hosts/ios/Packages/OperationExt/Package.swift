// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "OperationExt",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "OperationExt",
            targets: ["OperationExt"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/Operation-iOS",
            from: "3.0.0"
        ),
        .package(
            url: "https://github.com/novasamatech/logger-ios",
            from: "0.0.1"
        ),
    ],
    targets: [
        .target(
            name: "OperationExt",
            dependencies: [
                .product(name: "Operation-iOS", package: "operation-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
            ]
        )
    ]
)
