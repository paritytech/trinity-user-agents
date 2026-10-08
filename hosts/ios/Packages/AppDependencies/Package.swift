// swift-tools-version: 6.0
import Foundation
import PackageDescription

// This app is built two ways: vendored into the host monorepo, where the shared
// core sits in the same tree, and checked out on its own, where the core is a
// published dependency. Detect which by looking for the core's crates, the same
// marker the Android host checks for.
//
// The path is derived from #filePath rather than the working directory, which is
// not guaranteed to be the package root while the manifest is evaluated.
let corePath = "../../../.."
let coreIsInTree: Bool = {
    var root = URL(fileURLWithPath: #filePath)
    for _ in 0 ..< 5 { root.deleteLastPathComponent() }
    return FileManager.default.fileExists(
        atPath: root.appendingPathComponent("rust/crates/truapi").path
    )
}()

let dependencyConfigs: [DependencyConfig] = [
    .init(
        name: "extrinsic-service-ios",
        url: "https://github.com/novasamatech/extrinsic-service-ios",
        version: .exact("1.20.0"),
        products: ["ExtrinsicService"]
    ),
    .init(
        name: "verifiable-swift",
        url: "https://github.com/novasamatech/verifiable-swift",
        version: .exact("0.8.0"),
        products: ["BandersnatchApi"]
    ),
    .init(
        name: "firebase-ios-sdk",
        url: "https://github.com/firebase/firebase-ios-sdk",
        version: .exact("12.6.0"),
        products: [
            "FirebaseCore",
            "FirebaseRemoteConfig"
        ]
    ),
    .init(
        name: "Operation-iOS",
        url: "https://github.com/novasamatech/Operation-iOS",
        version: .exact("3.1.0"),
        products: ["Operation-iOS"]
    ),
    .init(
        name: "Foundation-iOS",
        url: "https://github.com/novasamatech/Foundation-iOS",
        version: .upToNextMajor("1.5.0"),
        products: ["Foundation-iOS"]
    ),
    .init(
        name: "Kingfisher",
        url: "https://github.com/onevcat/Kingfisher",
        version: .exact("8.2.0"),
        products: ["Kingfisher"]
    ),
    .init(
        name: "QRCode",
        url: "https://github.com/dagronf/QRCode",
        version: .exact("26.1.0"),
        products: ["QRCode"]
    ),
    .init(
        name: "SnapKit",
        url: "https://github.com/SnapKit/SnapKit",
        version: .exact("5.7.1"),
        products: ["SnapKit"]
    ),
    .init(
        name: "substrate-sdk-ios",
        url: "https://github.com/novasamatech/substrate-sdk-ios",
        version: .exact("5.17.0"),
        products: [
            "SubstrateSdk",
            "SubstrateMetadataHash"
        ]
    ),
    .init(
        name: "SVGKit",
        url: "https://github.com/SVGKit/SVGKit/",
        version: .exact("3.0.0"),
        products: ["SVGKit"]
    ),
    .init(
        name: "swift-algorithms",
        url: "https://github.com/apple/swift-algorithms.git",
        version: .exact("1.2.1"),
        products: ["Algorithms"]
    ),
    .init(
        name: "SwiftyBeaver",
        url: "https://github.com/SwiftyBeaver/SwiftyBeaver",
        version: .exact("2.1.1"),
        products: ["SwiftyBeaver"]
    ),
    .init(
        name: "swift-cid",
        url: "https://github.com/swift-libp2p/swift-cid.git",
        version: .upToNextMajor("0.0.4"),
        products: ["CID"]
    ),
    .init(
        name: "UIKit-iOS",
        url: "https://github.com/novasamatech/UIKit-iOS",
        version: .exact("1.1.4"),
        products: ["UIKit-iOS"]
    ),
    .init(
        name: "unique-device-ios",
        url: "https://github.com/novasamatech/unique-device-ios",
        version: .exact("0.4.0"),
        products: ["UniqueDevice"]
    ),
    .init(
        name: "ZipArchive",
        url: "https://github.com/ZipArchive/ZipArchive",
        version: .upToNextMajor("2.6.0"),
        products: ["ZipArchive"]
    ),
    .init(
        name: "swift-async-algorithms",
        url: "https://github.com/apple/swift-async-algorithms",
        version: .exact("1.0.4"),
        products: ["AsyncAlgorithms"]
    ),
    .init(
        name: "AsyncExtensions",
        url: "https://github.com/paritytech/AsyncExtensions",
        version: .exact("0.5.6"),
        products: ["AsyncExtensions"]
    ),
    .init(
        name: "WebRTC",
        url: "https://github.com/stasel/WebRTC",
        version: .exact("149.0.0"),
        products: ["WebRTC"]
    ),
    .init(
        name: "trinity-user-agents",
        url: coreIsInTree ? corePath : "https://github.com/paritytech/trinity-user-agents",
        version: coreIsInTree ? .local : .exact("0.16.0"),
        products: ["TrUAPIHost"]
    ),
    .init(
        name: "lottie-ios",
        url: "https://github.com/airbnb/lottie-ios",
        version: .exact("4.5.2"),
        products: ["Lottie"]
    ),
    .init(
        name: "swift-clocks",
        url: "https://github.com/pointfreeco/swift-clocks",
        version: .exact("1.0.6"),
        products: ["Clocks"]
    ),
    .init(
        name: "swift-custom-dump",
        url: "https://github.com/pointfreeco/swift-custom-dump",
        version: .exact("1.4.1"),
        products: ["CustomDump"]
    )
]

// MARK: - Config main

let package = Package(
    name: "AppDependencies",
    platforms: [
        .iOS(.v17)
    ],
    products: [
        .library(
            name: "AppDependencies",
            targets: ["AppDependencies"]
        )
    ],
    dependencies: dependencyConfigs.map(\.packageDependency),
    targets: [
        .target(
            name: "AppDependencies",
            dependencies: dependencyConfigs.flatMap(\.targetDependency),
            path: ""
        )
    ]
)

// MARK: -

struct DependencyConfig {
    let name: String
    let url: String
    let version: VersionSpecifier
    let products: [String]

    enum VersionSpecifier {
        case upToNextMajor(String)
        case exact(String)
        case commit(String)
        case branch(String)
        case local
    }
}

extension DependencyConfig {
    var packageDependency: Package.Dependency {
        switch version {
        case let .upToNextMajor(version):
            .package(url: url, from: Version(stringLiteral: version))
        case let .exact(version):
            .package(url: url, exact: Version(stringLiteral: version))
        case let .commit(hash):
            .package(url: url, revision: hash)
        case let .branch(name):
            .package(url: url, branch: name)
        case .local:
            // The name has to be declared. Swift Package Manager otherwise
            // derives a path dependency's identity from its directory, which is
            // the checkout folder, so `.product(package:)` would not match.
            .package(name: name, path: url)
        }
    }

    var targetDependency: [Target.Dependency] {
        products.map { .product(name: $0, package: name) }
    }
}
