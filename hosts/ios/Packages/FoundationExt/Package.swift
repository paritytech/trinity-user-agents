// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "FoundationExt",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "FoundationExt",
            targets: ["FoundationExt"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/novasamatech/Foundation-iOS",
            from: "1.4.0"
        ),
    ],
    targets: [
        .target(
            name: "FoundationExt",
            dependencies: [
                .product(name: "Foundation-iOS", package: "Foundation-iOS")
            ]
        ),
        .testTarget(
            name: "FoundationExtTests",
            dependencies: ["FoundationExt"]
        )
    ]
)
