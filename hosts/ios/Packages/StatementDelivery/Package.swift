// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "StatementDelivery",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "StatementDelivery",
            targets: ["StatementDelivery"]
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
        .package(
            url: "https://github.com/pointfreeco/swift-clocks",
            from: "1.0.6"
        ),
        .package(path: "../StatementStore"),
        .package(path: "../BackgroundExecution"),
        .package(path: "../IssueMonitoring"),
        .package(path: "../FoundationExt")
    ],
    targets: [
        .target(
            name: "StatementDelivery",
            dependencies: [
                .product(name: "AsyncExtensions", package: "AsyncExtensions"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "SDKLogger", package: "logger-ios"),
                .product(name: "StatementStore", package: "StatementStore"),
                .product(name: "BackgroundExecution", package: "BackgroundExecution"),
                .product(name: "IssueMonitoring", package: "IssueMonitoring"),
                .product(name: "FoundationExt", package: "FoundationExt")
            ]
        ),
        .testTarget(
            name: "StatementDeliveryTests",
            dependencies: [
                "StatementDelivery",
                .product(name: "Clocks", package: "swift-clocks"),
                .product(name: "FoundationExtTestSupport", package: "FoundationExt")
            ],
            path: "Tests"
        )
    ]
)
