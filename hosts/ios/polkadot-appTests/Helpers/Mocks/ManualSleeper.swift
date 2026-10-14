import Foundation

actor ManualSleeper {
    private typealias Waiter = CheckedContinuation<Void, Error>

    private var pendingAdvances = 0
    private var nextWaiterId = 0
    private var waiterOrder: [Int] = []
    private var waiters: [Int: Waiter] = [:]

    func sleep(for _: TimeInterval) async throws {
        try Task.checkCancellation()

        guard pendingAdvances == 0 else {
            pendingAdvances -= 1
            return
        }

        let waiterId = nextWaiterId
        nextWaiterId += 1

        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (continuation: Waiter) in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError())
                    return
                }

                waiterOrder.append(waiterId)
                waiters[waiterId] = continuation
            }
        } onCancel: {
            Task { await self.cancelWaiter(with: waiterId) }
        }

        try Task.checkCancellation()
    }

    func advance() {
        while !waiterOrder.isEmpty {
            let waiterId = waiterOrder.removeFirst()

            if let waiter = waiters.removeValue(forKey: waiterId) {
                waiter.resume()
                return
            }
        }

        pendingAdvances += 1
    }

    private func cancelWaiter(with id: Int) {
        guard let waiter = waiters.removeValue(forKey: id) else { return }

        waiterOrder.removeAll { $0 == id }
        waiter.resume(throwing: CancellationError())
    }
}
