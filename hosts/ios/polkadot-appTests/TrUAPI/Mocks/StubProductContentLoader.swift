import struct TrUAPIHost.ProductId
import enum TrUAPIHost.ExecutableKind
import AsyncExtensions
import Foundation
import IssueMonitoring
import os
import Products
@testable import polkadot_app

final class StubProductContentLoader: ProductContentLoading, @unchecked Sendable {
    private struct State {
        var openedIds: [ProductId] = []
        var openContexts: [HostLoadContext?] = []
        var openFailure: Error?
        var openedWorkerIds: [ProductId] = []
        var refreshedIds: [ProductId] = []
        var appliedUpdates: [ExecutableRelease] = []
        var contentHash = "00"
        var refreshWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
        var applyWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
        var keptHashesOnRemoval: [Set<String>] = []
        var removalWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    let contentURL = URL(fileURLWithPath: "/tmp/spa-test-content")

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let updateStream = AsyncStream<ExecutableRelease>.makeStream()

    private let product: ResolvedProduct?
    private static let contextReader = HostLoadTelemetry(recorder: NoopHostEventRecorder())

    init(product: ResolvedProduct? = nil) {
        self.product = product
    }

    var openedIds: [ProductId] { state.withLock { $0.openedIds } }
    /// The load context each `open` ran in.
    var openContexts: [HostLoadContext?] { state.withLock { $0.openContexts } }
    var openedWorkerIds: [ProductId] { state.withLock { $0.openedWorkerIds } }
    var refreshedIds: [ProductId] { state.withLock { $0.refreshedIds } }
    var appliedUpdates: [ExecutableRelease] { state.withLock { $0.appliedUpdates } }
    var keptHashesOnRemoval: [Set<String>] { state.withLock { $0.keptHashesOnRemoval } }

    func failOpens(with error: Error) {
        state.withLock { $0.openFailure = error }
    }

    func simulate(appliedContentHash: String) {
        state.withLock { $0.contentHash = appliedContentHash }
    }

    func simulate(update: ExecutableRelease) {
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
        let context = Self.contextReader.context
        let (contentHash, failure) = state.withLock { state in
            state.openedIds.append(productId)
            state.openContexts.append(context)
            return (state.contentHash, state.openFailure)
        }
        if let failure { throw failure }

        let resolved = product ?? .legacy(id: productId)
        let archive = try DotNsResolvedContent(
            contentHash: contentHash,
            localURL: contentURL.appendingPathComponent(resolved.contentId(for: kind).rawValue)
        )

        return ProductContent(resolved: resolved, kind: kind, archive: archive)
    }

    func openWorker(_ productId: ProductId, policy _: ProductLoadPolicy) async throws -> ResolvedProduct {
        state.withLock { $0.openedWorkerIds.append(productId) }
        return product ?? .legacy(id: productId)
    }

    @discardableResult
    func refresh(_ productId: ProductId) async -> [ExecutableRelease] {
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

    func apply(_ update: ExecutableRelease) async {
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

    func removeSupersededContent(keeping pinnedHashes: Set<String>) async {
        let ready = state.withLock { state in
            state.keptHashesOnRemoval.append(pinnedHashes)
            let count = state.keptHashesOnRemoval.count
            let ready = state.removalWaiters.filter { $0.count <= count }
            state.removalWaiters.removeAll { $0.count <= count }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
    }

    func waitForRemovals(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.keptHashesOnRemoval.count < count else { return true }
                state.removalWaiters.append((count, continuation))
                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }

    func updates() -> AnyAsyncSequence<ExecutableRelease> {
        updateStream.stream.eraseToAnyAsyncSequence()
    }
}
