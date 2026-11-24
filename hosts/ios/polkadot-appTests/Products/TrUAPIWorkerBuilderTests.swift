import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIWorkerBuilderTests {
    @Test
    func opensTheProductsContentThroughDotNs() async throws {
        let id = try ProductId("provider.dot")
        let loader = StubProductContentLoader()
        let builder = makeBuilder(loader: loader, resolver: StubDotNsResolver())

        _ = try? await builder.workerSource(for: id)

        #expect(loader.openedWorkerIds == [id])
    }

    private func makeBuilder(
        loader: StubProductContentLoader,
        resolver: StubDotNsResolver
    ) -> TrUAPIWorkerBuilder {
        TrUAPIWorkerBuilder(
            environment: { throw NoEnvironment() },
            contentLoader: loader,
            dotNsResolver: resolver,
            productFileProvider: NoChatProductFiles()
        )
    }
}

private struct NoEnvironment: Error {}
