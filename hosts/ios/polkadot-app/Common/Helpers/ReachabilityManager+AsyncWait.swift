import Foundation
import os
import StructuredConcurrency
import SubstrateSdk

extension ReachabilityManagerProtocol {
    func asyncWaitReachable() async throws {
        if isReachable { return }

        let waiter = ReachabilityWaiter()

        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                waiter.start(
                    manager: self,
                    guardian: CheckedContinuationGuard(continuation)
                )
            }
        } onCancel: {
            waiter.cancel()
        }
    }
}

private final class ReachabilityWaiter: ReachabilityListenerDelegate, @unchecked Sendable {
    private let managerLock = OSAllocatedUnfairLock<(any ReachabilityManagerProtocol)?>(initialState: nil)
    private var guardian: CheckedContinuationGuard<Void>?

    func start(
        manager: any ReachabilityManagerProtocol,
        guardian: CheckedContinuationGuard<Void>
    ) {
        self.guardian = guardian
        managerLock.withLock { $0 = manager }

        do {
            try manager.add(listener: self)
        } catch {
            finish(.failure(error))
            return
        }

        if manager.isReachable {
            finish(.success(()))
        }
    }

    func didChangeReachability(by manager: ReachabilityManagerProtocol) {
        guard manager.isReachable else { return }
        finish(.success(()))
    }

    func cancel() {
        finish(.failure(CancellationError()))
    }

    private func finish(_ result: Result<Void, Error>) {
        let manager = managerLock.withLock { value -> (any ReachabilityManagerProtocol)? in
            let current = value
            value = nil
            return current
        }

        manager?.remove(listener: self)
        guardian?.resume(result)
    }
}
