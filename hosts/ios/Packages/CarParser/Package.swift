// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "CarParser",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "CarParser",
            targets: ["CarParser"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/apple/swift-protobuf.git",
            from: "1.28.0"
        ),
        .package(
            url: "https://github.com/valpackett/SwiftCBOR.git",
            from: "0.4.7"
        ),
        .package(
            url: "https://github.com/swift-libp2p/swift-cid.git",
            .upToNextMajor(from: "0.0.4")
        ),
        .package(
            url: "https://github.com/swift-libp2p/swift-varint.git",
            .upToNextMajor(from: "0.2.0")
        ),
    ],
    targets: [
        .target(
            name: "CarParser",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
                .product(name: "SwiftCBOR", package: "SwiftCBOR"),
                .product(name: "CID", package: "swift-cid"),
                .product(name: "VarInt", package: "swift-varint"),
            ]
        ),
        .testTarget(
            name: "CarParserTests",
            dependencies: ["CarParser"]
        ),
    ]
)
