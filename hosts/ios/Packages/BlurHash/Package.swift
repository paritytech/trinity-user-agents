// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "BlurHash",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "BlurHash", targets: ["BlurHash"])
    ],
    targets: [
        .target(name: "BlurHash"),
        .testTarget(
            name: "BlurHashTests",
            dependencies: ["BlurHash"],
            resources: [.process("Fixtures")]
        )
    ]
)
