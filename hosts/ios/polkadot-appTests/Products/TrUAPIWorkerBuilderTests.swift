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
        let builder = TrUAPIWorkerBuilder(
            contentLoader: loader,
            dotNsResolver: StubDotNsResolver(),
            productFileProvider: NoChatProductFiles()
        )

        _ = try? await builder.workerSource(for: id)

        #expect(loader.openedWorkerIds == [id])
    }
}
