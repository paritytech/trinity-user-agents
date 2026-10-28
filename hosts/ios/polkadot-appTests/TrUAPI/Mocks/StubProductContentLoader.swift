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
        var applyWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let updateStream = AsyncStream<ProductUpdate>.makeStream()

    private let product: ResolvedProduct?

    init(pendingUpdates: [ProductUpdate] = [], product: ResolvedProduct? = nil) {
        self.product = product
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

    func open(
        _ productId: ProductId,
        policy _: ProductLoadPolicy,
        onPhase _: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent {
        let contentHash = state.withLock { state in
            state.openedIds.append(productId)
            return state.contentHash
        }

        let resolved = product ?? .legacy(id: productId)
        let contentIds = Set(ExecutableKind.allCases.map(resolved.contentId(for:)))
        let contents = Dictionary(uniqueKeysWithValues: contentIds.map { contentId in
            (
                contentId,
                DotNsResolvedContent(contentHash: contentHash, localURL: contentURL.appendingPathComponent(contentId))
            )
        })

        return ProductContent(resolved: resolved, records: [:], contents: contents)
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
        let ready = state.withLock { state in
            state.appliedUpdates.append(update)
            let count = state.appliedUpdates.count
            let ready = state.applyWaiters.filter { $0.count <= count }
            state.applyWaiters.removeAll { $0.count <= count }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
    }

    func waitForApplies(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.appliedUpdates.count < count else { return true }
                state.applyWaiters.append((count, continuation))
                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }

    func pendingUpdates() async -> [ProductUpdate] {
        state.withLock { $0.pendingUpdates }
    }

    func updates() -> AnyAsyncSequence<ProductUpdate> {
        updateStream.stream.eraseToAnyAsyncSequence()
    }
}
