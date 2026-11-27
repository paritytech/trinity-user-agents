import struct TrUAPIHost.ProductId
import Foundation
import Products
import Testing
@testable import polkadot_app

private let ramp = try! ProductId("ramp.paseo")
private let other = try! ProductId("other.paseo")

/// Where a provider's funding icons live in its worker archive, and that a
/// screen gets nil, so it keeps the monogram, whenever that archive has none.
struct FundingIconsTests {
    @Test
    func namesTheIconByItsLowercasedId() {
        #expect(FundingIconPath.path(.network, id: "Polkadot") == "funding/icons/networks/polkadot.png")
        #expect(FundingIconPath.path(.token, id: "USDC") == "funding/icons/tokens/usdc.png")
        #expect(FundingIconPath.path(.network, id: "bnb-chain2") == "funding/icons/networks/bnb-chain2.png")
    }

    @Test
    func anIdOutsideTheSetHasNoIcon() {
        #expect(FundingIconPath.path(.token, id: "") == nil)
        #expect(FundingIconPath.path(.token, id: "usd.c") == nil)
        #expect(FundingIconPath.path(.network, id: "../eth") == nil)
        #expect(FundingIconPath.path(.network, id: "arb_one") == nil)
        #expect(FundingIconPath.path(.token, id: "ÜSD") == nil)
    }

    @Test
    func findsTheIconInTheProvidersArchive() async throws {
        let root = try makeArchive(files: ["funding/icons/tokens/usdc.png"])
        let loader = makeLoader(roots: [ramp: root])

        let url = try #require(await loader.url(.token, id: "USDC", provider: ramp))

        #expect(url.lastPathComponent == "usdc.png")
    }

    @Test
    func aMissingFileHasNoIcon() async throws {
        let root = try makeArchive(files: ["funding/icons/tokens/usdc.png"])
        let loader = makeLoader(roots: [ramp: root])

        #expect(await loader.url(.token, id: "usdt", provider: ramp) == nil)
        #expect(await loader.image(.network, id: "polkadot", provider: ramp) == nil)
    }

    @Test
    func neverTakesAnotherProvidersIcon() async throws {
        let root = try makeArchive(files: ["funding/icons/networks/polkadot.png"])
        let otherRoot = try makeArchive(files: [])
        let loader = makeLoader(roots: [ramp: root, other: otherRoot])

        #expect(await loader.url(.network, id: "polkadot", provider: ramp) != nil)
        #expect(await loader.url(.network, id: "polkadot", provider: other) == nil)
    }

    @Test
    func aProviderWithNoArchiveHasNoIcon() async {
        let loader = makeLoader(roots: [:])

        #expect(await loader.image(.token, id: "usdc", provider: ramp) == nil)
    }
}

// MARK: - Fixtures

/// Each provider's worker archive is named after the provider itself.
private func makeLoader(roots: [ProductId: URL]) -> FundingIconLoader {
    FundingIconLoader(
        archive: ProductWorkerArchive(
            dotNsResolver: FailingResolver(),
            cachedRoot: { contentId in roots[contentId] }
        ),
        workerArchive: { $0 }
    )
}

private func makeArchive(files: [String]) throws -> URL {
    let root = URL.temporaryDirectory
        .appending(path: "funding-icons-\(UUID().uuidString)")
        .appending(path: "content")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    for path in files {
        let file = root.appending(path: path)
        try FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("png".utf8).write(to: file)
    }
    return root
}

private struct FailingResolver: DotNsResolverProtocol {
    func resolveToLocalURL(dotNsName: String) async throws -> URL {
        throw DotNsResolverError.resolutionFailed(dotNsName)
    }

    func clearCache() throws {}
}
