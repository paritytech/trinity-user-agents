// swift-tools-version: 6.0
import PackageDescription

let uiDependencyConfigs: [DependencyConfig] = [
    .init(
        name: "UIKit-iOS",
        url: "https://github.com/novasamatech/UIKit-iOS",
        version: .exact("1.1.4"),
        products: ["UIKit-iOS"]
    ),
    .init(
        name: "SnapKit",
        url: "https://github.com/SnapKit/SnapKit",
        version: .exact("5.7.1"),
        products: ["SnapKit"]
    )
]

// MARK: - Config main

let package = Package(
    name: "UIDependencies",
    platforms: [
        .iOS(.v17)
    ],
    products: [
        .library(
            name: "UIDependencies",
            targets: ["UIDependencies"]
        )
    ],
    dependencies: uiDependencyConfigs.map(\.packageDependency),
    targets: [
        .target(
            name: "UIDependencies",
            dependencies: uiDependencyConfigs.flatMap(\.targetDependency),
            path: ""
        )
    ]
)

// MARK: - Shared helper

struct DependencyConfig {
    let name: String
    let url: String
    let version: VersionSpecifier
    let products: [String]

    enum VersionSpecifier {
        case upToNextMajor(String)
        case exact(String)
        case commit(String)
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
        }
    }

    var targetDependency: [Target.Dependency] {
        products.map { .product(name: $0, package: name) }
    }
}
