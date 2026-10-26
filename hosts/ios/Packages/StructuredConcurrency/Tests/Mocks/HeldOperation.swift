import Foundation
import os

final class HeldOperation<Value: Sendable>: @unchecked Sendable {
    private struct State {
        var callCount = 0
        var held: [CheckedContinuation<Void, Never>] = []
        var isHolding = true
    }

    private let value: Value
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(returning value: Value) {
        self.value = value
    }

    var callCount: Int {
        state.withLock(\.callCount)
    }

    var operation: @Sendable () async -> Value {
        { [self] in
            await hold()
            return value
        }
    }

    func release() {
        let held = state.withLock { state in
            state.isHolding = false
            defer { state.held = [] }
            return state.held
        }

        held.forEach { $0.resume() }
    }
}

private extension HeldOperation {
    func hold() async {
        await withCheckedContinuation { continuation in
            let resumeNow = state.withLock { state -> Bool in
                state.callCount += 1
                guard state.isHolding else { return true }
                state.held.append(continuation)
                return false
            }

            if resumeNow {
                continuation.resume()
            }
        }
    }
}
