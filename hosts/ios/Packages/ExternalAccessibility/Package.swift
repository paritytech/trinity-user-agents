// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "ExternalAccessibility",
    platforms: [.iOS(.v14)],
    products: [
        .library(
            name: "ExternalAccessibility",
            targets: ["ExternalAccessibility"]
        )
    ],
    targets: [
        .target(
            name: "ExternalAccessibility"
        )
    ]
)
