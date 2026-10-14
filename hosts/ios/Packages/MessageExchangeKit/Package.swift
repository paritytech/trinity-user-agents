// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "MessageExchangeKit",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "MessageExchangeKit",
            targets: ["MessageExchangeKit"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/substrate-sdk-ios",
            from: "5.7.0"
        ),
        .package(
            url: "https://github.com/novasamatech/Foundation-iOS",
            from: "1.4.0"
        ),
        .package(path: "../StatementStore")
    ],
    targets: [
        .target(
            name: "MessageExchangeKit",
            dependencies: [
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                .product(name: "Foundation-iOS", package: "Foundation-iOS"),
                .product(name: "StatementStore", package: "StatementStore")
            ]
        ),
        .testTarget(
            name: "MessageExchangeKitTests",
            dependencies: ["MessageExchangeKit"]
        )
    ]
)
