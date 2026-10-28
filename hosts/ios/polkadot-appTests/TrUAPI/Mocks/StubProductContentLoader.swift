import struct TrUAPIHost.ProductId
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
        let contentIds = Set([resolved.executables.app?.identifier, resolved.executables.widget?.identifier, resolved.executables.worker?.identifier].compactMap { $0 })
        let contents = Dictionary(uniqueKeysWithValues: contentIds.map { contentId in
            (contentId, DotNsResolvedContent(contentHash: "00", localURL: contentURL.appendingPathComponent(contentId.rawValue)))
        })

        return ProductContent(
            resolved: resolved,
            contents: contents
        )
    }
}
