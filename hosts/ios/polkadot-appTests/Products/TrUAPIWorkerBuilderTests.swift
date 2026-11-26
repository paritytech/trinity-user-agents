import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIWorkerBuilderTests {
    @Test
    func fetchesThePublishedWorkerArchiveBeforeReturningItsEntry() async throws {
        let id = try ProductId("provider.dot")
        let archive = try ProductId("worker.provider.dot")
        let worker = ProductExecutable.Worker(
            identifier: archive, appVersion: .zero, entrypoint: "chat.js", modalities: [.chat]
        )
        let loader = StubProductContentLoader(product: ResolvedProduct(
            id: id, displayName: "Provider", description: nil, icon: nil,
            executables: ProductExecutables(app: nil, widget: nil, worker: worker), hasManifest: true
        ))
        let resolver = StubDotNsResolver()
        let builder = TrUAPIWorkerBuilder(
            contentLoader: loader,
            dotNsResolver: resolver,
            productFileProvider: NoChatProductFiles()
        )

        let (source, _) = try await builder.workerSource(for: id)

        #expect(loader.openedWorkerIds == [id])
        #expect(resolver.resolvedNames == [archive.rawValue])
        #expect(source == ProductWorkerSource(contentId: archive, entryRelativePath: "chat.js"))
    }
}
