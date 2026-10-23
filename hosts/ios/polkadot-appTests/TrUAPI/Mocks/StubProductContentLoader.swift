import AsyncExtensions
import Foundation
import os
import Products
@testable import polkadot_app

final class StubProductContentLoader: ProductContentLoading, @unchecked Sendable {
    private struct State {
        var openedIds: [ProductId] = []
        var refreshedIds: [ProductId] = []
        var appliedUpdates: [ProductUpdate] = []
        var pendingUpdates: [ProductUpdate] = []
        var contentHash = "00"
        var refreshWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let updateStream = AsyncStream<ProductUpdate>.makeStream()

    init(pendingUpdates: [ProductUpdate] = []) {
        state.withLock { $0.pendingUpdates = pendingUpdates }
    }

    var openedIds: [ProductId] { state.withLock { $0.openedIds } }
    var refreshedIds: [ProductId] { state.withLock { $0.refreshedIds } }
    var appliedUpdates: [ProductUpdate] { state.withLock { $0.appliedUpdates } }

    func simulate(appliedContentHash: String) {
        state.withLock { $0.contentHash = appliedContentHash }
    }

    func simulate(update: ProductUpdate) {
        updateStream.continuation.yield(update)
    }

    func waitForRefreshes(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.refreshedIds.count < count else { return true }
                state.refreshWaiters.append((count, continuation))
                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }

    func open(_ productId: ProductId, policy _: ProductLoadPolicy) async throws -> ProductContent {
        let contentHash = state.withLock { state in
            state.openedIds.append(productId)
            return state.contentHash
        }

        return ProductContent(
            resolved: .legacy(id: productId),
            appContent: DotNsResolvedContent(
                contentHash: contentHash,
                localURL: contentURL.appendingPathComponent(contentHash)
            ),
            version: ProductVersion(productId: productId, records: [:], contentHashes: [:]),
            source: .cache
        )
    }

    func refresh(_ productId: ProductId) async -> ProductUpdate? {
        let ready = state.withLock { state in
            state.refreshedIds.append(productId)
            let count = state.refreshedIds.count
            let ready = state.refreshWaiters.filter { $0.count <= count }
            state.refreshWaiters.removeAll { $0.count <= count }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
        return nil
    }

    func apply(_ update: ProductUpdate) async {
        state.withLock { $0.appliedUpdates.append(update) }
    }

    func pendingUpdates() async -> [ProductUpdate] {
        state.withLock { $0.pendingUpdates }
    }

    func updates() -> AnyAsyncSequence<ProductUpdate> {
        updateStream.stream.eraseToAnyAsyncSequence()
    }
}
