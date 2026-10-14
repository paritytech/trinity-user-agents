// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "UIKitExt",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "UIKitExt",
            targets: ["UIKitExt"]
        )
    ],
    targets: [
        .target(
            name: "UIKitExt"
        )
    ]
)
