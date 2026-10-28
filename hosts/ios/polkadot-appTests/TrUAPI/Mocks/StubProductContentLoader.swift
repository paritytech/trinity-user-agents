import AsyncExtensions
import Foundation
import os
import Products
@testable import polkadot_app

final class StubProductContentLoader: ProductContentLoading, @unchecked Sendable {
    private struct State {
        var openedIds: [ProductId] = []
        var refreshedIds: [ProductId] = []
        var appliedUpdates: [ProductContentUpdate] = []
        var contentHash = "00"
        var refreshWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
        var applyWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let updateStream = AsyncStream<ProductContentUpdate>.makeStream()

    private let product: ResolvedProduct?

    init(product: ResolvedProduct? = nil) {
        self.product = product
    }

    var openedIds: [ProductId] { state.withLock { $0.openedIds } }
    var refreshedIds: [ProductId] { state.withLock { $0.refreshedIds } }
    var appliedUpdates: [ProductContentUpdate] { state.withLock { $0.appliedUpdates } }

    func simulate(appliedContentHash: String) {
        state.withLock { $0.contentHash = appliedContentHash }
    }

    func simulate(update: ProductContentUpdate) {
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
        kind: ExecutableKind,
        policy _: ProductLoadPolicy,
        onPhase _: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent {
        let contentHash = state.withLock { state in
            state.openedIds.append(productId)
            return state.contentHash
        }

        let resolved = product ?? .legacy(id: productId)
        let archive = DotNsResolvedContent(
            contentHash: contentHash,
            localURL: contentURL.appendingPathComponent(resolved.contentId(for: kind))
        )

        return ProductContent(resolved: resolved, kind: kind, archive: archive)
    }

    func openWorker(_ productId: ProductId) async throws -> ResolvedProduct {
        product ?? .legacy(id: productId)
    }

    func refresh(_ productId: ProductId) async -> [ProductContentUpdate] {
        let ready = state.withLock { state in
            state.refreshedIds.append(productId)
            let count = state.refreshedIds.count
            let ready = state.refreshWaiters.filter { $0.count <= count }
            state.refreshWaiters.removeAll { $0.count <= count }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
        return []
    }

    func apply(_ update: ProductContentUpdate) async {
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

    func updates() -> AnyAsyncSequence<ProductContentUpdate> {
        updateStream.stream.eraseToAnyAsyncSequence()
    }
}
