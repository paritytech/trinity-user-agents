// swift-tools-version: 5.11
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "EventCenter",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "EventCenter",
            targets: ["EventCenter"]
        )
    ],
    targets: [
        .target(name: "EventCenter")
    ]
)
