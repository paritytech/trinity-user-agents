import AsyncExtensions
import Foundation
import os
import StructuredConcurrency

final class StubNetworkPath: NetworkPathProviding, @unchecked Sendable {
    private struct State {
        var requests = 0
        var waiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    private let subject: AsyncCurrentValueSubject<Bool>
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(isSatisfied: Bool = true) {
        subject = AsyncCurrentValueSubject(isSatisfied)
    }

    func simulate(isSatisfied: Bool) {
        subject.send(isSatisfied)
    }

    func waitForStreamRequests(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.requests < count else { return true }
                state.waiters.append((count, continuation))
                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }

    func pathStream() -> AnyAsyncSequence<Bool> {
        let ready = state.withLock { state in
            state.requests += 1
            let requests = state.requests
            let ready = state.waiters.filter { $0.count <= requests }
            state.waiters.removeAll { $0.count <= requests }
            return ready
        }

        ready.forEach { $0.continuation.resume() }

        return subject.eraseToAnyAsyncSequence()
    }
}
