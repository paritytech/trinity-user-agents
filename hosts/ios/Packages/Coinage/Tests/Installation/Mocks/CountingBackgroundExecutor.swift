import BackgroundExecution
import Foundation
import os

/// Runs operations inline, counts them, and can report the OS window as expired for the next few, the
/// way a real assertion does when the app stays folded.
final class CountingBackgroundExecutor: BackgroundExecuting, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (executions: 0, expiring: 0))

    var executions: Int { state.withLock { $0.executions } }

    func expireNextExecutions(_ count: Int) {
        state.withLock { $0.expiring = count }
    }

    func execute<T: Sendable>(_ operation: @Sendable () async throws -> T) async throws -> T {
        let expired = state.withLock { current -> Bool in
            current.executions += 1
            guard current.expiring > 0 else { return false }
            current.expiring -= 1
            return true
        }
        if expired {
            throw BackgroundExecutionExpiredError()
        }
        return try await operation()
    }
}
