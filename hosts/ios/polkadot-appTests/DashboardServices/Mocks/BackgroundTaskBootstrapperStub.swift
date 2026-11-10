import Foundation
import os

@testable import polkadot_app

final class BackgroundTaskBootstrapperStub: BackgroundTaskBootstrapping, @unchecked Sendable {
    private struct State {
        var waitCallCount = 0
        var heldWaits: [CheckedContinuation<Void, Never>] = []
        var isHolding: Bool
    }

    private let readiness: BackgroundTaskReadiness
    private let state: OSAllocatedUnfairLock<State>

    init(readiness: BackgroundTaskReadiness = .dashboard, holdsReadiness: Bool = false) {
        self.readiness = readiness
        state = OSAllocatedUnfairLock(initialState: State(isHolding: holdsReadiness))
    }

    var waitCallCount: Int {
        state.withLock(\.waitCallCount)
    }

    func waitWhenReady(deadline _: Duration) async -> BackgroundTaskReadiness {
        await withCheckedContinuation { continuation in
            let resumeNow = state.withLock { state -> Bool in
                state.waitCallCount += 1
                guard state.isHolding else { return true }
                state.heldWaits.append(continuation)
                return false
            }

            if resumeNow {
                continuation.resume()
            }
        }

        return readiness
    }

    func releaseReadiness() {
        let waits = state.withLock { state in
            state.isHolding = false
            defer { state.heldWaits = [] }
            return state.heldWaits
        }

        waits.forEach { $0.resume() }
    }
}
