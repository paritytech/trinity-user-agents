// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "StateMachine",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "StateMachine",
            targets: ["StateMachine"]
        )
    ],
    targets: [
        .target(
            name: "StateMachine"
        ),
        .testTarget(
            name: "StateMachineTests",
            dependencies: ["StateMachine"],
            path: "Tests"
        )
    ]
)
