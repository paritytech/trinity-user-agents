import Foundation
import os
import Products
@testable import polkadot_app

final class StubProductContentLoader: ProductContentLoading, @unchecked Sendable {
    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")

    private let product: ResolvedProduct?
    private let opened = OSAllocatedUnfairLock(initialState: [ProductId]())

    init(product: ResolvedProduct? = nil) {
        self.product = product
    }

    var openedIds: [ProductId] { opened.withLock { $0 } }

    func open(
        _ productId: ProductId,
        policy _: ProductLoadPolicy,
        onPhase _: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent {
        opened.withLock { $0.append(productId) }

        let resolved = product ?? .legacy(id: productId)
        let contentIds = Set(ExecutableKind.allCases.map(resolved.contentId(for:)))
        let contents = Dictionary(uniqueKeysWithValues: contentIds.map { contentId in
            (contentId, DotNsResolvedContent(contentHash: "00", localURL: contentURL.appendingPathComponent(contentId)))
        })

        return ProductContent(
            resolved: resolved,
            contents: contents,
            appContent: contents[resolved.appContentId] ?? DotNsResolvedContent(contentHash: "00", localURL: contentURL)
        )
    }
}
