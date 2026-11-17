import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product the app ships runs its worker from its own files: the builder
/// never asks the content loader, so no dotNS read happens for it.
struct TrUAPIWorkerBuilderTests {
    @Test
    func runsABundledWorkerWithoutOpeningItsContent() async throws {
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write(BundledFixture.workerManifest, to: root, "provider.dot/worker-manifest.json")
        try BundledFixture.write("export {}", to: root, "provider.dot/worker/index.js")

        let id = try ProductId("provider.dot")
        let loader = StubProductContentLoader()
        let resolver = StubDotNsResolver()
        let bundled = BundledProducts(root: root)
        let builder = makeBuilder(loader: loader, resolver: resolver, bundled: bundled)

        let product = try #require(bundled.product(id))
        let (source, files) = try await builder.workerSource(for: id)

        #expect(source == .bundled(product))
        #expect(files.load(for: id, relativePath: "worker/index.js") == Data("export {}".utf8))
        #expect(loader.openedWorkerIds.isEmpty)
        #expect(resolver.resolvedNames.isEmpty)
    }

    /// A rejected manifest costs a bundled product its worker, as it does a
    /// published one, and never sends it to dotNS for another.
    @Test
    func runsNoWorkerForABundledProductWhoseManifestIsRejected() async throws {
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write("{\"kind\":\"worker\"}", to: root, "provider.dot/worker-manifest.json")
        try BundledFixture.write("export {}", to: root, "provider.dot/worker/index.js")

        let id = try ProductId("provider.dot")
        let loader = StubProductContentLoader()
        let builder = makeBuilder(loader: loader, resolver: StubDotNsResolver(), bundled: BundledProducts(root: root))

        await #expect(throws: TrUAPIWorkerError.self) { try await builder.workerSource(for: id) }
        #expect(loader.openedWorkerIds.isEmpty)
    }

    @Test
    func opensTheContentOfAProductItDoesNotShip() async throws {
        let id = try ProductId("provider.dot")
        let loader = StubProductContentLoader()
        let builder = makeBuilder(loader: loader, resolver: StubDotNsResolver(), bundled: .none)

        _ = try? await builder.workerSource(for: id)

        #expect(loader.openedWorkerIds == [id])
    }

    private func makeBuilder(
        loader: StubProductContentLoader,
        resolver: StubDotNsResolver,
        bundled: BundledProducts
    ) -> TrUAPIWorkerBuilder {
        TrUAPIWorkerBuilder(
            environment: { throw NoEnvironment() },
            contentLoader: loader,
            dotNsResolver: resolver,
            productFileProvider: NoChatProductFiles(),
            sources: ProductSources(bundledProducts: bundled)
        )
    }
}

private struct NoEnvironment: Error {}
