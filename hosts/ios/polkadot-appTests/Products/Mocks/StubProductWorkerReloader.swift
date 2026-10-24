import Foundation
import os
import Products
@testable import polkadot_app

final class StubProductWorkerReloader: ProductWorkerReloading, @unchecked Sendable {
    private struct State {
        var reloadedIds: [ProductId] = []
        var waiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var reloadedIds: [ProductId] { state.withLock { $0.reloadedIds } }

    func waitForReloads(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.reloadedIds.count < count else { return true }
                state.waiters.append((count, continuation))
                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }

    func reloadWorker(of productId: ProductId) async {
        let ready = state.withLock { state in
            state.reloadedIds.append(productId)
            let count = state.reloadedIds.count
            let ready = state.waiters.filter { $0.count <= count }
            state.waiters.removeAll { $0.count <= count }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
    }
}
