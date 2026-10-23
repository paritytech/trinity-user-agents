import AsyncExtensions
import Foundation
import Products
@testable import polkadot_app

final class StubProductContentLoader: ProductContentLoading, @unchecked Sendable {
    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")
    private(set) var openedIds: [ProductId] = []

    func open(_ productId: ProductId, policy _: ProductLoadPolicy) async throws -> ProductContent {
        openedIds.append(productId)

        return ProductContent(
            resolved: .legacy(id: productId),
            appContent: DotNsResolvedContent(contentHash: "00", localURL: contentURL),
            version: ProductVersion(productId: productId, records: [:], contentHashes: [:]),
            source: .cache
        )
    }

    func refresh(_: ProductId) async -> ProductUpdate? { nil }

    func apply(_: ProductUpdate) async {}

    func updates() -> AnyAsyncSequence<ProductUpdate> {
        AsyncStream<ProductUpdate> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
