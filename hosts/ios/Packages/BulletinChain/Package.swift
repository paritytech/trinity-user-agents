// swift-tools-version: 5.10

import PackageDescription

let package = Package(
    name: "BulletinChain",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "BulletinChain",
            targets: ["BulletinChain"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/swift-libp2p/swift-cid.git",
            .upToNextMajor(from: "0.0.4")
        ),
        .package(path: "../FoundationExt"),
        .package(url: "https://github.com/novasamatech/substrate-sdk-ios", from: "5.7.1")
    ],
    targets: [
        .target(
            name: "BulletinChain",
            dependencies: [
                .product(name: "CID", package: "swift-cid"),
                .product(name: "SubstrateSdk", package: "substrate-sdk-ios"),
                "FoundationExt"
            ]
        )
    ]
)
