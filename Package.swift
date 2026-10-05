// swift-tools-version: 5.10
//
// TrUAPIHost — iOS host package for the Rust TrUAPI core — and TrUAPIProvider —
// chain transport over an embedded smoldot light client with a bundled chain-spec
// catalog — consumed as SPM git dependencies (the manifest must live at the repo
// root for that). Package sources live under ios/truapi-host/ and
// ios/truapi-provider/. The two products are independent and release on separate
// tags. For both, the uniffi-generated bindings, the container resource and the
// xcframework are gitignored build outputs: regenerate them with the package's
// scripts/rebuild.sh, and publish the xcframework as a GitHub release asset
// with scripts/publish.sh.

import Foundation
import PackageDescription

// Which truapi_server.xcframework the package links.
//
// TRUAPI_USE_LOCAL_BINARY=1 forces the locally generated one, which is what CI
// passes. It is also preferred whenever it is present, because a host building
// against this tree needs it: the published asset comes from a release
// commit (#723), so bindings generated from this tree would be paired with an
// older binary, and the generated code checks that pairing at runtime. The
// staged xcframework is a gitignored build output, so a remote consumer never
// has one and keeps the published asset. Preferring it also covers Xcode opened
// from Finder, which cannot be handed an environment variable and evaluates this
// manifest before any build phase could run.
//
// The path comes from #filePath, not the working directory, which is not
// guaranteed to be the package root while the manifest is evaluated.
let stagedBinaryPath = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("ios/truapi-host/Binaries/truapi_server.xcframework")
    .path
let useLocalBinary = ProcessInfo.processInfo.environment["TRUAPI_USE_LOCAL_BINARY"] == "1"
    || FileManager.default.fileExists(atPath: stagedBinaryPath)

let publishedBinaryURL = "https://github.com/paritytech/trinity-user-agents/releases/download/%40parity%2Fios-host%400.23.0/truapi_server.xcframework.zip"
let publishedBinaryChecksum = "54ffc874297b10cc3f1876021cfe47b490c723acffd9601b073a237adf392aa4"

let binaryTarget: Target = useLocalBinary
    ? .binaryTarget(
        name: "truapiFFI_binary",
        path: "ios/truapi-host/Binaries/truapi_server.xcframework"
    )
    : .binaryTarget(
        name: "truapiFFI_binary",
        url: publishedBinaryURL,
        checksum: publishedBinaryChecksum
    )

// Set TRUAPI_PROVIDER_USE_LOCAL_BINARY=1 to build against the locally generated
// ios/truapi-provider/Binaries/truapi_provider.xcframework (run rebuild.sh
// first). Separate from TRUAPI_USE_LOCAL_BINARY because the products release
// independently: a local build of one must not require a local build of the other.
let useLocalProviderBinary =
    ProcessInfo.processInfo.environment["TRUAPI_PROVIDER_USE_LOCAL_BINARY"] == "1"

// Set by ios/truapi-provider/scripts/publish.sh.
let providerBinaryURL = "https://github.com/paritytech/trinity-user-agents/releases/download/%40parity%2Fios-provider%400.7.0/truapi_provider.xcframework.zip"
let providerBinaryChecksum = "04fd47522fad5b12048396efae5d34c40f049623678066215654a3c9166d2bb3"

let providerBinaryTarget: Target = useLocalProviderBinary
    ? .binaryTarget(
        name: "truapi_providerFFI_binary",
        path: "ios/truapi-provider/Binaries/truapi_provider.xcframework"
    )
    : .binaryTarget(
        name: "truapi_providerFFI_binary",
        url: providerBinaryURL,
        checksum: providerBinaryChecksum
    )

let package = Package(
    name: "TrUAPIHost",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "TrUAPIHost", targets: ["TrUAPIHost"]),
        .library(name: "TrUAPIProvider", targets: ["TrUAPIProvider"]),
    ],
    targets: [
        .systemLibrary(
            name: "truapiFFI",
            path: "ios/truapi-host/Sources/truapiFFI/include",
            pkgConfig: nil,
            providers: []
        ),
        binaryTarget,
        .target(
            name: "TrUAPIHost",
            dependencies: [
                "truapiFFI", "truapiFFI_binary",
            ],
            path: "ios/truapi-host/Sources/TrUAPIHost",
            resources: [.copy("Resources/truapi-container.js")]
        ),
        .testTarget(
            name: "TrUAPIHostTests",
            dependencies: ["TrUAPIHost"],
            path: "ios/truapi-host/Tests"
        ),
        .systemLibrary(
            name: "truapi_providerFFI",
            path: "ios/truapi-provider/Sources/truapi_providerFFI/include",
            pkgConfig: nil,
            providers: []
        ),
        providerBinaryTarget,
        .target(
            name: "TrUAPIProvider",
            dependencies: ["truapi_providerFFI", "truapi_providerFFI_binary"],
            path: "ios/truapi-provider/Sources/TrUAPIProvider"
        ),
    ]
)
