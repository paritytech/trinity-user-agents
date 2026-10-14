// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "PolkadotUI",
    defaultLocalization: "en",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "PolkadotUI",
            targets: ["PolkadotUI"]
        )
    ],
    dependencies: [
        .package(path: "../UIDependencies"),
        .package(path: "../FoundationExt"),
        .package(path: "../UIKitExt"),
        .package(path: "../ExternalAccessibility"),
        .package(
            url: "https://github.com/paritytech/polkadot-app-design-system-ios",
            from: "0.0.18"
        ),
        .package(
            url: "https://github.com/novasamatech/Foundation-iOS",
            from: "1.4.0"
        )
    ],
    targets: [
        .target(
            name: "PolkadotUI",
            dependencies: [
                "UIDependencies",
                "FoundationExt",
                "UIKitExt",
                "ExternalAccessibility",
                .product(name: "DesignSystem", package: "polkadot-app-design-system-ios"),
                .product(name: "Foundation-iOS", package: "Foundation-iOS")
            ],
            resources: [
                .process("Typography/Fonts"),
                .process("Resources")
            ]
        ),
        .testTarget(
            name: "PolkadotUITests",
            dependencies: ["PolkadotUI"],
            path: "Tests"
        )
    ]
)
