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
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bundled-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("provider.dot/worker/index.js")
        try FileManager.default.createDirectory(
            at: script.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("export {}".utf8).write(to: script)

        let id = try ProductId("provider.dot")
        let loader = StubProductContentLoader()
        let resolver = StubDotNsResolver()
        let bundled = BundledProducts(root: root)
        let builder = makeBuilder(loader: loader, resolver: resolver, bundled: bundled)

        let entry = try #require(bundled.entries.first)
        let (source, files) = try await builder.workerSource(for: id)

        #expect(source == .bundled(entry))
        #expect(files.load(for: id, relativePath: "worker/index.js") == Data("export {}".utf8))
        #expect(loader.openedWorkerIds.isEmpty)
        #expect(resolver.resolvedNames.isEmpty)
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
            bundledProducts: bundled
        )
    }
}

private struct NoEnvironment: Error {}
