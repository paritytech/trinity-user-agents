@testable import polkadot_app

actor ManualDataChannelSendAttempts<Payload: Sendable & Equatable> {
    struct Call: Equatable {
        let payload: Payload
        let byteCount: UInt64
        let wasThrottled: Bool
    }

    private(set) var calls: [Call] = []
    private var resultContinuations: [CheckedContinuation<DataChannelSendAttemptResult, Never>] = []
    private var callCountWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []

    func attempt(
        payload: Payload,
        byteCount: UInt64,
        wasThrottled: Bool
    ) async -> DataChannelSendAttemptResult {
        calls.append(.init(
            payload: payload,
            byteCount: byteCount,
            wasThrottled: wasThrottled
        ))
        resumeSatisfiedCallCountWaiters()

        return await withCheckedContinuation { continuation in
            resultContinuations.append(continuation)
        }
    }

    func waitForCallCount(_ count: Int) async {
        guard calls.count < count else { return }

        await withCheckedContinuation { continuation in
            callCountWaiters.append((count, continuation))
        }
    }

    func resolveNext(with result: DataChannelSendAttemptResult) {
        precondition(!resultContinuations.isEmpty)
        resultContinuations.removeFirst().resume(returning: result)
    }

    private func resumeSatisfiedCallCountWaiters() {
        let satisfied = callCountWaiters.filter { calls.count >= $0.count }
        callCountWaiters.removeAll { calls.count >= $0.count }
        satisfied.forEach { $0.continuation.resume() }
    }
}
